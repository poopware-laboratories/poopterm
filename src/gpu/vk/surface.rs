//! Windows to draw into: the instance extensions a window system needs, a
//! surface made from a window's handles and destroyed on drop, and which
//! queue families can show on it.
use super::{
	call::{Error, check, enumerate},
	choose::Device,
	entry::cast,
	instance::Instance,
	surface_types::*,
	swapchain_plan::{Capabilities, Plan, SurfaceFormat, plan},
	types::RawPhysicalDevice,
};
use std::{
	ffi::{CStr, c_ulong, c_void},
	ptr::{self, NonNull},
	rc::Rc,
};

pub(crate) const SURFACE: &CStr = c"VK_KHR_surface";

/// A window system Vulkan can show in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
	Wayland,
	Xlib,
	/// No window system: surfaces that show nothing, which test drawing
	/// to a window against real drivers.
	Headless,
}

impl Platform {
	/// The instance extensions that make surfaces for its windows.
	pub(crate) fn extensions(self) -> [&'static CStr; 2] {
		let own = match self {
			Self::Wayland => c"VK_KHR_wayland_surface",
			Self::Xlib => c"VK_KHR_xlib_surface",
			Self::Headless => c"VK_EXT_headless_surface",
		};
		[SURFACE, own]
	}
}

/// A window's handles, as its window system gives them.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Window {
	Wayland {
		/// A `wl_display`.
		display: NonNull<c_void>,
		/// A `wl_surface`.
		surface: NonNull<c_void>,
	},
	Xlib {
		/// An Xlib `Display`.
		display: NonNull<c_void>,
		window: c_ulong,
	},
	Headless,
}

impl Window {
	pub(crate) fn platform(self) -> Platform {
		match self {
			Self::Wayland { .. } => Platform::Wayland,
			Self::Xlib { .. } => Platform::Xlib,
			Self::Headless => Platform::Headless,
		}
	}
}

/// The instance's functions for surfaces, other than making them.
struct Functions {
	destroy: DestroySurface,
	support: GetPhysicalDeviceSurfaceSupport,
	capabilities: GetPhysicalDeviceSurfaceCapabilities,
	formats: GetPhysicalDeviceSurfaceFormats,
	modes: GetPhysicalDeviceSurfacePresentModes,
}

impl Functions {
	fn load(instance: &Instance) -> Result<Self, Error> {
		let function = |name| instance.function(name);
		// SAFETY: the functions the loader returned for these names have
		// these types.
		unsafe {
			Ok(Self {
				destroy: cast(function(c"vkDestroySurfaceKHR")?),
				support: cast(function(c"vkGetPhysicalDeviceSurfaceSupportKHR")?),
				capabilities: cast(function(c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR")?),
				formats: cast(function(c"vkGetPhysicalDeviceSurfaceFormatsKHR")?),
				modes: cast(function(c"vkGetPhysicalDeviceSurfacePresentModesKHR")?),
			})
		}
	}
}

pub(crate) struct Surface {
	handle: RawSurface,
	functions: Functions,
	/// Dropped after the surface is destroyed.
	pub(super) instance: Rc<Instance>,
}

impl Surface {
	/// A surface showing `window`, made with `instance`, which must have
	/// been made for its platform.
	///
	/// # Safety
	///
	/// The window's handles are live, and stay live until the surface is
	/// dropped.
	pub(crate) unsafe fn new(instance: &Rc<Instance>, window: Window) -> Result<Self, Error> {
		let platform = window.platform();
		if instance.platform != Some(platform) {
			return Err(Error::Missing(platform.extensions()[1]));
		}
		let functions = Functions::load(instance)?;
		let (wayland, xlib, headless);
		let (name, info): (&'static CStr, *const c_void) = match window {
			Window::Wayland { display, surface } => {
				wayland = WaylandSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
					display: display.as_ptr(),
					surface: surface.as_ptr(),
				};
				(c"vkCreateWaylandSurfaceKHR", (&raw const wayland).cast())
			}
			Window::Xlib { display, window } => {
				xlib = XlibSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
					display: display.as_ptr(),
					window,
				};
				(c"vkCreateXlibSurfaceKHR", (&raw const xlib).cast())
			}
			Window::Headless => {
				headless = HeadlessSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_HEADLESS_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
				};
				(c"vkCreateHeadlessSurfaceEXT", (&raw const headless).cast())
			}
		};
		// SAFETY: the function the loader returned for this name has this
		// type but for the pointee of its info, and pointers to sized types
		// are passed alike.
		let create: CreateSurface = unsafe { cast(instance.function(name)?) };
		let mut handle = 0;
		// SAFETY: the instance is live, `info` is the structure `create`
		// takes and outlives the call, the caller vouches for the handles,
		// and `handle` is writable.
		check(unsafe { create(instance.handle(), info, ptr::null(), &mut handle) })?;
		Ok(Self {
			handle,
			functions,
			instance: instance.clone(),
		})
	}

	/// Marks which queue families of `device`, the description of
	/// `physical`, can show on this surface.
	pub(crate) fn mark_present(
		&self,
		physical: RawPhysicalDevice,
		device: &mut Device,
	) -> Result<(), Error> {
		for (index, family) in device.families.iter_mut().enumerate() {
			let mut supported = 0;
			// SAFETY: `physical` is one of the instance's devices, `index` one
			// of its families, the surface is live and `supported` writable.
			check(unsafe {
				(self.functions.support)(physical, index as u32, self.handle, &mut supported)
			})?;
			family.present = supported != 0;
		}
		Ok(())
	}

	/// The swapchain to make on `physical`, one of the instance's devices,
	/// for a window of `window` pixels; `Error::Unfit` says why none suits.
	pub(crate) fn plan(&self, physical: RawPhysicalDevice, window: [u32; 2]) -> Result<Plan, Error> {
		let (functions, handle) = (&self.functions, self.handle);
		let mut capabilities = Capabilities::default();
		// SAFETY: `physical` is one of the instance's devices, the surface is
		// live and `capabilities` writable.
		check(unsafe { (functions.capabilities)(physical, handle, &mut capabilities) })?;
		let none = SurfaceFormat {
			format: 0,
			color_space: 0,
		};
		let formats = enumerate(none, |count, formats| {
			// SAFETY: as above, and `enumerate` passes room for `count`.
			unsafe { (functions.formats)(physical, handle, count, formats) }
		})?;
		let modes = enumerate(0, |count, modes| {
			// SAFETY: as above.
			unsafe { (functions.modes)(physical, handle, count, modes) }
		})?;
		plan(&capabilities, &formats, &modes, window).map_err(Error::Unfit)
	}
}

impl Drop for Surface {
	fn drop(&mut self) {
		// SAFETY: the surface is live, made with this live instance, and
		// nothing made from it is left.
		unsafe { (self.functions.destroy)(self.instance.handle(), self.handle, ptr::null()) };
	}
}
