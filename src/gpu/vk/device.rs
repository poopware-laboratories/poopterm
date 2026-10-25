//! A Vulkan device: a GPU with the features the renderer needs turned on,
//! the queue it draws with and its memory types, destroyed when dropped.
use super::{
	call::Error, descriptor_types::*, entry::cast, instance::Instance, memory::MemoryType,
	pipeline_types::*, submit_types::*, types::*,
};
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
	pub create_image: CreateImage,
	pub destroy_image: DestroyImage,
	pub image_requirements: GetImageMemoryRequirements,
	pub bind_image: BindImageMemory,
	pub create_view: CreateImageView,
	pub destroy_view: DestroyImageView,
	pub create_semaphore: CreateSemaphore,
	pub destroy_semaphore: DestroySemaphore,
	pub semaphore_value: GetSemaphoreCounterValue,
	pub wait_semaphores: WaitSemaphores,
	pub create_pool: CreateCommandPool,
	pub destroy_pool: DestroyCommandPool,
	pub reset_pool: ResetCommandPool,
	pub allocate_commands: AllocateCommandBuffers,
	pub begin: BeginCommandBuffer,
	pub end: EndCommandBuffer,
	pub submit: QueueSubmit2,
	pub barrier: CmdPipelineBarrier2,
	pub upload: CmdCopyBufferToImage,
	pub download: CmdCopyImageToBuffer,
	pub create_module: CreateShaderModule,
	pub destroy_module: DestroyShaderModule,
	pub create_sampler: CreateSampler,
	pub destroy_sampler: DestroySampler,
	pub create_set_layout: CreateDescriptorSetLayout,
	pub destroy_set_layout: DestroyDescriptorSetLayout,
	pub create_pipeline_layout: CreatePipelineLayout,
	pub destroy_pipeline_layout: DestroyPipelineLayout,
	pub create_pipelines: CreateGraphicsPipelines,
	pub destroy_pipeline: DestroyPipeline,
	pub create_descriptor_pool: CreateDescriptorPool,
	pub destroy_descriptor_pool: DestroyDescriptorPool,
	pub allocate_descriptor_sets: AllocateDescriptorSets,
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
				create_image: cast(function(c"vkCreateImage")?),
				destroy_image: cast(function(c"vkDestroyImage")?),
				image_requirements: cast(function(c"vkGetImageMemoryRequirements")?),
				bind_image: cast(function(c"vkBindImageMemory")?),
				create_view: cast(function(c"vkCreateImageView")?),
				destroy_view: cast(function(c"vkDestroyImageView")?),
				create_semaphore: cast(function(c"vkCreateSemaphore")?),
				destroy_semaphore: cast(function(c"vkDestroySemaphore")?),
				semaphore_value: cast(function(c"vkGetSemaphoreCounterValue")?),
				wait_semaphores: cast(function(c"vkWaitSemaphores")?),
				create_pool: cast(function(c"vkCreateCommandPool")?),
				destroy_pool: cast(function(c"vkDestroyCommandPool")?),
				reset_pool: cast(function(c"vkResetCommandPool")?),
				allocate_commands: cast(function(c"vkAllocateCommandBuffers")?),
				begin: cast(function(c"vkBeginCommandBuffer")?),
				end: cast(function(c"vkEndCommandBuffer")?),
				submit: cast(function(c"vkQueueSubmit2")?),
				barrier: cast(function(c"vkCmdPipelineBarrier2")?),
				upload: cast(function(c"vkCmdCopyBufferToImage")?),
				download: cast(function(c"vkCmdCopyImageToBuffer")?),
				create_module: cast(function(c"vkCreateShaderModule")?),
				destroy_module: cast(function(c"vkDestroyShaderModule")?),
				create_sampler: cast(function(c"vkCreateSampler")?),
				destroy_sampler: cast(function(c"vkDestroySampler")?),
				create_set_layout: cast(function(c"vkCreateDescriptorSetLayout")?),
				destroy_set_layout: cast(function(c"vkDestroyDescriptorSetLayout")?),
				create_pipeline_layout: cast(function(c"vkCreatePipelineLayout")?),
				destroy_pipeline_layout: cast(function(c"vkDestroyPipelineLayout")?),
				create_pipelines: cast(function(c"vkCreateGraphicsPipelines")?),
				destroy_pipeline: cast(function(c"vkDestroyPipeline")?),
				create_descriptor_pool: cast(function(c"vkCreateDescriptorPool")?),
				destroy_descriptor_pool: cast(function(c"vkDestroyDescriptorPool")?),
				allocate_descriptor_sets: cast(function(c"vkAllocateDescriptorSets")?),
			})
		}
	}
}

pub(crate) struct Device {
	pub(super) handle: RawDevice,
	pub(super) functions: Functions,
	/// The memory types of the GPU, which allocations name by index.
	pub(super) memory: Vec<MemoryType>,
	/// The largest image: its width and height, and its layers.
	pub(super) image_limits: [u32; 2],
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
		let limits = instance.properties(physical).limits.0;
		Ok(Self {
			handle,
			functions,
			memory: instance.memory_types(physical),
			image_limits: [
				limits[MAX_IMAGE_DIMENSION_2D],
				limits[MAX_IMAGE_ARRAY_LAYERS],
			],
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
