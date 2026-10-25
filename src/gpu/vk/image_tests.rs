//! Tests of images against the fake driver of `driver_tests`: what each is
//! made with, its memory and view, the device's size limits, what is undone
//! when a step fails, and the order things go in. The fake's image
//! functions live here and check the specification's valid usage rules.
use super::{
	buffer::Buffer,
	buffer_tests::{GARBAGE, INTEL, allocation, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	device::Device,
	driver_tests::{family, finish, install, with, with_devices},
	image::{Format, Image},
	memory::Usage,
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	num::NonZeroUsize,
	ptr,
	rc::Rc,
};

/// What an image was created with.
#[derive(Debug, PartialEq)]
pub(super) struct MadeImage {
	pub format: i32,
	pub extent: [u32; 3],
	pub layers: u32,
	pub usage: u32,
	/// Mip levels, samples, tiling and initial layout.
	pub fixed: [i64; 4],
}

/// What a view was created with.
#[derive(Debug, PartialEq)]
pub(super) struct MadeView {
	pub image: RawImage,
	pub view_type: i32,
	pub format: i32,
	pub components: [i32; 4],
	pub range: SubresourceRange,
}

/// What the fake aligns image sizes to.
const PAGE: u64 = 4096;
const BGRA: Format = Format::Bgra;
const RGBA: Format = Format::Rgba;
/// `VK_IMAGE_USAGE_TRANSFER_SRC_BIT`, `_TRANSFER_DST_BIT`, `_SAMPLED_BIT`
/// and `_COLOR_ATTACHMENT_BIT`.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
const SAMPLED: u32 = 4;
const STORAGE: u32 = 8;
const COLOR_ATTACHMENT: u32 = 0x10;

/// The fake's image function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateImage" => transmute::<CreateImage, VoidFunction>(create_image),
			b"vkDestroyImage" => transmute::<DestroyImage, VoidFunction>(destroy_image),
			b"vkGetImageMemoryRequirements" => {
				transmute::<GetImageMemoryRequirements, VoidFunction>(requirements)
			}
			b"vkBindImageMemory" => transmute::<BindImageMemory, VoidFunction>(bind),
			b"vkCreateImageView" => transmute::<CreateImageView, VoidFunction>(create_view),
			b"vkDestroyImageView" => transmute::<DestroyImageView, VoidFunction>(destroy_view),
			_ => return super::timeline_tests::function(name),
		})
	}
}

/// The index of `image` in `images`, as the fake numbers them.
fn index(image: RawImage) -> usize {
	(image - 0xa0_0001) as usize
}

unsafe extern "system" fn create_image(
	device: RawDevice,
	info: *const ImageCreateInfo,
	allocator: *const c_void,
	image: *mut RawImage,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateImage");
		let (dimension, layers) = driver.image_limits.unwrap_or((16384, 2048));
		let [width, height, depth] = info.extent;
		let within = |value, most| (1..=most).contains(&value);
		let rules = [
			(info.s_type == STRUCTURE_TYPE_IMAGE_CREATE_INFO, "sType"),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(allocator.is_null(), "no allocator"),
			(info.image_type == 1 && depth == 1, "two-dimensional"),
			(
				within(width, dimension) && within(height, dimension),
				"extent",
			),
			(within(info.array_layers, layers), "arrayLayers"),
			(
				info.mip_levels == 1 && info.samples == 1,
				"one level and sample",
			),
			(
				info.usage != 0 && info.sharing_mode == 0,
				"usage and sharing",
			),
			(matches!(info.initial_layout, 0 | 8), "initialLayout"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateImage: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateImage") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { image.write(GARBAGE) };
			return code;
		}
		driver.images.push(MadeImage {
			format: info.format,
			extent: info.extent,
			layers: info.array_layers,
			usage: info.usage,
			fixed: [
				info.mip_levels.into(),
				info.samples.into(),
				info.tiling.into(),
				info.initial_layout.into(),
			],
		});
		let handle = 0xa0_0000 + driver.images.len() as u64;
		driver.live_images.push(handle);
		// SAFETY: as above.
		unsafe { image.write(handle) };
		0
	})
}

