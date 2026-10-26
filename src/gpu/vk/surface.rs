//! Windows to draw into: the instance extensions a window system needs, a
//! surface made from a window's handles and destroyed on drop, and which
//! queue families can show on it.
use super::{
	call::{Error, check},
	choose::Device,
	entry::cast,
	instance::Instance,
	surface_types::*,
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

pub(crate) struct Surface {
	handle: RawSurface,
	destroy: DestroySurface,
	support: GetPhysicalDeviceSurfaceSupport,
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
		let function = |name| instance.function(name);
		// SAFETY: the functions the loader returned for these names have
		// these types.
		let (destroy, support) = unsafe {
			(
				cast::<DestroySurface>(function(c"vkDestroySurfaceKHR")?),
				cast::<GetPhysicalDeviceSurfaceSupport>(function(c"vkGetPhysicalDeviceSurfaceSupportKHR")?),
			)
		};
		let mut handle = 0;
		let code = match window {
			Window::Wayland { display, surface } => {
				// SAFETY: as above.
				let create: CreateWaylandSurface = unsafe { cast(function(c"vkCreateWaylandSurfaceKHR")?) };
				let info = WaylandSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
					display: display.as_ptr(),
					surface: surface.as_ptr(),
				};
				// SAFETY: the instance is live, `info` outlives the call, the
				// caller vouches for the handles, and `handle` is writable.
				unsafe { create(instance.handle(), &info, ptr::null(), &mut handle) }
			}
			Window::Xlib { display, window } => {
				// SAFETY: as above.
				let create: CreateXlibSurface = unsafe { cast(function(c"vkCreateXlibSurfaceKHR")?) };
				let info = XlibSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
					display: display.as_ptr(),
					window,
				};
				// SAFETY: as above.
				unsafe { create(instance.handle(), &info, ptr::null(), &mut handle) }
			}
			Window::Headless => {
				// SAFETY: as above.
				let create: CreateHeadlessSurface =
					unsafe { cast(function(c"vkCreateHeadlessSurfaceEXT")?) };
				let info = HeadlessSurfaceCreateInfo {
					s_type: STRUCTURE_TYPE_HEADLESS_SURFACE_CREATE_INFO,
					next: ptr::null(),
					flags: 0,
				};
				// SAFETY: the instance is live, `info` outlives the call, and
				// `handle` is writable.
				unsafe { create(instance.handle(), &info, ptr::null(), &mut handle) }
			}
		};
		check(code)?;
		Ok(Self {
			handle,
			destroy,
			support,
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
			check(unsafe { (self.support)(physical, index as u32, self.handle, &mut supported) })?;
			family.present = supported != 0;
		}
		Ok(())
	}
}

impl Drop for Surface {
	fn drop(&mut self) {
		// SAFETY: the surface is live, made with this live instance, and
		// nothing made from it is left.
		unsafe { (self.destroy)(self.instance.handle(), self.handle, ptr::null()) };
	}
}
