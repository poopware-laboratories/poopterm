//! A Vulkan device: a GPU with the features the renderer needs turned on,
//! the queue it draws with and its memory types, destroyed when dropped.
use super::{
	call::Error, descriptor_types::*, draw_types::*, entry::cast, fault, instance::Instance,
	memory::MemoryType, optional::Optional, pipeline_types::*, stamps, submit_types::*, types::*,
};
use std::{ffi::CStr, ptr, rc::Rc};

pub(crate) const SWAPCHAIN: &CStr = c"VK_KHR_swapchain";

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
	pub update_sets: UpdateDescriptorSets,
	pub begin_rendering: CmdBeginRendering,
	pub end_rendering: CmdEndRendering,
	pub bind_pipeline: CmdBindPipeline,
	pub set_viewport: CmdSetViewport,
	pub set_scissor: CmdSetScissor,
	pub bind_sets: CmdBindDescriptorSets,
	pub push: CmdPushConstants,
	pub draw: CmdDraw,
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
				update_sets: cast(function(c"vkUpdateDescriptorSets")?),
				begin_rendering: cast(function(c"vkCmdBeginRendering")?),
				end_rendering: cast(function(c"vkCmdEndRendering")?),
				bind_pipeline: cast(function(c"vkCmdBindPipeline")?),
				set_viewport: cast(function(c"vkCmdSetViewport")?),
				set_scissor: cast(function(c"vkCmdSetScissor")?),
				bind_sets: cast(function(c"vkCmdBindDescriptorSets")?),
				push: cast(function(c"vkCmdPushConstants")?),
				draw: cast(function(c"vkCmdDraw")?),
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
	pub(super) physical: RawPhysicalDevice,
	/// Whether it was made to show in windows, with `VK_KHR_swapchain`.
	pub(super) swapchain: bool,
	/// The optional extensions it was made with.
	pub(crate) optional: Optional,
	/// Its query functions, when its driver has them all.
	pub(super) stamps: Option<stamps::Functions>,
	/// The bits of its queue's timestamps that count, none if it has none,
	/// and the nanoseconds a tick.
	pub(super) timestamp_bits: u32,
	pub(super) timestamp_period: f32,
	/// What says why it was lost, on a device made with
	/// `VK_EXT_device_fault`.
	pub(super) fault: Option<fault::GetDeviceFaultInfo>,
	destroy: DestroyDevice,
	/// Dropped after the device is destroyed, as it was made from it.
	pub(super) instance: Rc<Instance>,
}

impl Device {
	/// The bytes free in the heap a glyph atlas goes to, on a device made
	/// with `VK_EXT_memory_budget` whose driver says.
	pub(crate) fn atlas_heap_free(&self) -> Option<u64> {
		self.optional.memory_budget?;
		use super::memory::{Usage, choose_memory};
		let index = choose_memory(&self.memory, u32::MAX, Usage::Device)?;
		self
			.instance
			.heap_free(self.physical, self.memory[index as usize].heap)
	}

	/// A device on `physical`, one of the instance's, with one queue of
	/// `family`, drawing offscreen.
	pub(crate) fn new(
		instance: Rc<Instance>,
		physical: RawPhysicalDevice,
		family: u32,
	) -> Result<Self, Error> {
		Self::make(instance, physical, family, None)
	}

	/// A device as [`Self::new`] makes, that also shows in windows: its
	/// family must show on their surfaces.
	pub(crate) fn for_windows(
		instance: Rc<Instance>,
		physical: RawPhysicalDevice,
		family: u32,
	) -> Result<Self, Error> {
		Self::with_optional(instance, physical, family, Optional::default())
	}

	/// A device as [`Self::for_windows`] makes, with the `optional`
	/// extensions, which it must offer.
	pub(crate) fn with_optional(
		instance: Rc<Instance>,
		physical: RawPhysicalDevice,
		family: u32,
		optional: Optional,
	) -> Result<Self, Error> {
		let mut device = Self::make(instance, physical, family, Some(optional))?;
		device.optional = optional;
		let name = c"vkGetDeviceFaultInfoEXT";
		let found = optional
			.device_fault
			.and_then(|_| device.instance.device_function(device.handle, name).ok());
		// SAFETY: the function the driver returned for this name has this type.
		device.fault = found.map(|function| unsafe { cast(function) });
		Ok(device)
	}

	/// With `windows`, it shows in them, made with the optional extensions.
	fn make(
		instance: Rc<Instance>,
		physical: RawPhysicalDevice,
		family: u32,
		windows: Option<Optional>,
	) -> Result<Self, Error> {
		let swapchain = windows.is_some();
		let mut extensions = Vec::from(if swapchain { &[SWAPCHAIN][..] } else { &[] });
		let optional = windows.unwrap_or_default();
		extensions.extend(optional.extensions());
		let handle = instance.create_device(physical, family, &extensions, &optional.features())?;
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
		let stamp = |name: &'static CStr| instance.device_function(handle, name).ok();
		// SAFETY: the functions the driver returned for these names have
		// these types.
		let stamps = (|| unsafe {
			Some(stamps::Functions {
				create: cast(stamp(c"vkCreateQueryPool")?),
				destroy: cast(stamp(c"vkDestroyQueryPool")?),
				reset: cast(stamp(c"vkCmdResetQueryPool")?),
				write: cast(stamp(c"vkCmdWriteTimestamp2")?),
				results: cast(stamp(c"vkGetQueryPoolResults")?),
			})
		})();
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
			physical,
			swapchain,
			optional: Optional::default(),
			stamps,
			timestamp_bits: instance.timestamp_bits(physical, family),
			timestamp_period: f32::from_bits(limits[TIMESTAMP_PERIOD]),
			fault: None,
			destroy,
			instance,
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