unsafe extern "system" fn requirements(
	device: RawDevice,
	image: RawImage,
	requirements: *mut MemoryRequirements,
) {
	with(|driver| {
		driver.check_live_device(device, "vkGetImageMemoryRequirements");
		let live = driver.live_images.contains(&image);
		driver.require(live, || "vkGetImageMemoryRequirements: a live image".into());
		let made = driver.images.get(index(image));
		let texels = made.map_or(0, |made| made.extent[0] * made.extent[1] * made.layers);
		let written = MemoryRequirements {
			size: (u64::from(texels) * 4).next_multiple_of(PAGE),
			alignment: PAGE,
			memory_type_bits: driver.image_bits.unwrap_or(driver.allowed()),
		};
		// SAFETY: the bindings pass room for the requirements.
		unsafe { requirements.write(written) };
	})
}

unsafe extern "system" fn bind(
	device: RawDevice,
	image: RawImage,
	memory: RawMemory,
	offset: u64,
) -> i32 {
	with(|driver| {
		driver.check_live_device(device, "vkBindImageMemory");
		let live = driver.live_images.contains(&image);
		let unbound = driver.bound_images.iter().all(|&(bound, _)| bound != image);
		let (size, kind) = allocation(driver, memory).unwrap_or((0, 32));
		let allowed = driver.image_bits.unwrap_or(driver.allowed());
		let made = driver.images.get(index(image));
		let needed = made.map_or(0, |made| {
			u64::from(made.extent[0] * made.extent[1] * made.layers) * 4
		});
		let rules = [
			(live && unbound, "a live image bound once"),
			(
				kind < 32 && allowed >> kind & 1 != 0,
				"a memory type it allows",
			),
			(
				offset.is_multiple_of(PAGE) && offset + needed <= size,
				"room at the offset",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkBindImageMemory: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkBindImageMemory") {
			return code;
		}
		driver.bound_images.push((image, memory));
		0
	})
}

unsafe extern "system" fn create_view(
	device: RawDevice,
	info: *const ImageViewCreateInfo,
	allocator: *const c_void,
	view: *mut RawImageView,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateImageView");
		let live = driver.live_images.contains(&info.image);
		let bound = driver
			.bound_images
			.iter()
			.any(|&(bound, _)| bound == info.image);
		let made = driver.images.get(index(info.image)).filter(|_| live);
		let range = info.subresource_range;
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
				"sType",
			),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(allocator.is_null(), "no allocator"),
			(live && bound, "a live image bound to memory"),
			(
				made.is_some_and(|made| made.format == info.format),
				"the image's format",
			),
			(
				info.components.iter().all(|c| (0..=6).contains(c)),
				"components",
			),
			(range.aspect_mask == 1, "the color aspect"),
			(
				range.base_mip_level == 0 && range.level_count == 1,
				"levels",
			),
			(
				made.is_some_and(|made| {
					range.layer_count > 0 && range.base_array_layer + range.layer_count <= made.layers
				}),
				"layers",
			),
			(
				made.is_some_and(|made| made.usage & (SAMPLED | STORAGE | COLOR_ATTACHMENT) != 0),
				"an image made to be viewed",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateImageView: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateImageView") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { view.write(GARBAGE) };
			return code;
		}
		driver.views.push(MadeView {
			image: info.image,
			view_type: info.view_type,
			format: info.format,
			components: info.components,
			range,
		});
		let handle = 0xf0_0000 + driver.views.len() as u64;
		driver.live_views.push(handle);
		// SAFETY: as above.
		unsafe { view.write(handle) };
		0
	})
}

