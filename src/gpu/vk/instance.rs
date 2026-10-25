//! A Vulkan instance: the devices it sees and what they are, destroyed
//! when dropped.
use super::{
	call::{Error, check, enumerate, fixed_string},
	choose::{Device, Kind, QueueFamily},
	entry::{Entry, cast},
	features::Features,
	types::*,
	version::major_minor,
};
use std::{ffi::CStr, ptr};

/// The instance's functions other than `vkDestroyInstance`.
struct Functions {
	enumerate_physical_devices: EnumeratePhysicalDevices,
	properties: GetPhysicalDeviceProperties,
	queue_families: GetPhysicalDeviceQueueFamilyProperties,
	extensions: EnumerateDeviceExtensionProperties,
	features: GetPhysicalDeviceFeatures2,
	create_device: CreateDevice,
	device_function: GetDeviceProcAddr,
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
				features: cast(function(c"vkGetPhysicalDeviceFeatures2")?),
				create_device: cast(function(c"vkCreateDevice")?),
				device_function: cast(function(c"vkGetDeviceProcAddr")?),
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
		let properties = self.properties(device);
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

impl Instance {
	fn properties(&self, device: RawPhysicalDevice) -> PhysicalDeviceProperties {
		let mut properties = PhysicalDeviceProperties::default();
		// SAFETY: the device is one of this live instance's, and `properties`
		// is writable.
		unsafe { (self.functions.properties)(device, &mut properties) };
		properties
	}

	/// A new device on `physical`, one of this instance's, with one queue
	/// of `family` and the features the renderer needs, or why not.
	pub(super) fn create_device(
		&self,
		physical: RawPhysicalDevice,
		family: u32,
	) -> Result<RawDevice, Error> {
		// Older devices know nothing of the 1.3 features.
		let api = self.properties(physical).api_version;
		if major_minor(api) < (1, 3) {
			return Err(Error::TooOld(api));
		}
		let mut supported = Features::none();
		// SAFETY: the device is live, and the chain is valid and writable.
		unsafe { (self.functions.features)(physical, &mut supported.head) };
		if let Some(feature) = supported.missing() {
			return Err(Error::Lacks(feature));
		}
		let enabled = Features::needed();
		let priority = 1.0;
		let queue = DeviceQueueCreateInfo {
			s_type: STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			queue_family_index: family,
			queue_count: 1,
			queue_priorities: &priority,
		};
		let info = DeviceCreateInfo {
			s_type: STRUCTURE_TYPE_DEVICE_CREATE_INFO,
			next: (&raw const enabled.head).cast(),
			flags: 0,
			queue_create_info_count: 1,
			queue_create_infos: &queue,
			enabled_layer_count: 0,
			enabled_layer_names: ptr::null(),
			enabled_extension_count: 0,
			enabled_extension_names: ptr::null(),
			enabled_features: ptr::null(),
		};
		let mut handle = ptr::null_mut();
		// SAFETY: `info` and all it points to outlive the call, and `handle`
		// is writable.
		check(unsafe { (self.functions.create_device)(physical, &info, ptr::null(), &mut handle) })?;
		Ok(handle)
	}

	/// The function `name` of `device`, which is live and made from this
	/// instance.
	pub(super) fn device_function(
		&self,
		device: RawDevice,
		name: &'static CStr,
	) -> Result<VoidFunction, Error> {
		// SAFETY: the driver's own function, given a live device and a C
		// string.
		unsafe { (self.functions.device_function)(device, name.as_ptr()) }.ok_or(Error::Missing(name))
	}
}

impl Drop for Instance {
	fn drop(&mut self) {
		// SAFETY: the instance is live and owns nothing still in use.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}
