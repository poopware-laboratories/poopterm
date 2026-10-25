//! A Vulkan device: a GPU with the features the renderer needs turned on,
//! the queue it draws with and its memory types, destroyed when dropped.
use super::{call::Error, entry::cast, instance::Instance, memory::MemoryType, types::*};
use std::{ffi::CStr, ptr, rc::Rc};

/// The device's functions other than `vkDestroyDevice`.
pub(super) struct Functions {
	pub get_queue: GetDeviceQueue,
	pub create_buffer: CreateBuffer,
	pub destroy_buffer: DestroyBuffer,
	pub buffer_requirements: GetBufferMemoryRequirements,
	pub allocate: AllocateMemory,
	pub free: FreeMemory,
	pub bind_buffer: BindBufferMemory,
	pub map: MapMemory,
}

impl Functions {
	fn load(instance: &Instance, device: RawDevice) -> Result<Self, Error> {
		let function = |name: &'static CStr| instance.device_function(device, name);
		// SAFETY: the functions the driver returned for these names have
		// these types.
		unsafe {
			Ok(Self {
				get_queue: cast(function(c"vkGetDeviceQueue")?),
				create_buffer: cast(function(c"vkCreateBuffer")?),
				destroy_buffer: cast(function(c"vkDestroyBuffer")?),
				buffer_requirements: cast(function(c"vkGetBufferMemoryRequirements")?),
				allocate: cast(function(c"vkAllocateMemory")?),
				free: cast(function(c"vkFreeMemory")?),
				bind_buffer: cast(function(c"vkBindBufferMemory")?),
				map: cast(function(c"vkMapMemory")?),
			})
		}
	}
}

pub(crate) struct Device {
	pub(super) handle: RawDevice,
	pub(super) functions: Functions,
	/// The memory types of the GPU, which allocations name by index.
	pub(super) memory: Vec<MemoryType>,
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
		let functions = match Functions::load(&instance, handle) {
			Ok(functions) => functions,
			Err(error) => {
				// SAFETY: the device is live, and nothing was made from it.
				unsafe { destroy(handle, ptr::null()) };
				return Err(error);
			}
		};
		let mut queue = ptr::null_mut();
		// SAFETY: the device was made with one queue of `family`, and `queue`
		// is writable.
		unsafe { (functions.get_queue)(handle, family, 0, &mut queue) };
		Ok(Self {
			handle,
			functions,
			memory: instance.memory_types(physical),
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
		// SAFETY: the device is live, and what was made from it holds an `Rc`
		// of it, so is gone.
		unsafe { (self.destroy)(self.handle, ptr::null()) };
	}
}