unsafe extern "system" fn destroy_image(
	device: RawDevice,
	image: RawImage,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyImage");
		let live = driver.live_images.contains(&image) && allocator.is_null();
		driver.require(live, || {
			format!("vkDestroyImage: {image:#x}, which is not live")
		});
		driver.live_images.retain(|&other| other != image);
		driver.released.push(format!("image {image:#x}"));
	})
}

unsafe extern "system" fn destroy_view(
	device: RawDevice,
	view: RawImageView,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyImageView");
		let live = driver.live_views.contains(&view) && allocator.is_null();
		driver.require(live, || {
			format!("vkDestroyImageView: {view:#x}, which is not live")
		});
		driver.live_views.retain(|&other| other != view);
		driver.released.push(format!("view {view:#x}"));
	})
}

#[test]
fn an_image_is_made_in_gpu_memory_and_viewed_as_all_its_layers() {
	let device = device(&INTEL, |_| {});
	let image = Image::new(&device, BGRA, [512, 256, 3], SAMPLED | TRANSFER_DST).expect("image");
	assert_eq!((image.handle(), image.view()), (0xa0_0001, 0xf0_0001));
	drop(image);
	drop(device);
	let driver = finish();
	let made = MadeImage {
		format: 44,
		extent: [512, 256, 1],
		layers: 3,
		usage: SAMPLED | TRANSFER_DST,
		// One level and sample, optimal tiling, undefined layout.
		fixed: [1, 1, 0, 0],
	};
	assert_eq!(driver.images, [made]);
	let view = MadeView {
		image: 0xa0_0001,
		// VK_IMAGE_VIEW_TYPE_2D_ARRAY, identity components.
		view_type: 5,
		format: 44,
		components: [0; 4],
		range: SubresourceRange {
			aspect_mask: 1,
			base_mip_level: 0,
			level_count: 1,
			base_array_layer: 0,
			layer_count: 3,
		},
	};
	assert_eq!(driver.views, [view]);
	// Device-local memory the CPU cannot see, as large as asked for.
	assert_eq!(driver.allocations, [(512 * 256 * 3 * 4, 0)]);
	assert_eq!(driver.bound_images, [(0xa0_0001, 0xe0_0001)]);
	assert_eq!(
		driver.released,
		["view 0xf00001", "image 0xa00001", "memory 0xe00001"]
	);
	assert!(driver.mapped.is_empty());
}

#[test]
fn an_image_takes_the_memory_it_allows_and_none_is_an_error() {
	// llvmpipe's one type, and host-visible memory when nothing else is
	// allowed.
	for (memory, bits, chosen) in [(&[0xf][..], None, 0), (&INTEL[..], Some(0b110), 1)] {
		let device = device(memory, |driver| driver.image_bits = bits);
		let image = Image::new(&device, RGBA, [3, 5, 1], COLOR_ATTACHMENT).unwrap();
		drop(image);
		drop(device);
		assert_eq!(finish().allocations, [(PAGE, chosen)]);
	}
	let device = device(&INTEL, |driver| driver.image_bits = Some(0));
	let error = Image::new(&device, RGBA, [8, 8, 1], SAMPLED).err();
	assert_eq!(error, Some(Error::NoMemory));
	drop(device);
	let driver = finish();
	assert_eq!(driver.released, ["image 0xa00001"]);
}

#[test]
fn sizes_beyond_the_devices_limits_or_zero_are_refused_before_anything_is_made() {
	let limited = |driver: &mut super::driver_tests::Driver| driver.image_limits = Some((4096, 16));
	let device = device(&INTEL, limited);
	assert!(Image::new(&device, BGRA, [4096, 4096, 16], SAMPLED).is_ok());
	assert!(Image::new(&device, BGRA, [1, 1, 1], SAMPLED).is_ok());
	let refused = [
		[4097, 1, 1],
		[1, 4097, 1],
		[1, 1, 17],
		[0, 1, 1],
		[1, 0, 1],
		[1, 1, 0],
		[u32::MAX, u32::MAX, u32::MAX],
	];
	for size in refused {
		let error = Image::new(&device, BGRA, size, SAMPLED).err();
		assert_eq!(error, Some(Error::ImageSize(size)));
	}
	let text = Error::ImageSize([4097, 1, 2]).to_string();
	assert_eq!(
		text,
		"the Vulkan device cannot make a 4097×1 image of 2 layers"
	);
	drop(device);
	assert_eq!(finish().images.len(), 2);
}

