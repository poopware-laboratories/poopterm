//! A Vulkan image in the GPU's own memory with a view of all its layers,
//! destroyed and freed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	memory::{Usage, choose_memory},
	types::*,
};
use std::{ptr, rc::Rc};

/// `VK_IMAGE_VIEW_TYPE_2D_ARRAY`: every image is viewed as layers, so
/// shaders sample one kind of image.
const VIEW_2D_ARRAY: i32 = 5;

pub(crate) struct Image {
	handle: RawImage,
	/// Zero until allocated.
	memory: RawMemory,
	/// Zero until made.
	view: RawImageView,
	/// Dropped after the image is destroyed, as it was made from it.
	device: Rc<Device>,
}

impl Image {
	/// An image of `format` (`VkFormat`) for `usage` (`VkImageUsageFlags`),
	/// `size` wide, high and deep in layers, within the device's limits.
	pub(crate) fn new(
		device: &Rc<Device>,
		format: i32,
		size: [u32; 3],
		usage: u32,
	) -> Result<Self, Error> {
		let [width, height, layers] = size;
		let [dimension, most_layers] = device.image_limits;
		let fits = |value, most| (1..=most).contains(&value);
		if !(fits(width, dimension) && fits(height, dimension) && fits(layers, most_layers)) {
			return Err(Error::ImageSize(size));
		}
		let functions = &device.functions;
		let info = ImageCreateInfo {
			s_type: STRUCTURE_TYPE_IMAGE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			image_type: 1,
			format,
			extent: [width, height, 1],
			mip_levels: 1,
			array_layers: layers,
			samples: 1,
			tiling: 0,
			usage,
			sharing_mode: 0,
			queue_family_index_count: 0,
			queue_family_indices: ptr::null(),
			initial_layout: 0,
		};
		let mut handle = 0;
		// SAFETY: the device is live, `info` outlives the call, and `handle`
		// is writable.
		check(unsafe { (functions.create_image)(device.handle, &info, ptr::null(), &mut handle) })?;
		// From here on, dropping `image` undoes what was done so far.
		let mut image = Self {
			handle,
			memory: 0,
			view: 0,
			device: device.clone(),
		};
		let mut needs = MemoryRequirements::default();
		// SAFETY: the image is the device's, and `needs` is writable.
		unsafe { (functions.image_requirements)(device.handle, handle, &mut needs) };
		let index = choose_memory(&device.memory, needs.memory_type_bits, Usage::Device);
		let info = MemoryAllocateInfo {
			s_type: STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
			next: ptr::null(),
			allocation_size: needs.size,
			memory_type_index: index.ok_or(Error::NoMemory)?,
		};
		// A failed call may leave anything in its output.
		let mut memory = 0;
		// SAFETY: as for the image.
		check(unsafe { (functions.allocate)(device.handle, &info, ptr::null(), &mut memory) })?;
		image.memory = memory;
		// SAFETY: the memory is of a type the image allows, and as large as
		// it needs.
		check(unsafe { (functions.bind_image)(device.handle, handle, memory, 0) })?;
		let info = ImageViewCreateInfo {
			s_type: STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			image: handle,
			view_type: VIEW_2D_ARRAY,
			format,
			components: [0; 4],
			subresource_range: SubresourceRange {
				aspect_mask: 1,
				base_mip_level: 0,
				level_count: 1,
				base_array_layer: 0,
				layer_count: layers,
			},
		};
		let mut view = 0;
		// SAFETY: the image is bound to memory, and the view covers what it
		// has.
		check(unsafe { (functions.create_view)(device.handle, &info, ptr::null(), &mut view) })?;
		image.view = view;
		Ok(image)
	}

	pub(crate) fn handle(&self) -> RawImage {
		self.handle
	}

	pub(crate) fn view(&self) -> RawImageView {
		self.view
	}
}

impl Drop for Image {
	fn drop(&mut self) {
		let (device, functions) = (self.device.handle, &self.device.functions);
		// SAFETY: the view, image and memory are the device's and no longer
		// used, and each goes before what it was made from.
		unsafe {
			if self.view != 0 {
				(functions.destroy_view)(device, self.view, ptr::null());
			}
			(functions.destroy_image)(device, self.handle, ptr::null());
			if self.memory != 0 {
				(functions.free)(device, self.memory, ptr::null());
			}
		}
	}
}
