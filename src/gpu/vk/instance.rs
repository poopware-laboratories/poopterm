//! A Vulkan instance: the devices it sees and what they are, destroyed
//! when dropped.
use super::{
	call::{Error, check, enumerate, fixed_string},
	choose::{Device, Kind, QueueFamily},
	driver_info::{
		DriverProperties, GetPhysicalDeviceProperties2, PhysicalDeviceProperties2,
		STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,
	},
	entry::{Entry, cast, load, load_optional},
	features::Features,
	memory::MemoryType,
	memory_budget::{
		GetPhysicalDeviceMemoryProperties2, MemoryBudget, MemoryProperties2,
		STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2,
	},
	surface::Platform,
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
	memory: GetPhysicalDeviceMemoryProperties,
	features: GetPhysicalDeviceFeatures2,
	create_device: CreateDevice,
	device_function: GetDeviceProcAddr,
	/// Only for the driver's facts: without it there are none.
	properties2: Option<GetPhysicalDeviceProperties2>,
	/// Only for memory budgets: without it there are none.
	memory2: Option<GetPhysicalDeviceMemoryProperties2>,
}

impl Functions {
	fn load(entry: &Entry, instance: RawInstance) -> Result<Self, Error> {
		let function = |name| entry.function(instance, name);
		Ok(Self {
			enumerate_physical_devices: load!(function, c"vkEnumeratePhysicalDevices"),
			properties: load!(function, c"vkGetPhysicalDeviceProperties"),
			queue_families: load!(function, c"vkGetPhysicalDeviceQueueFamilyProperties"),
			extensions: load!(function, c"vkEnumerateDeviceExtensionProperties"),
			memory: load!(function, c"vkGetPhysicalDeviceMemoryProperties"),
			features: load!(function, c"vkGetPhysicalDeviceFeatures2"),
			create_device: load!(function, c"vkCreateDevice"),
			device_function: load!(function, c"vkGetDeviceProcAddr"),
			properties2: load_optional!(function, c"vkGetPhysicalDeviceProperties2"),
			memory2: load_optional!(function, c"vkGetPhysicalDeviceMemoryProperties2"),
		})
	}
}

pub(crate) struct Instance {
	handle: RawInstance,
	api: u32,
	/// The window system it makes surfaces for, if any.
	pub(super) platform: Option<Platform>,
	destroy: DestroyInstance,
	functions: Functions,
	/// Dropped after the instance is destroyed, keeping the loader open.
	entry: Entry,
}