#[test]
fn a_failing_step_undoes_the_steps_before_it() {
	let cases: [(&CStr, &[&str]); 4] = [
		(c"vkCreateImage", &[]),
		(c"vkAllocateMemory", &["image 0xa00001"]),
		(c"vkBindImageMemory", &["image 0xa00001", "memory 0xe00001"]),
		(c"vkCreateImageView", &["image 0xa00001", "memory 0xe00001"]),
	];
	// VK_ERROR_OUT_OF_HOST_MEMORY, _DEVICE_MEMORY and
	// _COMPRESSION_EXHAUSTED_EXT.
	for (name, released) in cases {
		for code in [-1, -2, -1_000_338_000] {
			let device = device(&INTEL, |driver| driver.fails.push((name, code)));
			let result = Image::new(&device, BGRA, [64, 64, 1], SAMPLED);
			assert_eq!(result.err(), Some(Error::Failed(code)), "{name:?}");
			drop(device);
			// Never the garbage a failed call left behind.
			let driver = finish();
			assert_eq!(driver.released, released, "{name:?}");
			assert!(driver.live_images.is_empty() && driver.live_views.is_empty());
		}
	}
}

#[test]
fn images_and_buffers_keep_their_device_alive_and_go_in_any_order() {
	let device = device(&INTEL, |_| {});
	let atlas = Image::new(&device, BGRA, [2048, 2048, 4], SAMPLED | TRANSFER_DST).unwrap();
	let staging = Buffer::new(&device, NonZeroUsize::MIN, 1, Usage::Upload).unwrap();
	let target = Image::new(
		&device,
		RGBA,
		[800, 600, 1],
		COLOR_ATTACHMENT | TRANSFER_SRC,
	)
	.unwrap();
	assert_ne!(atlas.view(), target.view());
	drop(device);
	drop(atlas);
	drop(staging);
	assert!(with(|driver| driver.destroyed_devices.is_empty()));
	drop(target);
	let driver = finish();
	let released = [
		"view 0xf00001",
		"image 0xa00001",
		"memory 0xe00001",
		"buffer 0xb00001",
		"memory 0xe00002",
		"view 0xf00002",
		"image 0xa00002",
		"memory 0xe00003",
	];
	assert_eq!(driver.released, released);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_device_missing_an_image_function_is_not_made() {
	let names = [
		c"vkCreateImage",
		c"vkDestroyImage",
		c"vkGetImageMemoryRequirements",
		c"vkBindImageMemory",
		c"vkCreateImageView",
		c"vkDestroyImageView",
	];
	for name in names {
		let mut driver = with_devices(&[Described {
			name: "lacking".into(),
			kind: Kind::Integrated,
			api: api_version(1, 3),
			families: vec![family(GRAPHICS, 1)],
			swapchain: true,
		}]);
		driver.lacks.push(name);
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		assert_eq!(
			Device::new(instance, physical, 0).err(),
			Some(Error::Missing(name))
		);
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

#[test]
fn the_fake_driver_reports_broken_image_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let device = device(&INTEL, |driver| driver.image_bits = Some(0b1));
	let handle = with(|driver| driver.live_devices[0]);
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let info = |extent, array_layers, initial_layout| ImageCreateInfo {
		s_type: STRUCTURE_TYPE_IMAGE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		image_type: 1,
		format: BGRA as i32,
		extent,
		mip_levels: 1,
		array_layers,
		samples: 1,
		tiling: 0,
		usage: SAMPLED,
		sharing_mode: 0,
		queue_family_index_count: 0,
		queue_family_indices: ptr::null(),
		initial_layout,
	};
	let mut image = 0;
	// SAFETY: the fake reads only what the pointers point to.
	unsafe {
		create_image(handle, &info([16385, 1, 1], 1, 0), ptr::null(), &mut image);
		create_image(handle, &info([1, 1, 2], 1, 0), ptr::null(), &mut image);
		create_image(handle, &info([1, 1, 1], 2049, 0), ptr::null(), &mut image);
		create_image(handle, &info([1, 1, 1], 1, 2), ptr::null(), &mut image);
	}
	let expected = [
		"vkCreateImage: extent",
		"vkCreateImage: two-dimensional",
		"vkCreateImage: arrayLayers",
		"vkCreateImage: initialLayout",
	];
	assert_eq!(reported(), expected);
	let allocate = |memory_type_index| {
		let info = MemoryAllocateInfo {
			s_type: STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
			next: ptr::null(),
			allocation_size: PAGE,
			memory_type_index,
		};
		let mut memory = 0;
		// SAFETY: as above.
		unsafe { (device.functions.allocate)(handle, &info, ptr::null(), &mut memory) };
		memory
	};
	let (device_local, visible) = (allocate(0), allocate(1));
	let view_info = |image, format, layer_count| ImageViewCreateInfo {
		s_type: STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		image,
		view_type: 5,
		format,
		components: [0; 4],
		subresource_range: SubresourceRange {
			aspect_mask: 1,
			base_mip_level: 0,
			level_count: 1,
			base_array_layer: 0,
			layer_count,
		},
	};
	let mut view = 0;
	// SAFETY: as above.
	unsafe {
		// A view of an image not bound yet; then bound to a type it does not
		// allow, and bound twice; then views in another format and of more
		// layers than it has.
		create_view(
			handle,
			&view_info(image, BGRA as i32, 1),
			ptr::null(),
			&mut view,
		);
		bind(handle, image, visible, 0);
		bind(handle, image, device_local, 0);
		create_view(
			handle,
			&view_info(image, RGBA as i32, 1),
			ptr::null(),
			&mut view,
		);
		create_view(
			handle,
			&view_info(image, BGRA as i32, 2),
			ptr::null(),
			&mut view,
		);
		// A view of an image only copied to and from.
		let mut copied = info([1, 1, 1], 1, 0);
		copied.usage = TRANSFER_SRC | TRANSFER_DST;
		let mut image = 0;
		create_image(handle, &copied, ptr::null(), &mut image);
		bind(handle, image, allocate(0), 0);
		create_view(
			handle,
			&view_info(image, BGRA as i32, 1),
			ptr::null(),
			&mut view,
		);
	}
	let expected = [
		"vkCreateImageView: a live image bound to memory",
		"vkBindImageMemory: a memory type it allows",
		"vkBindImageMemory: a live image bound once",
		"vkCreateImageView: the image's format",
		"vkCreateImageView: layers",
		"vkCreateImageView: an image made to be viewed",
	];
	assert_eq!(reported(), expected);
	// Destroying the device while they live, then each twice.
	drop(device);
	assert_eq!(reported().len(), 2, "buffers or memory, images or views");
	let live = with(|driver| {
		driver.live_devices.push(handle);
		(driver.live_images.clone(), driver.live_views.clone())
	});
	// SAFETY: the fake only records the handles.
	unsafe {
		for &view in &live.1 {
			destroy_view(handle, view, ptr::null());
			destroy_view(handle, view, ptr::null());
		}
		for &image in &live.0 {
			destroy_image(handle, image, ptr::null());
			destroy_image(handle, image, ptr::null());
		}
	}
	assert_eq!(reported().len(), live.0.len() + live.1.len());
	with(|driver| {
		driver.live_devices.clear();
		driver.live_memory.clear();
	});
	finish();
}
