//! A Vulkan device: a GPU with the features the renderer needs turned on,
//! and the queue it draws with, destroyed when dropped.
use super::{call::Error, entry::cast, instance::Instance, types::*};
use std::{ptr, rc::Rc};

pub(crate) struct Device {
	handle: RawDevice,
	queue: RawQueue,
	family: u32,
	destroy: DestroyDevice,
	/// Dropped after the device is destroyed, as it was made from it.
	_instance: Rc<Instance>,
}

impl Device {
	/// A device on `physical`, one of the instance's, with one queue of
	/// `family`.
	pub(crate) fn new(
		instance: Rc<Instance>,
		physical: RawPhysicalDevice,
		family: u32,
	) -> Result<Self, Error> {
		let handle = instance.create_device(physical, family)?;
		// A driver without it is broken, and the device leaks.
		// SAFETY: the function the driver returned for this name has this type.
		let destroy: DestroyDevice =
			unsafe { cast(instance.device_function(handle, c"vkDestroyDevice")?) };
		let get_queue: GetDeviceQueue = match instance.device_function(handle, c"vkGetDeviceQueue") {
			// SAFETY: as above.
			Ok(function) => unsafe { cast(function) },
			Err(error) => {
				// SAFETY: the device is live, and nothing was made from it.
				unsafe { destroy(handle, ptr::null()) };
				return Err(error);
			}
		};
		let mut queue = ptr::null_mut();
		// SAFETY: the device was made with one queue of `family`, and `queue`
		// is writable.
		unsafe { get_queue(handle, family, 0, &mut queue) };
		Ok(Self {
			handle,
			queue,
			family,
			destroy,
			_instance: instance,
		})
	}

	pub(crate) fn queue(&self) -> RawQueue {
		self.queue
	}

	pub(crate) fn family(&self) -> u32 {
		self.family
	}
}

impl Drop for Device {
	fn drop(&mut self) {
		// SAFETY: the device is live and owns nothing still in use.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}