impl Instance {
	/// Takes ownership of `handle`, created for version `api` and with the
	/// extensions of `platform`.
	pub(super) fn new(
		entry: Entry,
		handle: RawInstance,
		api: u32,
		platform: Option<Platform>,
	) -> Result<Self, Error> {
		// A driver without it is broken, and the instance leaks.
		// SAFETY: the function the loader returned for this name has this type.
		let destroy: DestroyInstance = unsafe { cast(entry.function(handle, c"vkDestroyInstance")?) };
		match Functions::load(&entry, handle) {
			Ok(functions) => Ok(Self {
				handle,
				api,
				platform,
				destroy,
				functions,
				entry,
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

	pub(super) fn handle(&self) -> RawInstance {
		self.handle
	}

	/// The instance function `name`.
	pub(super) fn function(&self, name: &'static CStr) -> Result<VoidFunction, Error> {
		self.entry.function(self.handle, name)
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
		let extensions = self.device_extensions(device)?;
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
			swapchain: extensions.iter().any(|name| name == "VK_KHR_swapchain"),
		})
	}

	/// The names of the extensions `device`, one of
	/// [`Self::physical_devices`], offers.
	pub(crate) fn device_extensions(&self, device: RawPhysicalDevice) -> Result<Vec<String>, Error> {
		let none = ExtensionProperties {
			extension_name: [0; 256],
			spec_version: 0,
		};
		let extensions = enumerate(none, |count, extensions| {
			// SAFETY: the device is one of this live instance's, `enumerate`
			// passes room for `count` extensions, and no layer is named.
			unsafe { (self.functions.extensions)(device, ptr::null(), count, extensions) }
		})?;
		Ok(
			extensions
				.iter()
				.map(|extension| fixed_string(&extension.extension_name))
				.collect(),
		)
	}

	/// How many bits of the timestamps of `family` of `device`, one of
	/// [`Self::physical_devices`], count; none when it has none.
	pub(crate) fn timestamp_bits(&self, device: RawPhysicalDevice, family: u32) -> u32 {
		let families = enumerate(QueueFamilyProperties::default(), |count, families| {
			// SAFETY: the device is one of this live instance's, with room
			// for `count` families.
			unsafe { (self.functions.queue_families)(device, count, families) };
			0
		});
		let family = families
			.ok()
			.and_then(|all| all.get(family as usize).copied());
		family.map_or(0, |family| family.timestamp_valid_bits)
	}

	/// How the driver of `device`, one of [`Self::physical_devices`], names
	/// itself, when it says.
	pub(crate) fn driver(&self, device: RawPhysicalDevice) -> Option<String> {
		let get = self.functions.properties2?;
		let mut driver = DriverProperties::empty();
		let mut properties = PhysicalDeviceProperties2 {
			s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,
			next: (&raw mut driver).cast(),
			properties: PhysicalDeviceProperties::default(),
		};
		// SAFETY: the device is one of this live instance's, and both
		// structures are writable, chained as the specification allows, and
		// outlive the call.
		unsafe { get(device, &mut properties) };
		driver.text()
	}

	/// The bytes `heap` of `device`, one offering `VK_EXT_memory_budget`,
	/// has free for this program, when the driver says.
	pub(crate) fn heap_free(&self, device: RawPhysicalDevice, heap: u32) -> Option<u64> {
		let get = self.functions.memory2?;
		let mut budget = MemoryBudget::empty();
		let mut properties = MemoryProperties2 {
			s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2,
			next: (&raw mut budget).cast(),
			properties: MemoryProperties::default(),
		};
		// SAFETY: the device is one of this live instance's and offers the
		// extension, and both structures are writable, chained as the
		// specification allows, and outlive the call.
		unsafe { get(device, &mut properties) };
		budget.free(heap as usize)
	}

	pub(super) fn properties(&self, device: RawPhysicalDevice) -> PhysicalDeviceProperties {
		let mut properties = PhysicalDeviceProperties::default();
		// SAFETY: the device is one of this live instance's, and `properties`
		// is writable.
		unsafe { (self.functions.properties)(device, &mut properties) };
		properties
	}

	/// Whether `physical`, one of this instance's, has the feature of the
	/// one-flag structure of type `s_type`, an extension's it offers.
	pub(crate) fn has_feature(&self, physical: RawPhysicalDevice, s_type: i32) -> bool {
		let mut asked = Features::none_with(&[s_type]);
		// SAFETY: the device is live, and the chain is valid and writable.
		unsafe { (self.functions.features)(physical, &mut asked.head) };
		asked.extra[0].enabled != 0
	}

	/// A new device on `physical`, one of this instance's, with one queue
	/// of `family`, the features the renderer needs and `extensions`, with
	/// the one-flag feature structures of types `features`, or why not.
	pub(super) fn create_device(
		&self,
		physical: RawPhysicalDevice,
		family: u32,
		extensions: &[&'static CStr],
		features: &[i32],
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
		let enabled = Features::needed(features);
		let names: Vec<_> = extensions.iter().map(|name| name.as_ptr()).collect();
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
			enabled_extension_count: names.len() as u32,
			enabled_extension_names: names.as_ptr(),
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

	/// The memory types of `device`, one of [`Self::physical_devices`], in
	/// the driver's order, which [`choose_memory`] picks from by index.
	///
	/// [`choose_memory`]: super::memory::choose_memory
	pub(crate) fn memory_types(&self, device: RawPhysicalDevice) -> Vec<MemoryType> {
		let mut memory = MemoryProperties::default();
		// SAFETY: the device is one of this live instance's, and `memory` is
		// writable.
		unsafe { (self.functions.memory)(device, &mut memory) };
		// A broken driver may claim more than there is room for.
		let count = (memory.type_count as usize).min(memory.types.len());
		memory.types[..count].to_vec()
	}
}

impl Drop for Instance {
	fn drop(&mut self) {
		// SAFETY: the instance is live and owns nothing still in use.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}

#[cfg(test)]
#[path = "instance_table_tests.rs"]
mod table_tests;
