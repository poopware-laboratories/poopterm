//! Tests of drawing quads against the fake driver of `driver_tests`: the
//! set updated with the quads and the atlas, the barriers, the rendering
//! and the draw, the draws refused, a set used once a submission, and what
//! is held until the GPU is done.
use super::{
	buffer::Buffer,
	buffer_tests::{INTEL, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	commands::Commands,
	descriptors::QuadSet,
	device::Device,
	draw::Draw,
	driver_tests::{Driver, family, finish, install, with, with_devices},
	image::{Format, Image},
	memory::{MemoryType, Usage},
	pipeline::QuadPipeline,
	recording::{Layout, Region},
	timeline::Timeline,
	version::api_version,
};
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT` and `_DST_BIT`, which image usage
/// shares; `VK_IMAGE_USAGE_SAMPLED_BIT` and `_COLOR_ATTACHMENT_BIT`; and
/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT`.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
const SAMPLED: u32 = 4;
const COLOR_ATTACHMENT: u32 = 0x10;
const STORAGE: u32 = 0x20;

/// What the tests draw with, on a fake device: room for three quads, an
/// atlas of two pages, and a 16×8 target.
struct Scene {
	timeline: Rc<Timeline>,
	set: Rc<QuadSet>,
	quads: Rc<Buffer>,
	atlas: Rc<Image>,
	target: Rc<Image>,
}

fn quads(device: &Rc<Device>, bytes: usize, usage: u32) -> Rc<Buffer> {
	let size = NonZeroUsize::new(bytes).unwrap();
	Rc::new(Buffer::new(device, size, usage, Usage::Upload).unwrap())
}

fn image(device: &Rc<Device>, format: Format, size: [u32; 3], usage: u32) -> Rc<Image> {
	Rc::new(Image::new(device, format, size, usage).unwrap())
}

fn scene() -> (Commands, Scene) {
	scene_on(&device(&INTEL, |_| {}))
}

fn scene_on(device: &Rc<Device>) -> (Commands, Scene) {
	let timeline = Rc::new(Timeline::new(device).unwrap());
	let pipeline = Rc::new(QuadPipeline::new(device, Format::Bgra).unwrap());
	let commands = Commands::new(&timeline).unwrap();
	let scene = Scene {
		timeline,
		set: Rc::new(QuadSet::new(&pipeline).unwrap()),
		quads: quads(device, 3 * 48, STORAGE),
		atlas: image(device, Format::Bgra, [8, 8, 2], SAMPLED | TRANSFER_DST),
		target: image(
			device,
			Format::Bgra,
			[16, 8, 1],
			COLOR_ATTACHMENT | TRANSFER_SRC,
		),
	};
	(commands, scene)
}

impl Scene {
	/// A draw of `count` quads with the scene's set, atlas and target.
	fn draw(&self, count: u32) -> Draw<'_> {
		Draw {
			set: &self.set,
			quads: &self.quads,
			count,
			atlas: &self.atlas,
			target: &self.target,
		}
	}
}

/// What was recorded from the first descriptor write on.
fn drawn(driver: &Driver) -> Vec<&str> {
	let recorded = driver.recorded.iter().map(String::as_str);
	let from = recorded.clone().position(|call| call.starts_with("write"));
	recorded.skip(from.unwrap_or(usize::MAX)).collect()
}

#[test]
fn a_draw_points_the_set_at_the_quads_and_atlas_and_renders_over_the_target() {
	let (mut commands, scene) = scene();
	let mut recording = commands.record().unwrap();
	assert_eq!(recording.draw(scene.draw(3)), Ok(()));
	recording.submit().unwrap();
	assert_eq!(scene.atlas.layout.get(), Layout::ShaderReadOnly);
	assert_eq!(scene.target.layout.get(), Layout::ColorAttachment);
	drop((commands, scene));
	let driver = finish();
	// The whole buffer, the atlas's view read by shaders; the atlas made
	// readable and the target writable; loaded, so quads go over what is
	// there; the target's size pushed for the vertex shader.
	let expected = [
		"write 0x660001 binding 0: 0xb00001 range 0xffffffffffffffff",
		"write 0x660001 binding 1: 0xf00001 layout 0x5",
		"barrier 0xa00001 0 to 5",
		"barrier 0xa00002 0 to 2",
		"begin rendering 0xf00002 at 0,0 size 16x8 load 0",
		"bind pipeline 0x640001",
		"viewport 0,0 size 16x8",
		"scissor 0,0 size 16x8",
		"bind set 0x660001",
		"push [16.0, 8.0]",
		"draw 6 vertices of 3 instances from 0",
		"end rendering",
		"end 0x7c0001",
	];
	assert_eq!(drawn(&driver)[..expected.len()], expected);
}

