//! Tests of recording copies against the fake driver of `driver_tests`:
//! the barriers that move images between layouts and make downloads
//! visible to the CPU, the regions copied, the copies refused, the layouts
//! that take effect only once submitted, and what is held until the GPU is
//! done. The fake's barrier and copy functions live here and check the
//! specification's valid usage rules.
use super::{
	buffer::Buffer,
	buffer_tests::{INTEL, device},
	call::Error,
	commands::Commands,
	commands_tests::Stage,
	driver_tests::{Driver, finish, with},
	image::{Format, Image},
	memory::Usage,
	recording::{Layout, Region},
	submit_types::*,
	timeline::Timeline,
	types::*,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	num::NonZeroUsize,
	ptr,
	rc::Rc,
	slice,
};

/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT` and `_DST_BIT`, which image usage
/// shares, and `VK_IMAGE_USAGE_SAMPLED_BIT`.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
const SAMPLED: u32 = 4;
/// `VkImageLayout`: undefined, transfer source and destination.
const UNDEFINED: i32 = 0;
const SOURCE: i32 = 6;
const DESTINATION: i32 = 7;
/// `VK_PIPELINE_STAGE_2_ALL_TRANSFER_BIT` and `_HOST_BIT`;
/// `VK_ACCESS_2_TRANSFER_READ_BIT`, `_TRANSFER_WRITE_BIT` and
/// `_HOST_READ_BIT`.
const ALL_TRANSFER: u64 = 0x1000;
const HOST: u64 = 0x4000;
const TRANSFER_READ: u64 = 0x800;
const TRANSFER_WRITE: u64 = 0x1000;
const HOST_READ: u64 = 0x2000;

/// The fake's barrier and copy function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCmdPipelineBarrier2" => transmute::<CmdPipelineBarrier2, VoidFunction>(barrier),
			b"vkCmdCopyBufferToImage" => transmute::<CmdCopyBufferToImage, VoidFunction>(upload),
			b"vkCmdCopyImageToBuffer" => transmute::<CmdCopyImageToBuffer, VoidFunction>(download),
			_ => return super::pipeline_driver_tests::function(name),
		})
	}
}

/// What uses an image in `layout`: the stages and accesses a barrier
/// waits for or makes wait.
fn scope(layout: i32) -> (u64, u64) {
	match layout {
		SOURCE => (ALL_TRANSFER, TRANSFER_READ),
		DESTINATION => (ALL_TRANSFER, TRANSFER_WRITE),
		_ => (0, 0),
	}
}

impl Driver {
	/// The layout `image` is in where `buffer` has got to: what its own
	/// barriers left it in, else what the submissions before did.
	fn layout(&mut self, buffer: RawCommandBuffer, image: RawImage) -> i32 {
		let committed = (self.image_layouts.iter().rev()).find(|(made, _)| *made == image);
		let committed = committed.map_or(UNDEFINED, |&(_, layout)| layout);
		let Some(buffer) = self.command_buffer(buffer) else {
			return committed;
		};
		let pending = (buffer.layouts.iter().rev()).find(|(made, _)| *made == image);
		pending.map_or(committed, |&(_, layout)| layout)
	}

	/// Breaks the test unless `buffer` is being recorded.
	fn check_recording(&mut self, buffer: RawCommandBuffer, call: &str) {
		let stage = self.command_buffer(buffer).map(|buffer| buffer.stage);
		self.require(stage == Some(Stage::Recording), || {
			format!("{call}: {stage:?}, not recording")
		});
	}

	/// The size and usage of `buffer`, if live.
	fn made_buffer(&self, buffer: RawBuffer) -> Option<(u64, u32)> {
		let made = self.buffers.get(buffer.wrapping_sub(0xb0_0001) as usize);
		made
			.filter(|_| self.live_buffers.contains(&buffer))
			.map(|made| (made.size, made.usage))
	}

	/// The extent, layers and usage of `image`, if live and bound.
	fn made_image(&self, image: RawImage) -> Option<([u32; 3], u32, u32)> {
		let made = self.images.get(image.wrapping_sub(0xa0_0001) as usize);
		let bound = self.bound_images.iter().any(|&(bound, _)| bound == image);
		let live = self.live_images.contains(&image) && bound;
		made
			.filter(|_| live)
			.map(|made| (made.extent, made.layers, made.usage))
	}

	/// Breaks the test unless `region` lies within `image` and `buffer`,
	/// which are made for the copy, and returns how it prints.
	fn check_copy(
		&mut self,
		call: &str,
		(buffer, buffer_usage): (RawBuffer, u32),
		(image, image_usage): (RawImage, u32),
		regions: &[BufferImageCopy],
	) -> String {
		let [region] = regions else {
			self.require(false, || format!("{call}: one region"));
			return String::new();
		};
		let made_buffer = self.made_buffer(buffer);
		let made_image = self.made_image(image);
		let [x, y, z] = region.image_offset;
		let [width, height, depth] = region.image_extent;
		let [aspect, level, layer, layers] = region.image_subresource;
		let inside = made_image.is_some_and(|([right, bottom, _], count, _)| {
			let within =
				|at: i32, size: u32, end| at >= 0 && at as u64 + u64::from(size) <= u64::from(end);
			within(x, width, right) && within(y, height, bottom) && layer < count
		});
		let bytes = u64::from(width) * u64::from(height) * 4;
		let rules = [
			(
				made_buffer.is_some_and(|(_, usage)| usage & buffer_usage != 0),
				"a live buffer made for it",
			),
			(
				made_image.is_some_and(|(.., usage)| usage & image_usage != 0),
				"a bound image made for it",
			),
			(
				aspect == 1 && level == 0 && layers == 1,
				"one layer of color",
			),
			(
				z == 0 && depth == 1 && width > 0 && height > 0,
				"a rectangle",
			),
			(inside, "within the image"),
			(
				region.buffer_offset.is_multiple_of(4)
					&& made_buffer.is_some_and(|(size, _)| region.buffer_offset + bytes <= size),
				"within the buffer",
			),
			(
				region.buffer_row_length == 0 && region.buffer_image_height == 0,
				"tightly packed",
			),
		];
		for (holds, rule) in rules {
			self.require(holds, || format!("{call}: {rule}"));
		}
		format!(
			"{buffer:#x}+{} layer {layer} at {x},{y} size {width}x{height} of {image:#x}",
			region.buffer_offset
		)
	}
}

unsafe extern "system" fn barrier(buffer: RawCommandBuffer, info: *const DependencyInfo) {
	// SAFETY: the bindings pass a dependency whose arrays live across the
	// call, each as long as its count.
	let (info, buffers, images) = unsafe {
		let info = &*info;
		let read = |count: u32| count as usize;
		let buffers = match info.buffer_memory_barrier_count {
			0 => &[][..],
			count => slice::from_raw_parts(info.buffer_memory_barriers, read(count)),
		};
		let images = match info.image_memory_barrier_count {
			0 => &[][..],
			count => slice::from_raw_parts(info.image_memory_barriers, read(count)),
		};
		(info, buffers, images)
	};
	with(|driver| {
		driver.check_recording(buffer, "vkCmdPipelineBarrier2");
		let rules = [
			(info.s_type == STRUCTURE_TYPE_DEPENDENCY_INFO, "sType"),
			(
				info.next.is_null() && info.dependency_flags == 0,
				"pNext and flags",
			),
			(info.memory_barrier_count == 0, "no global barriers"),
			(buffers.len() + images.len() == 1, "one barrier"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCmdPipelineBarrier2: {rule}"));
		}
		let ignored = |src, dst| src == QUEUE_FAMILY_IGNORED && dst == QUEUE_FAMILY_IGNORED;
		for barrier in images {
			let current = driver.layout(buffer, barrier.image);
			let made = driver.made_image(barrier.image);
			let range = barrier.subresource_range;
			let (from, to) = (barrier.old_layout, barrier.new_layout);
			let rules = [
				(
					barrier.s_type == STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2 && barrier.next.is_null(),
					"sType and pNext",
				),
				(
					ignored(
						barrier.src_queue_family_index,
						barrier.dst_queue_family_index,
					),
					"no queue transfer",
				),
				(made.is_some(), "a live, bound image"),
				(
					range.aspect_mask == 1 && range.base_mip_level == 0 && range.level_count == 1,
					"color, one level",
				),
				(
					made.is_some_and(|(_, layers, _)| {
						range.base_array_layer == 0 && range.layer_count == layers
					}),
					"every layer",
				),
				(from == UNDEFINED || from == current, "the layout it is in"),
				(to != UNDEFINED, "a new layout"),
				(
					(barrier.src_stage_mask, barrier.src_access_mask) == scope(from),
					"after what used it",
				),
				(
					(barrier.dst_stage_mask, barrier.dst_access_mask) == scope(to),
					"before what uses it",
				),
			];
			for (holds, rule) in rules {
				driver.require(holds, || format!("vkCmdPipelineBarrier2: image {rule}"));
			}
			if let Some(recorded) = driver.command_buffer(buffer) {
				recorded.layouts.push((barrier.image, to));
			}
			let image = barrier.image;
			driver
				.recorded
				.push(format!("barrier {image:#x} {from} to {to}"));
		}
		for barrier in buffers {
			let made = driver.made_buffer(barrier.buffer);
			let rules = [
				(
					barrier.s_type == STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER_2 && barrier.next.is_null(),
					"sType and pNext",
				),
				(
					ignored(
						barrier.src_queue_family_index,
						barrier.dst_queue_family_index,
					),
					"no queue transfer",
				),
				(
					made.is_some_and(|(size, _)| barrier.size > 0 && barrier.offset + barrier.size <= size),
					"within a live buffer",
				),
				(
					(barrier.src_stage_mask, barrier.src_access_mask) == (ALL_TRANSFER, TRANSFER_WRITE),
					"after copies wrote it",
				),
				(
					(barrier.dst_stage_mask, barrier.dst_access_mask) == (HOST, HOST_READ),
					"before the CPU reads it",
				),
			];
			for (holds, rule) in rules {
				driver.require(holds, || format!("vkCmdPipelineBarrier2: buffer {rule}"));
			}
			driver.recorded.push(format!(
				"barrier {:#x}+{}+{} to the host",
				barrier.buffer, barrier.offset, barrier.size
			));
		}
	})
}

unsafe extern "system" fn upload(
	buffer: RawCommandBuffer,
	from: RawBuffer,
	to: RawImage,
	layout: i32,
	count: u32,
	regions: *const BufferImageCopy,
) {
	// SAFETY: the bindings pass `count` regions.
	let regions = unsafe { slice::from_raw_parts(regions, count as usize) };
	with(|driver| {
		driver.check_recording(buffer, "vkCmdCopyBufferToImage");
		let current = driver.layout(buffer, to);
		driver.require(layout == DESTINATION && current == DESTINATION, || {
			format!("vkCmdCopyBufferToImage: in layout {layout}, not {current}")
		});
		let copy = driver.check_copy(
			"vkCmdCopyBufferToImage",
			(from, TRANSFER_SRC),
			(to, TRANSFER_DST),
			regions,
		);
		driver.recorded.push(format!("upload {copy}"));
	})
}

unsafe extern "system" fn download(
	buffer: RawCommandBuffer,
	from: RawImage,
	layout: i32,
	to: RawBuffer,
	count: u32,
	regions: *const BufferImageCopy,
) {
	// SAFETY: the bindings pass `count` regions.
	let regions = unsafe { slice::from_raw_parts(regions, count as usize) };
	with(|driver| {
		driver.check_recording(buffer, "vkCmdCopyImageToBuffer");
		let current = driver.layout(buffer, from);
		driver.require(layout == SOURCE && current == SOURCE, || {
			format!("vkCmdCopyImageToBuffer: in layout {layout}, not {current}")
		});
		let copy = driver.check_copy(
			"vkCmdCopyImageToBuffer",
			(to, TRANSFER_DST),
			(from, TRANSFER_SRC),
			regions,
		);
		driver.recorded.push(format!("download {copy}"));
	})
}

/// What the tests copy with: commands on a fake device, an upload buffer,
/// a readback buffer, and a 16×8 image of 3 layers to copy in and out of.
struct Setup {
	commands: Commands,
	staging: Rc<Buffer>,
	readback: Rc<Buffer>,
	image: Rc<Image>,
	_timeline: Rc<Timeline>,
}

fn setup() -> Setup {
	let device = device(&INTEL, |_| {});
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let buffer = |size, usage, purpose| {
		let size = NonZeroUsize::new(size).unwrap();
		Rc::new(Buffer::new(&device, size, usage, purpose).unwrap())
	};
	let staging = buffer(1024, TRANSFER_SRC, Usage::Upload);
	let readback = buffer(1024, TRANSFER_DST, Usage::Readback);
	let usage = TRANSFER_SRC | TRANSFER_DST | SAMPLED;
	let image = Rc::new(Image::new(&device, Format::Bgra, [16, 8, 3], usage).unwrap());
	Setup {
		commands: Commands::new(&timeline).unwrap(),
		staging,
		readback,
		image,
		_timeline: timeline,
	}
}

const REGION: Region = Region {
	layer: 1,
	at: [2, 3],
	size: [4, 5],
};

/// The barriers and copies recorded, without the resets, begins, ends and
/// submissions around them.
fn copies(driver: &Driver) -> Vec<&str> {
	let recorded = driver.recorded.iter().map(String::as_str);
	recorded
		.filter(|call| call.starts_with(['b', 'u', 'd']) && !call.starts_with("begin"))
		.collect()
}

#[test]
fn an_upload_makes_the_image_a_transfer_destination_and_copies_the_region() {
	let Setup {
		mut commands,
		staging,
		image,
		..
	} = setup();
	let mut recording = commands.record().unwrap();
	assert_eq!(recording.upload(&staging, 16, &image, REGION), Ok(()));
	recording.submit().unwrap();
	assert_eq!(image.layout.get(), Layout::TransferDst);
	drop((commands, staging, image));
	let driver = finish();
	let expected = [
		"barrier 0xa00001 0 to 7",
		"upload 0xb00001+16 layer 1 at 2,3 size 4x5 of 0xa00001",
	];
	assert_eq!(copies(&driver), expected);
}

#[test]
fn a_download_makes_the_image_a_transfer_source_and_its_bytes_the_cpus() {
	let Setup {
		mut commands,
		readback,
		image,
		..
	} = setup();
	let mut recording = commands.record().unwrap();
	let whole = Region {
		layer: 2,
		at: [0, 0],
		size: [16, 8],
	};
	assert_eq!(recording.download(&image, whole, &readback, 512), Ok(()));
	recording.submit().unwrap();
	assert_eq!(image.layout.get(), Layout::TransferSrc);
	drop((commands, readback, image));
	let driver = finish();
	// The last texel ends where the buffer does: 512 + 16 × 8 × 4.
	let expected = [
		"barrier 0xa00001 0 to 6",
		"download 0xb00002+512 layer 2 at 0,0 size 16x8 of 0xa00001",
		"barrier 0xb00002+512+512 to the host",
	];
	assert_eq!(copies(&driver), expected);
}

#[test]
fn each_copy_waits_for_the_one_before_and_layouts_carry_over_between_submissions() {
	let Setup {
		mut commands,
		staging,
		readback,
		image,
		..
	} = setup();
	let mut recording = commands.record().unwrap();
	recording.upload(&staging, 0, &image, REGION).unwrap();
	recording.upload(&staging, 80, &image, REGION).unwrap();
	recording.download(&image, REGION, &readback, 0).unwrap();
	recording.upload(&staging, 160, &image, REGION).unwrap();
	recording.submit().unwrap();
	// The next submission starts from where the last copy left the image.
	let mut recording = commands.record().unwrap();
	recording.download(&image, REGION, &readback, 80).unwrap();
	recording.submit().unwrap();
	drop((commands, staging, readback, image));
	let driver = finish();
	let calls: Vec<_> = copies(&driver)
		.into_iter()
		.map(|call| call.split(" layer").next().unwrap())
		.collect();
	// Even two uploads in a row are ordered, in case their regions overlap.
	let expected = [
		"barrier 0xa00001 0 to 7",
		"upload 0xb00001+0",
		"barrier 0xa00001 7 to 7",
		"upload 0xb00001+80",
		"barrier 0xa00001 7 to 6",
		"download 0xb00002+0",
		"barrier 0xb00002+0+80 to the host",
		"barrier 0xa00001 6 to 7",
		"upload 0xb00001+160",
		"barrier 0xa00001 7 to 6",
		"download 0xb00002+80",
		"barrier 0xb00002+80+80 to the host",
	];
	assert_eq!(calls, expected);
}

#[test]
fn layouts_take_effect_only_once_submitted() {
	let Setup {
		mut commands,
		staging,
		readback,
		image,
		..
	} = setup();
	// A recording dropped unsubmitted, and one whose submission failed.
	commands
		.record()
		.unwrap()
		.upload(&staging, 0, &image, REGION)
		.unwrap();
	assert_eq!(image.layout.get(), Layout::Undefined);
	with(|driver| driver.fails.push((c"vkQueueSubmit2", -1)));
	let mut recording = commands.record().unwrap();
	recording.download(&image, REGION, &readback, 0).unwrap();
	assert_eq!(recording.submit(), Err(Error::Failed(-1)));
	assert_eq!(image.layout.get(), Layout::Undefined);
	with(|driver| driver.fails.clear());
	let mut recording = commands.record().unwrap();
	recording.download(&image, REGION, &readback, 0).unwrap();
	recording.submit().unwrap();
	assert_eq!(image.layout.get(), Layout::TransferSrc);
	drop((commands, staging, readback, image));
	let driver = finish();
	let barriers: Vec<_> = (copies(&driver).into_iter())
		.filter(|call| call.starts_with("barrier 0xa"))
		.collect();
	// Each starts from the layout the GPU left the image in.
	let expected = [
		"barrier 0xa00001 0 to 7",
		"barrier 0xa00001 0 to 6",
		"barrier 0xa00001 0 to 6",
	];
	assert_eq!(barriers, expected);
}

#[test]
fn copies_outside_their_buffer_or_image_or_not_made_for_it_are_refused() {
	let Setup {
		mut commands,
		staging,
		readback,
		image,
		..
	} = setup();
	let device = image.device.clone();
	let other =
		Rc::new(Image::new(&device, Format::Bgra, [16, 8, 3], TRANSFER_DST | SAMPLED).unwrap());
	let sampled = Rc::new(Image::new(&device, Format::Rgba, [16, 8, 3], SAMPLED).unwrap());
	let at = |layer, at, size| Region { layer, at, size };
	let mut recording = commands.record().unwrap();
	// Up to the edges: the last layer, the right and bottom texels, and
	// the last byte of the buffer.
	let fits = [
		(0, at(2, [0, 0], [16, 8])),
		(1024 - 4, at(0, [15, 7], [1, 1])),
		(4, at(1, [12, 0], [4, 1])),
	];
	for (offset, region) in fits {
		assert_eq!(
			recording.upload(&staging, offset, &image, region),
			Ok(()),
			"{region:?}"
		);
	}
	let refused = [
		(0, at(3, [0, 0], [1, 1])),
		(0, at(0, [16, 0], [1, 1])),
		(0, at(0, [0, 8], [1, 1])),
		(0, at(0, [15, 0], [2, 1])),
		(0, at(0, [0, 7], [1, 2])),
		(0, at(0, [0, 0], [0, 1])),
		(0, at(0, [0, 0], [1, 0])),
		(0, at(0, [u32::MAX, 0], [2, 1])),
		(0, at(0, [0, u32::MAX], [1, 2])),
		(2, at(0, [0, 0], [1, 1])),
		(1024, at(0, [0, 0], [1, 1])),
		(1024 - 60, at(0, [0, 0], [16, 1])),
		(u64::MAX - 3, at(0, [0, 0], [1, 1])),
	];
	for (offset, region) in refused {
		let result = recording.upload(&staging, offset, &image, region);
		assert_eq!(result, Err(Error::Copy), "{offset} {region:?}");
		let result = recording.download(&image, region, &readback, offset);
		assert_eq!(result, Err(Error::Copy), "{offset} {region:?}");
	}
	// Buffers and images not made for the copy's direction.
	let one = at(0, [0, 0], [1, 1]);
	assert_eq!(
		recording.upload(&readback, 0, &image, one),
		Err(Error::Copy)
	);
	assert_eq!(
		recording.download(&image, one, &staging, 0),
		Err(Error::Copy)
	);
	assert_eq!(
		recording.upload(&staging, 0, &sampled, one),
		Err(Error::Copy)
	);
	assert_eq!(
		recording.download(&other, one, &readback, 0),
		Err(Error::Copy)
	);
	recording.submit().unwrap();
	drop(commands);
	assert_eq!(Rc::strong_count(&other) + Rc::strong_count(&sampled), 2);
	drop((staging, readback, image, other, sampled, device));
	let driver = finish();
	// Only the copies that fit were recorded, nothing for the refused ones.
	let uploads = driver
		.recorded
		.iter()
		.filter(|call| call.starts_with("upload"));
	assert_eq!(uploads.count(), 3);
	assert!(
		!driver
			.recorded
			.iter()
			.any(|call| call.starts_with("download"))
	);
}

#[test]
fn copies_between_devices_are_refused() {
	use super::{
		choose::{Device as Described, GRAPHICS, Kind},
		device::Device,
		driver_tests::{family, install, with_devices},
		memory::MemoryType,
		version::api_version,
	};
	// Two devices of one fake driver: commands on one, an image on the
	// other.
	let mut driver = with_devices(&[Described {
		name: "two".into(),
		kind: Kind::Integrated,
		api: api_version(1, 3),
		families: vec![family(GRAPHICS, 1)],
		swapchain: true,
	}]);
	driver.memory = INTEL.map(|flags| MemoryType { flags, heap: 0 }).to_vec();
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	let here = Rc::new(Device::new(instance.clone(), physical, 0).unwrap());
	let elsewhere = Rc::new(Device::new(instance, physical, 0).unwrap());
	let timeline = Rc::new(Timeline::new(&here).unwrap());
	let mut commands = Commands::new(&timeline).unwrap();
	let size = NonZeroUsize::new(64).unwrap();
	let staging = |device| Rc::new(Buffer::new(device, size, TRANSFER_SRC, Usage::Upload).unwrap());
	let image =
		|device| Rc::new(Image::new(device, Format::Bgra, [4, 4, 1], TRANSFER_DST | SAMPLED).unwrap());
	let one = Region {
		layer: 0,
		at: [0, 0],
		size: [1, 1],
	};
	let mut recording = commands.record().unwrap();
	let (ours, theirs) = (staging(&here), image(&elsewhere));
	assert_eq!(recording.upload(&ours, 0, &theirs, one), Err(Error::Copy));
	let (theirs, ours) = (staging(&elsewhere), image(&here));
	assert_eq!(recording.upload(&theirs, 0, &ours, one), Err(Error::Copy));
	drop(recording);
	drop((commands, timeline, ours, theirs, here, elsewhere));
	assert!(
		!finish()
			.recorded
			.iter()
			.any(|call| call.starts_with("upload"))
	);
}

#[test]
fn what_a_copy_uses_is_held_until_the_gpu_is_done_with_it() {
	let Setup {
		mut commands,
		staging,
		readback,
		image,
		..
	} = setup();
	let mut recording = commands.record().unwrap();
	recording.upload(&staging, 0, &image, REGION).unwrap();
	recording.download(&image, REGION, &readback, 0).unwrap();
	recording.submit().unwrap();
	// Held once for each copy that used it.
	let counts = || [&staging, &readback].map(Rc::strong_count);
	assert_eq!((counts(), Rc::strong_count(&image)), ([2, 2], 3));
	// Recording again waits for the GPU, then lets go.
	commands.record().unwrap();
	assert_eq!((counts(), Rc::strong_count(&image)), ([1, 1], 1));
	let mut recording = commands.record().unwrap();
	recording.upload(&staging, 0, &image, REGION).unwrap();
	recording.submit().unwrap();
	drop(commands);
	assert_eq!((counts(), Rc::strong_count(&image)), ([1, 1], 1));
	drop((staging, readback, image));
	finish();
}

#[test]
fn layouts_name_what_uses_an_image_in_them() {
	assert_eq!(Layout::Undefined.scope(), (0, 0));
	assert_eq!(Layout::TransferSrc.scope(), (ALL_TRANSFER, TRANSFER_READ));
	assert_eq!(Layout::TransferDst.scope(), (ALL_TRANSFER, TRANSFER_WRITE));
	let values =
		[Layout::Undefined, Layout::TransferSrc, Layout::TransferDst].map(|layout| layout as i32);
	assert_eq!(values, [UNDEFINED, SOURCE, DESTINATION]);
}

#[test]
fn the_fake_driver_reports_broken_copy_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let Setup {
		mut commands,
		staging,
		readback,
		image,
		..
	} = setup();
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let functions = &image.device.functions;
	let command_buffer = (0x7c_0001) as RawCommandBuffer;
	let region = |buffer_offset, layer, image_offset: [i32; 2], extent: [u32; 2]| BufferImageCopy {
		buffer_offset,
		buffer_row_length: 0,
		buffer_image_height: 0,
		image_subresource: [1, 0, layer, 1],
		image_offset: [image_offset[0], image_offset[1], 0],
		image_extent: [extent[0], extent[1], 1],
	};
	let image_barrier =
		|old_layout, new_layout, scopes: [(u64, u64); 2], layer_count| ImageMemoryBarrier2 {
			s_type: STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
			next: ptr::null(),
			src_stage_mask: scopes[0].0,
			src_access_mask: scopes[0].1,
			dst_stage_mask: scopes[1].0,
			dst_access_mask: scopes[1].1,
			old_layout,
			new_layout,
			src_queue_family_index: QUEUE_FAMILY_IGNORED,
			dst_queue_family_index: QUEUE_FAMILY_IGNORED,
			image: image.handle(),
			subresource_range: SubresourceRange {
				aspect_mask: 1,
				base_mip_level: 0,
				level_count: 1,
				base_array_layer: 0,
				layer_count,
			},
		};
	let barrier_with = |image: Option<&ImageMemoryBarrier2>,
	                    buffer: Option<&BufferMemoryBarrier2>| {
		let info = DependencyInfo {
			s_type: STRUCTURE_TYPE_DEPENDENCY_INFO,
			next: ptr::null(),
			dependency_flags: 0,
			memory_barrier_count: 0,
			memory_barriers: ptr::null(),
			buffer_memory_barrier_count: buffer.is_some().into(),
			buffer_memory_barriers: buffer.map_or(ptr::null(), ptr::from_ref),
			image_memory_barrier_count: image.is_some().into(),
			image_memory_barriers: image.map_or(ptr::null(), ptr::from_ref),
		};
		// SAFETY: the fake reads only what the pointers point to.
		unsafe { (functions.barrier)(command_buffer, &info) };
	};
	let write = scope(DESTINATION);
	// A barrier outside a recording.
	barrier_with(
		Some(&image_barrier(UNDEFINED, DESTINATION, [(0, 0), write], 3)),
		None,
	);
	assert_eq!(
		reported(),
		["vkCmdPipelineBarrier2: Some(Initial), not recording"]
	);
	let recording = commands.record().unwrap();
	// SAFETY: as above.
	unsafe {
		// Copying in while it is still undefined.
		(functions.upload)(
			command_buffer,
			staging.handle(),
			image.handle(),
			DESTINATION,
			1,
			&region(0, 0, [0, 0], [1, 1]),
		);
		// A barrier from a layout it is not in, of some layers, to
		// undefined, and with scopes that miss what used it.
		barrier_with(
			Some(&image_barrier(
				SOURCE,
				DESTINATION,
				[scope(SOURCE), write],
				3,
			)),
			None,
		);
		barrier_with(
			Some(&image_barrier(UNDEFINED, DESTINATION, [(0, 0), write], 2)),
			None,
		);
		barrier_with(
			Some(&image_barrier(UNDEFINED, UNDEFINED, [(0, 0), (0, 0)], 3)),
			None,
		);
		barrier_with(
			Some(&image_barrier(
				UNDEFINED,
				DESTINATION,
				[(0, 0), (ALL_TRANSFER, 0)],
				3,
			)),
			None,
		);
		barrier_with(
			Some(&image_barrier(
				DESTINATION,
				SOURCE,
				[(0, 0), scope(SOURCE)],
				3,
			)),
			None,
		);
	}
	let expected = [
		"vkCmdCopyBufferToImage: in layout 7, not 0",
		"vkCmdPipelineBarrier2: image the layout it is in",
		"vkCmdPipelineBarrier2: image every layer",
		"vkCmdPipelineBarrier2: image a new layout",
		"vkCmdPipelineBarrier2: image before what uses it",
		"vkCmdPipelineBarrier2: image after what used it",
	];
	assert_eq!(reported(), expected);
	// The last barriers left it a transfer source: copies out of the
	// bounds of the image or buffer, from a buffer not made for it, and
	// rows not tightly packed.
	let mut padded = region(0, 0, [0, 0], [1, 1]);
	padded.buffer_row_length = 2;
	// SAFETY: as above.
	unsafe {
		let out = |region: &BufferImageCopy, to| {
			(functions.download)(command_buffer, image.handle(), SOURCE, to, 1, region)
		};
		out(&region(0, 3, [0, 0], [1, 1]), readback.handle());
		out(&region(0, 0, [-1, 0], [1, 1]), readback.handle());
		out(&region(0, 0, [0, 0], [17, 1]), readback.handle());
		out(&region(1021, 0, [0, 0], [1, 1]), readback.handle());
		out(&region(1024, 0, [0, 0], [1, 1]), readback.handle());
		out(&region(0, 0, [0, 0], [1, 1]), staging.handle());
		out(&padded, readback.handle());
		(functions.download)(
			command_buffer,
			image.handle(),
			SOURCE,
			readback.handle(),
			0,
			ptr::NonNull::dangling().as_ptr(),
		);
	}
	let expected = [
		"vkCmdCopyImageToBuffer: within the image",
		"vkCmdCopyImageToBuffer: within the image",
		"vkCmdCopyImageToBuffer: within the image",
		"vkCmdCopyImageToBuffer: within the buffer",
		"vkCmdCopyImageToBuffer: within the buffer",
		"vkCmdCopyImageToBuffer: a live buffer made for it",
		"vkCmdCopyImageToBuffer: tightly packed",
		"vkCmdCopyImageToBuffer: one region",
	];
	assert_eq!(reported(), expected);
	// Buffer barriers past the buffer's end, or that miss the copies or
	// the CPU.
	let buffer_barrier = |offset, size, src: (u64, u64), dst: (u64, u64)| BufferMemoryBarrier2 {
		s_type: STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER_2,
		next: ptr::null(),
		src_stage_mask: src.0,
		src_access_mask: src.1,
		dst_stage_mask: dst.0,
		dst_access_mask: dst.1,
		src_queue_family_index: QUEUE_FAMILY_IGNORED,
		dst_queue_family_index: QUEUE_FAMILY_IGNORED,
		buffer: readback.handle(),
		offset,
		size,
	};
	let (copied, host) = ((ALL_TRANSFER, TRANSFER_WRITE), (HOST, HOST_READ));
	barrier_with(None, Some(&buffer_barrier(1000, 32, copied, host)));
	barrier_with(
		None,
		Some(&buffer_barrier(0, 4, (ALL_TRANSFER, TRANSFER_READ), host)),
	);
	barrier_with(None, Some(&buffer_barrier(0, 4, copied, (HOST, 0))));
	// Two barriers in one, and none.
	barrier_with(
		Some(&image_barrier(SOURCE, SOURCE, [scope(SOURCE); 2], 3)),
		Some(&buffer_barrier(0, 4, copied, host)),
	);
	barrier_with(None, None);
	let expected = [
		"vkCmdPipelineBarrier2: buffer within a live buffer",
		"vkCmdPipelineBarrier2: buffer after copies wrote it",
		"vkCmdPipelineBarrier2: buffer before the CPU reads it",
		"vkCmdPipelineBarrier2: one barrier",
		"vkCmdPipelineBarrier2: one barrier",
	];
	assert_eq!(reported(), expected);
	drop(recording);
	drop((commands, staging, readback, image));
	finish();
}

#[test]
fn a_device_missing_a_copy_function_is_not_made() {
	use super::{
		choose::{Device as Described, GRAPHICS, Kind},
		device::Device,
		driver_tests::{family, install, with_devices},
		version::api_version,
	};
	for name in [
		c"vkCmdPipelineBarrier2",
		c"vkCmdCopyBufferToImage",
		c"vkCmdCopyImageToBuffer",
	] {
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
		let made = Device::new(instance, physical, 0);
		assert_eq!(made.err(), Some(Error::Missing(name)));
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

/// Keeps `c_void` in use for the signatures above on every target.
const _: Option<*const c_void> = None;
