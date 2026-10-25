//! A Vulkan instance: the devices it sees, destroyed when dropped.
use super::{
	call::{Error, enumerate},
	entry::{Entry, cast},
	types::*,
};
use std::ptr;

pub(crate) struct Instance {
	handle: RawInstance,
	api: u32,
	destroy: DestroyInstance,
	enumerate_physical_devices: EnumeratePhysicalDevices,
	/// Dropped after the instance is destroyed, keeping the loader open.
	_entry: Entry,
}

impl Instance {
	/// Takes ownership of `handle`, created for version `api`.
	pub(super) fn new(entry: Entry, handle: RawInstance, api: u32) -> Result<Self, Error> {
		// A driver without it is broken, and the instance leaks.
		// SAFETY: the function the loader returned for this name has this type.
		let destroy: DestroyInstance = unsafe { cast(entry.function(handle, c"vkDestroyInstance")?) };
		let enumerate = match entry.function(handle, c"vkEnumeratePhysicalDevices") {
			// SAFETY: as above.
			Ok(function) => unsafe { cast(function) },
			Err(error) => {
				// SAFETY: the instance is live, and nothing was made from it.
				unsafe { destroy(handle, ptr::null()) };
				return Err(error);
			}
		};
		Ok(Self {
			handle,
			api,
			destroy,
			enumerate_physical_devices: enumerate,
			_entry: entry,
		})
	}

	/// The version the instance was created for.
	pub(crate) fn api_version(&self) -> u32 {
		self.api
	}

	pub(crate) fn physical_devices(&self) -> Result<Vec<RawPhysicalDevice>, Error> {
		enumerate(ptr::null_mut(), |count, devices| {
			// SAFETY: the instance is live, and `enumerate` passes room for
			// `count` devices.
			unsafe { (self.enumerate_physical_devices)(self.handle, count, devices) }
		})
	}
}

impl Drop for Instance {
	fn drop(&mut self) {
		// SAFETY: the instance is live and owns nothing still in use.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}