#[test]
fn a_drawn_target_reads_back_after_the_draw_and_layouts_carry_over() {
	let (mut commands, scene) = scene();
	let device = scene.target.device.clone();
	let readback = Rc::new(
		Buffer::new(
			&device,
			NonZeroUsize::new(512).unwrap(),
			TRANSFER_DST,
			Usage::Readback,
		)
		.unwrap(),
	);
	let whole = Region {
		layer: 0,
		at: [0, 0],
		size: [16, 8],
	};
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	recording
		.download(&scene.target, whole, &readback, 0)
		.unwrap();
	recording.submit().unwrap();
	assert_eq!(scene.target.layout.get(), Layout::TransferSrc);
	// The next submission draws again from where the first left both.
	commands.finish().unwrap();
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	recording.submit().unwrap();
	drop((commands, readback, device, scene));
	let driver = finish();
	let barriers: Vec<_> = (driver.recorded.iter())
		.filter(|call| call.starts_with("barrier 0xa"))
		.collect();
	assert_eq!(
		barriers,
		[
			"barrier 0xa00001 0 to 5",
			"barrier 0xa00002 0 to 2",
			"barrier 0xa00002 2 to 6",
			"barrier 0xa00001 5 to 5",
			"barrier 0xa00002 6 to 2",
		]
	);
}

#[test]
fn draws_that_break_a_rule_are_refused_and_record_and_hold_nothing() {
	let (mut commands, scene) = scene();
	let device = scene.target.device.clone();
	let unstored = quads(&device, 3 * 48, TRANSFER_SRC);
	let unsampled = image(&device, Format::Bgra, [8, 8, 2], COLOR_ATTACHMENT);
	let unattached = image(&device, Format::Bgra, [16, 8, 1], SAMPLED);
	let rgba = image(&device, Format::Rgba, [16, 8, 1], COLOR_ATTACHMENT);
	let layered = image(&device, Format::Bgra, [16, 8, 2], COLOR_ATTACHMENT);
	let both = image(
		&device,
		Format::Bgra,
		[16, 8, 1],
		COLOR_ATTACHMENT | SAMPLED,
	);
	let mut recording = commands.record().unwrap();
	let cases = [
		// The quads: drawn past the buffer's end, or not made for storage.
		Draw {
			count: 4,
			..scene.draw(0)
		},
		Draw {
			quads: &unstored,
			..scene.draw(1)
		},
		// The atlas, unsampled; the target, not an attachment, of another
		// format than the pipeline draws, or of two layers.
		Draw {
			atlas: &unsampled,
			..scene.draw(1)
		},
		Draw {
			target: &unattached,
			..scene.draw(1)
		},
		Draw {
			target: &rgba,
			..scene.draw(1)
		},
		Draw {
			target: &layered,
			..scene.draw(1)
		},
		// One image as both atlas and target.
		Draw {
			atlas: &both,
			target: &both,
			..scene.draw(1)
		},
	];
	for (case, draw) in cases.into_iter().enumerate() {
		assert_eq!(recording.draw(draw), Err(Error::Draw), "case {case}");
	}
	assert_eq!(recording.sets.len(), 0);
	// Exactly as many quads as the buffer holds are drawn.
	assert_eq!(recording.draw(scene.draw(3)), Ok(()));
	drop(recording);
	let images = [&unsampled, &unattached, &rgba, &layered, &both].map(Rc::strong_count);
	assert_eq!((Rc::strong_count(&unstored), images), (1, [1; 5]));
	drop((
		commands, scene, device, unstored, unsampled, unattached, rgba, layered, both,
	));
	let driver = finish();
	let writes = driver
		.recorded
		.iter()
		.filter(|call| call.starts_with("write"))
		.count();
	let barriers = driver
		.recorded
		.iter()
		.filter(|call| call.starts_with("barrier"))
		.count();
	assert_eq!((writes, barriers), (2, 2));
}

