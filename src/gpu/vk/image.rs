//! A Vulkan image in the GPU's own memory with a view of all its layers,
//! destroyed and freed when dropped, or a swapchain's image with a view.
use super::{
	allocation::allocate,
	call::{Error, check},
	device::Device,
	memory::Usage,
	recording::Layout,
	types::*,
};
use std::{any::Any, cell::Cell, ptr, rc::Rc};

/// `VK_IMAGE_VIEW_TYPE_2D_ARRAY`: every image is viewed as layers, so
/// shaders sample one kind of image.
const VIEW_2D_ARRAY: i32 = 5;

/// `VkFormat`: eight bits a channel, four bytes a texel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
	/// `VK_FORMAT_R8G8B8A8_UNORM`.
	Rgba = 37,
	/// `VK_FORMAT_B8G8R8A8_UNORM`: `0xAARRGGBB` words in memory.
	Bgra = 44,
}

impl Format {
	/// Its name in the specification, without `VK_FORMAT_`.
	pub(crate) fn name(self) -> &'static str {
		match self {
			Self::Rgba => "R8G8B8A8_UNORM",
			Self::Bgra => "B8G8R8A8_UNORM",
		}
	}
}

pub(crate) struct Image {
	handle: RawImage,
	/// Zero until allocated.
	memory: RawMemory,
	/// Zero until made.
	view: RawImageView,
	/// Width, height and layers.
	pub(super) size: [u32; 3],
	pub(super) format: Format,
	/// `VkImageUsageFlags`.
	pub(super) usage: u32,
	/// The layout the submissions so far leave it in.
	pub(super) layout: Cell<Layout>,
	/// The swapchain whose image it is, which destroys it, and is dropped
	/// after the view; none for an image of our own.
	swapchain: Option<Rc<dyn Any>>,
	/// Dropped after the image is destroyed, as it was made from it.
	pub(super) device: Rc<Device>,
}

impl Image {
	/// An image of `format` for `usage` (`VkImageUsageFlags`), `size` wide,
	/// high and deep in layers, within the device's limits.
	pub(crate) fn new(
		device: &Rc<Device>,
		format: Format,
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
			format: format as i32,
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
		let mut handle = RawImage::NULL;
		// SAFETY: the device is live, `info` outlives the call, and `handle`
		// is writable.
		check(unsafe { (functions.create_image)(device.handle, &info, ptr::null(), &mut handle) })?;
		// From here on, dropping `image` undoes what was done so far.
		let mut image = Self {
			handle,
			memory: RawMemory::NULL,
			view: RawImageView::NULL,
			size,
			format,
			usage,
			layout: Cell::new(Layout::Undefined),
			swapchain: None,
			device: device.clone(),
		};
		let mut needs = MemoryRequirements::default();
		// SAFETY: the image is the device's, and `needs` is writable.
		unsafe { (functions.image_requirements)(device.handle, handle, &mut needs) };
		let memory = allocate(device, &needs, Usage::Device)?;
		image.memory = memory;
		// SAFETY: the memory is of a type the image allows, and as large as
		// it needs.
		check(unsafe { (functions.bind_image)(device.handle, handle, memory, 0) })?;
		image.make_view()?;
		Ok(image)
	}

	/// A view of `handle`, an image of `swapchain` that is `format`, `size`
	/// wide and high and for `usage`.
	pub(super) fn presented(
		device: &Rc<Device>,
		handle: RawImage,
		(format, [width, height], usage): (Format, [u32; 2], u32),
		swapchain: Rc<dyn Any>,
	) -> Result<Self, Error> {
		let mut image = Self {
			handle,
			memory: RawMemory::NULL,
			view: RawImageView::NULL,
			size: [width, height, 1],
			format,
			usage,
			layout: Cell::new(Layout::Undefined),
			swapchain: Some(swapchain),
			device: device.clone(),
		};
		image.make_view()?;
		Ok(image)
	}

	/// Makes the view of every layer, once the image has memory.
	fn make_view(&mut self) -> Result<(), Error> {
		let (format, layers) = (self.format, self.size[2]);
		let info = ImageViewCreateInfo {
			s_type: STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			image: self.handle,
			view_type: VIEW_2D_ARRAY,
			format: format as i32,
			components: [0; 4],
			subresource_range: SubresourceRange {
				aspect_mask: 1,
				base_mip_level: 0,
				level_count: 1,
				base_array_layer: 0,
				layer_count: layers,
			},
		};
		let (device, mut view) = (&self.device, RawImageView::NULL);
		// SAFETY: the image is bound to memory, and the view covers what it
		// has.
		check(unsafe { (device.functions.create_view)(device.handle, &info, ptr::null(), &mut view) })?;
		self.view = view;
		Ok(())
	}

	pub(crate) fn handle(&self) -> RawImage {
		self.handle
	}

	pub(crate) fn view(&self) -> RawImageView {
		self.view
	}

	/// Whether it is a swapchain's, for the presentation engine to show.
	pub(crate) fn presentable(&self) -> bool {
		self.swapchain.is_some()
	}
}

impl Drop for Image {
	fn drop(&mut self) {
		let (device, functions) = (self.device.handle, &self.device.functions);
		// Each goes before what it was made from.
		if self.view != RawImageView::NULL {
			// SAFETY: the view is the device's and no longer used.
			unsafe { (functions.destroy_view)(device, self.view, ptr::null()) };
		}
		if self.swapchain.is_none() {
			// SAFETY: the image is the device's, not the swapchain's, no
			// longer used, and its view is gone.
			unsafe { (functions.destroy_image)(device, self.handle, ptr::null()) };
		}
		if self.memory != RawMemory::NULL {
			// SAFETY: the memory is the device's, and the image bound to it
			// is gone.
			unsafe { (functions.free)(device, self.memory, ptr::null()) };
		}
	}
}
