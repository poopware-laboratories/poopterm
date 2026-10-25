//! A Vulkan instance: the devices it sees and what they are, destroyed
//! when dropped.
use super::{
	call::{Error, enumerate, fixed_string},
	choose::{Device, Kind, QueueFamily},
	entry::{Entry, cast},
	types::*,
};
use std::ptr;

/// The instance's functions other than `vkDestroyInstance`.
struct Functions {
	enumerate_physical_devices: EnumeratePhysicalDevices,
	properties: GetPhysicalDeviceProperties,
	queue_families: GetPhysicalDeviceQueueFamilyProperties,
	extensions: EnumerateDeviceExtensionProperties,
}

impl Functions {
	fn load(entry: &Entry, instance: RawInstance) -> Result<Self, Error> {
		let function = |name| entry.function(instance, name);
		// SAFETY: the functions the loader returned for these names have
		// these types.
		unsafe {
			Ok(Self {
				enumerate_physical_devices: cast(function(c"vkEnumeratePhysicalDevices")?),
				properties: cast(function(c"vkGetPhysicalDeviceProperties")?),
				queue_families: cast(function(c"vkGetPhysicalDeviceQueueFamilyProperties")?),
				extensions: cast(function(c"vkEnumerateDeviceExtensionProperties")?),
			})
		}
	}
}

pub(crate) struct Instance {
	handle: RawInstance,
	api: u32,
	destroy: DestroyInstance,
	functions: Functions,
	/// Dropped after the instance is destroyed, keeping the loader open.
	_entry: Entry,
}

impl Instance {
	/// Takes ownership of `handle`, created for version `api`.
	pub(super) fn new(entry: Entry, handle: RawInstance, api: u32) -> Result<Self, Error> {
		// A driver without it is broken, and the instance leaks.
		// SAFETY: the function the loader returned for this name has this type.
		let destroy: DestroyInstance = unsafe { cast(entry.function(handle, c"vkDestroyInstance")?) };
		match Functions::load(&entry, handle) {
			Ok(functions) => Ok(Self {
				handle,
				api,
				destroy,
				functions,
				_entry: entry,
			}),
			Err(error) => {
				// SAFETY: the instance is live, and nothing was made from it.
				unsafe { destroy(handle, ptr::null()) };
				Err(error)
			}
		}
	}

	/// The version the instance was created for.
	pub(crate) fn api_version(&self) -> u32 {
		self.api
	}

	pub(crate) fn physical_devices(&self) -> Result<Vec<RawPhysicalDevice>, Error> {
		enumerate(ptr::null_mut(), |count, devices| {
			// SAFETY: the instance is live, and `enumerate` passes room for
			// `count` devices.
			unsafe { (self.functions.enumerate_physical_devices)(self.handle, count, devices) }
		})
	}

	/// What `device`, one of [`Self::physical_devices`], is and offers; it
	/// shows nothing yet, as no family is checked against a surface.
	pub(crate) fn describe(&self, device: RawPhysicalDevice) -> Result<Device, Error> {
		let mut properties = PhysicalDeviceProperties::default();
		// SAFETY: the device is one of this live instance's, and `properties`
		// is writable.
		unsafe { (self.functions.properties)(device, &mut properties) };
		let families = enumerate(QueueFamilyProperties::default(), |count, families| {
			// SAFETY: as above, with room for `count` families.
			unsafe { (self.functions.queue_families)(device, count, families) };
			0
		})?;
		let none = ExtensionProperties {
			extension_name: [0; 256],
			spec_version: 0,
		};
		let extensions = enumerate(none, |count, extensions| {
			// SAFETY: as above, with room for `count` extensions, and no layer.
			unsafe { (self.functions.extensions)(device, ptr::null(), count, extensions) }
		})?;
		Ok(Device {
			name: fixed_string(&properties.device_name),
			kind: Kind::from_type(properties.device_type),
			api: properties.api_version,
			families: (families.iter())
				.map(|family| QueueFamily {
					flags: family.queue_flags,
					count: family.queue_count,
					present: false,
				})
				.collect(),
			swapchain: (extensions.iter())
				.any(|extension| fixed_string(&extension.extension_name) == "VK_KHR_swapchain"),
		})
	}
}

impl Drop for Instance {
	fn drop(&mut self) {
		// SAFETY: the instance is live and owns nothing still in use.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}