#[test]
fn draws_between_devices_are_refused() {
	// Two devices of one fake driver: the scene on one, each part of a
	// draw in turn on the other.
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
	let (mut commands, scene) = scene_on(&here);
	let (theirs, there) = scene_on(&elsewhere);
	let mut recording = commands.record().unwrap();
	let cases = [
		Draw {
			set: &there.set,
			..scene.draw(1)
		},
		Draw {
			quads: &there.quads,
			..scene.draw(1)
		},
		Draw {
			atlas: &there.atlas,
			..scene.draw(1)
		},
		Draw {
			target: &there.target,
			..scene.draw(1)
		},
	];
	for (case, draw) in cases.into_iter().enumerate() {
		assert_eq!(recording.draw(draw), Err(Error::Draw), "case {case}");
	}
	drop(recording);
	drop((commands, theirs, scene, there, here, elsewhere));
	let driver = finish();
	assert!(!driver.recorded.iter().any(|call| call.starts_with("write")));
}

#[test]
fn a_set_draws_once_a_recording_and_again_only_once_the_gpu_is_done() {
	let (mut commands, scene) = scene();
	let mut others = Commands::new(&scene.timeline).unwrap();
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	// Updating the set again would change what the first draw reads.
	assert_eq!(recording.draw(scene.draw(1)), Err(Error::Busy));
	let value = recording.submit().unwrap();
	let mut recording = others.record().unwrap();
	assert_eq!(recording.draw(scene.draw(1)), Err(Error::Busy));
	with(|driver| driver.counters[0] = value);
	assert_eq!(recording.draw(scene.draw(1)), Ok(()));
	recording.submit().unwrap();
	drop((commands, others, scene));
	let driver = finish();
	let draws = driver
		.recorded
		.iter()
		.filter(|call| call.starts_with("draw"))
		.count();
	assert_eq!(draws, 2);
}

#[test]
fn a_set_dropped_with_its_recording_unsubmitted_is_free_again() {
	let (mut commands, scene) = scene();
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	drop(recording);
	let mut recording = commands.record().unwrap();
	assert_eq!(recording.draw(scene.draw(1)), Ok(()));
	recording.submit().unwrap();
	drop((commands, scene));
	finish();
}

#[test]
fn what_a_draw_uses_is_held_until_the_gpu_is_done_with_it() {
	let (mut commands, scene) = scene();
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	recording.submit().unwrap();
	let counts = || {
		let images = [&scene.atlas, &scene.target].map(Rc::strong_count);
		(
			Rc::strong_count(&scene.set),
			Rc::strong_count(&scene.quads),
			images,
		)
	};
	assert_eq!(counts(), (2, 2, [2, 2]));
	// Recording again waits for the GPU, then lets go.
	commands.record().unwrap();
	assert_eq!(counts(), (1, 1, [1, 1]));
	drop((commands, scene));
	finish();
}

#[test]
fn a_device_missing_a_draw_function_is_not_made() {
	let names = [
		c"vkUpdateDescriptorSets",
		c"vkCmdBeginRendering",
		c"vkCmdEndRendering",
		c"vkCmdBindPipeline",
		c"vkCmdSetViewport",
		c"vkCmdSetScissor",
		c"vkCmdBindDescriptorSets",
		c"vkCmdPushConstants",
		c"vkCmdDraw",
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
		let made = Device::new(instance, physical, 0);
		assert_eq!(made.err(), Some(Error::Missing(name)));
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

#[test]
fn the_target_is_of_the_format_its_pipeline_draws() {
	let (mut commands, scene) = scene();
	let device = scene.target.device.clone();
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Rgba).unwrap());
	let set = Rc::new(QuadSet::new(&pipeline).unwrap());
	let rgba = image(&device, Format::Rgba, [16, 8, 1], COLOR_ATTACHMENT);
	let mut recording = commands.record().unwrap();
	let draw = Draw {
		set: &set,
		..scene.draw(1)
	};
	assert_eq!(recording.draw(draw), Err(Error::Draw));
	assert_eq!(
		recording.draw(Draw {
			target: &rgba,
			..draw
		}),
		Ok(())
	);
	recording.submit().unwrap();
	drop((commands, scene, device, pipeline, set, rgba));
	finish();
}
