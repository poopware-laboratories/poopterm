//! A window's swapchain: the images its surface shows, made to a plan with
//! a view each, remade when the window changes, destroyed when dropped.
//! Acquiring and presenting its images is in `present`.
use super::{
	call::{Error, check, enumerate},
	device::{Device, SWAPCHAIN},
	entry::cast,
	image::Image,
	present::Acquire,
	semaphore::Semaphore,
	surface::Surface,
	swapchain_plan::{Plan, SRGB_NONLINEAR, USAGE},
	swapchain_types::*,
};
use std::{ffi::CStr, ptr, rc::Rc};

/// The device's swapchain functions.
#[derive(Clone, Copy)]
pub(super) struct Functions {
	create: CreateSwapchain,
	destroy: DestroySwapchain,
	images: GetSwapchainImages,
	pub acquire: AcquireNextImage,
	pub present: QueuePresent,
	wait_idle: QueueWaitIdle,
}

impl Functions {
	fn load(device: &Device) -> Result<Self, Error> {
		let function = |name: &'static CStr| device.instance.device_function(device.handle, name);
		// SAFETY: the functions the driver returned for these names have
		// these types.
		unsafe {
			Ok(Self {
				create: cast(function(c"vkCreateSwapchainKHR")?),
				destroy: cast(function(c"vkDestroySwapchainKHR")?),
				images: cast(function(c"vkGetSwapchainImagesKHR")?),
				acquire: cast(function(c"vkAcquireNextImageKHR")?),
				present: cast(function(c"vkQueuePresentKHR")?),
				wait_idle: cast(function(c"vkQueueWaitIdle")?),
			})
		}
	}
}

/// The swapchain itself, destroyed when dropped; the views of its images
/// hold it, so it goes after them.
pub(super) struct Chain {
	pub handle: RawSwapchain,
	destroy: DestroySwapchain,
	pub device: Rc<Device>,
	/// Destroyed after the swapchain, which shows on it.
	_surface: Rc<Surface>,
}

impl Drop for Chain {
	fn drop(&mut self) {
		// SAFETY: the swapchain is the device's, and the views of its images
		// are gone.
		unsafe { (self.destroy)(self.device.handle, self.handle, ptr::null()) };
	}
}

/// Dropped once the queue is idle, so the GPU and the presentation engine
/// are done with its images and semaphores.
pub(crate) struct Swapchain {
	pub(super) functions: Functions,
	plan: Plan,
	/// In the order the swapchain lists them, which acquiring indexes.
	images: Vec<Rc<Image>>,
	/// One for each image, signalled when its frame is drawn and waited for
	/// by its presentation; the image is acquired again only after that
	/// wait, so the semaphore is free whenever its image is.
	pub(super) drawn: Vec<Semaphore>,
	/// Signalled when an image is acquired, and waited for by the frame
	/// drawn into it.
	pub(super) acquires: Vec<Acquire>,
	pub(super) chain: Rc<Chain>,
}

impl Swapchain {
	/// A swapchain of `device`, made for windows, showing on `surface` as
	/// `plan` says, which the surface reported suits it.
	pub(crate) fn new(device: &Rc<Device>, surface: &Rc<Surface>, plan: Plan) -> Result<Self, Error> {
		if !device.swapchain {
			return Err(Error::Missing(SWAPCHAIN));
		}
		if !Rc::ptr_eq(&device.instance, &surface.instance) {
			return Err(Error::Foreign);
		}
		if !surface.shows(device.physical, device.family())? {
			return Err(Error::Unshown);
		}
		let functions = Functions::load(device)?;
		Self::make(functions, device, surface, plan, 0)
	}

	/// The swapchain remade to `plan`, as when the window changes size; the
	/// old one is retired, even if that fails, and destroyed once the queue
	/// is idle.
	pub(crate) fn recreate(self, plan: Plan) -> Result<Self, Error> {
		let (device, surface) = (&self.chain.device, &self.chain._surface);
		Self::make(self.functions, device, surface, plan, self.chain.handle)
	}

	fn make(
		functions: Functions,
		device: &Rc<Device>,
		surface: &Rc<Surface>,
		plan: Plan,
		old: RawSwapchain,
	) -> Result<Self, Error> {
		let info = SwapchainCreateInfo {
			s_type: STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			surface: surface.handle(),
			min_image_count: plan.images,
			image_format: plan.format as i32,
			image_color_space: SRGB_NONLINEAR,
			image_extent: plan.extent,
			image_array_layers: 1,
			image_usage: USAGE,
			image_sharing_mode: 0,
			queue_family_index_count: 0,
			queue_family_indices: ptr::null(),
			pre_transform: plan.transform,
			composite_alpha: plan.alpha,
			present_mode: plan.mode,
			clipped: 1,
			old_swapchain: old,
		};
		let mut handle = 0;
		// SAFETY: the device is live and made with the extension, `info`
		// outlives the call and names its live surface, which its queue shows
		// on, and `handle` is writable.
		check(unsafe { (functions.create)(device.handle, &info, ptr::null(), &mut handle) })?;
		let chain = Rc::new(Chain {
			handle,
			destroy: functions.destroy,
			device: device.clone(),
			_surface: surface.clone(),
		});
		let handles = enumerate(0, |count, images| {
			// SAFETY: the swapchain is live, and `enumerate` passes room for
			// `count`.
			unsafe { (functions.images)(device.handle, handle, count, images) }
		})?;
		let made = (plan.format, plan.extent, USAGE);
		let images = (handles.iter())
			.map(|&image| Image::presented(device, image, made, chain.clone()).map(Rc::new))
			.collect::<Result<_, _>>()?;
		let drawn = (handles.iter())
			.map(|_| Semaphore::new(device))
			.collect::<Result<_, _>>()?;
		Ok(Self {
			functions,
			plan,
			images,
			drawn,
			acquires: Vec::new(),
			chain,
		})
	}

	pub(crate) fn plan(&self) -> &Plan {
		&self.plan
	}

	pub(crate) fn images(&self) -> &[Rc<Image>] {
		&self.images
	}
}

impl Drop for Swapchain {
	fn drop(&mut self) {
		let device = &self.chain.device;
		// A lost device has finished everything.
		// SAFETY: the queue is the device's and used from this thread alone.
		let _ = unsafe { (self.functions.wait_idle)(device.queue()) };
	}
}
