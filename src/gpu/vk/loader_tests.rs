//! Tests of the system's Vulkan loader: a missing library or function is an
//! error, never a crash; the loader creates instances that see a device and
//! pass the Khronos validation layer; and without drivers it fails cleanly.
//!
//! Tests that need a loader skip without one, unless `POOPTERM_REQUIRE_VULKAN`
//! is 1, as on Linux CI, which installs Mesa's software driver and the
//! validation layer.
use super::{
	buffer::Buffer,
	call::Error,
	choose::{GRAPHICS, Kind, choose},
	commands::Commands,
	device::Device,
	entry::Entry,
	image::{Format, Image},
	instance::Instance,
	library::Library,
	memory::{DEVICE_LOCAL, HOST_COHERENT, HOST_VISIBLE, Usage, choose_memory},
	surface::{Platform, Surface, Window},
	timeline::Timeline,
	version::major_minor,
};
use std::{env, num::NonZeroUsize, process::Command, ptr, rc::Rc, time::Duration};

fn required() -> bool {
	env::var("POOPTERM_REQUIRE_VULKAN").is_ok_and(|value| value == "1")
}

/// The system's Vulkan, or `None` where there is none and none is required.
pub(super) fn entry() -> Option<Entry> {
	match Entry::load() {
		Ok(entry) => Some(entry),
		Err(error) if required() => panic!("{error}"),
		Err(error) => {
			eprintln!("skipped: {error}");
			None
		}
	}
}

#[test]
fn a_missing_library_is_an_error_that_names_it() {
	let Err(Error::NoLoader(message)) = Library::open(c"libpoopterm-no-such-library.so.9") else {
		panic!("opened a library that does not exist");
	};
	assert!(
		message.contains("libpoopterm-no-such-library.so.9"),
		"{message}"
	);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_library_without_vulkan_lacks_get_instance_proc_addr() {
	let library = Library::open(c"libm.so.6").expect("glibc has libm");
	assert!(library.symbol(c"cos").is_some());
	assert!(library.symbol(c"vkGetInstanceProcAddr").is_none());
	let Err(error) = Entry::from_library(library) else {
		panic!("libm loaded as Vulkan");
	};
	assert_eq!(error, Error::Missing(c"vkGetInstanceProcAddr"));
}

#[test]
fn a_function_the_loader_lacks_is_an_error_that_names_it() {
	let Some(entry) = entry() else { return };
	let missing = entry.function(ptr::null_mut(), c"vkPoopNoSuchFunction");
	assert_eq!(missing.err(), Some(Error::Missing(c"vkPoopNoSuchFunction")));
	assert!(entry.function(ptr::null_mut(), c"vkCreateInstance").is_ok());
}

#[test]
fn an_instance_asks_for_the_newest_version_used_and_sees_a_device() {
	let Some(entry) = entry() else { return };
	let loader = major_minor(entry.version().expect("version"));
	assert!(loader >= (1, 3), "{loader:?}");
	let instance = entry.create_instance(c"poopterm tests").expect("instance");
	let expected = if loader >= (1, 4) { (1, 4) } else { (1, 3) };
	assert_eq!(major_minor(instance.api_version()), expected);
	let devices = instance.physical_devices().expect("devices");
	assert!(
		!devices.is_empty(),
		"no Vulkan device, not even a software one"
	);
	assert!(devices.iter().all(|device| !device.is_null()));
	// Asking twice gives the same devices.
	assert_eq!(instance.physical_devices().unwrap(), devices);
}

/// What `child` does, run in a new process with `vars` set, as the loader
/// reads its settings only from the environment.
fn run_child(vars: &[(&str, &str)]) -> String {
	let output = Command::new(env::current_exe().unwrap())
		.args(["--exact", "gpu::vk::loader_tests::child", "--nocapture"])
		.env("POOPTERM_VK_CHILD", "1")
		.envs(vars.iter().copied())
		.output()
		.expect("test binary runs");
	let text = String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
	assert!(output.status.success(), "{text}");
	text.into_owned()
}

/// Run by [`run_child`] only: creates an instance, lists its devices, and
/// prints how that went.
#[test]
fn child() {
	if env::var_os("POOPTERM_VK_CHILD").is_none() {
		return;
	}
	let Ok(entry) = Entry::load() else {
		return println!("child: no loader");
	};
	match entry.create_instance(c"poopterm child") {
		Ok(instance) => {
			println!(
				"child: {} devices",
				instance.physical_devices().unwrap().len()
			);
			match offscreen(Rc::new(instance)) {
				Ok(device) => {
					println!("child: a device drawing on family {}", device.family());
					let device = Rc::new(device);
					for (purpose, usage) in PURPOSES {
						let buffer = Buffer::new(&device, NonZeroUsize::MIN, usage, purpose);
						println!("child: {purpose:?} buffer: {}", buffer.is_ok());
					}
					for (name, format, size, usage) in IMAGES {
						let image = Image::new(&device, format, size, usage);
						println!("child: {name} image: {}", image.is_ok());
					}
					let waited = Timeline::new(&device).and_then(|timeline| timeline.wait(0, Duration::ZERO));
					println!("child: timeline waited: {waited:?}");
					let timeline = Rc::new(Timeline::new(&device).unwrap());
					let submitted = Commands::new(&timeline).and_then(|mut commands| {
						let value = commands.record()?.submit()?;
						commands.finish().map(|()| value)
					});
					println!("child: submitted: {submitted:?}");
					let copied = super::round_trip_tests::round_trip(&device);
					println!("child: round trip: {copied:?}");
					let pipeline = super::pipeline::QuadPipeline::new(&device, Format::Bgra);
					println!("child: quad pipeline: {}", pipeline.is_ok());
					let set = pipeline
						.map(Rc::new)
						.and_then(|pipeline| super::descriptors::QuadSet::new(&pipeline));
					println!("child: quad set: {}", set.is_ok());
					let drawn = super::draw_gpu_tests::draw_known_quads(&device);
					println!("child: drawn: {drawn:?}");
					let unlike = super::renderer_gpu_tests::unlike_software(&device);
					println!("child: unlike software: {unlike:?}");
				}
				Err(error) => println!("child: no device: {error}"),
			}
		}
		Err(error) => println!("child: {error}"),
	}
	if let Ok(entry) = Entry::load() {
		println!("child: headless shows: {:?}", headless_shows(entry));
	}
}

/// Whether a family of some device shows on a headless surface.
fn headless_shows(entry: Entry) -> Result<bool, Error> {
	let instance = Rc::new(entry.create_window_instance(c"poopterm tests", Platform::Headless)?);
	// SAFETY: a headless surface has no window handles to keep live.
	let surface = unsafe { Surface::new(&instance, Window::Headless) }?;
	let mut shows = false;
	for physical in instance.physical_devices()? {
		let mut device = instance.describe(physical)?;
		surface.mark_present(physical, &mut device)?;
		shows |= device.families.iter().any(|family| family.present);
	}
	Ok(shows)
}

#[test]
fn a_headless_surface_shows_on_a_device() {
	let Some(entry) = entry() else { return };
	let offered = entry.instance_extensions().expect("extensions");
	assert!(
		offered.iter().any(|name| name == "VK_KHR_surface"),
		"{offered:?}"
	);
	if !offered.iter().any(|name| name == "VK_EXT_headless_surface") {
		assert!(!required(), "no headless surfaces: {offered:?}");
		return eprintln!("skipped: no headless surfaces");
	}
	assert_eq!(headless_shows(entry), Ok(true));
}

#[test]
fn a_window_instance_for_a_platform_the_loader_offers_is_made() {
	for platform in [Platform::Wayland, Platform::Xlib] {
		let Some(entry) = entry() else { return };
		let offered = entry.instance_extensions().expect("extensions");
		let all = (platform.extensions().iter()).all(|name| {
			offered
				.iter()
				.any(|offered| offered.as_bytes() == name.to_bytes())
		});
		let made = entry.create_window_instance(c"poopterm tests", platform);
		assert_eq!(made.is_ok(), all, "{platform:?} with {offered:?}");
	}
}

/// A device on the GPU the renderer would choose to draw offscreen, or on
/// the CPU when there is no GPU.
pub(super) fn offscreen(instance: Rc<Instance>) -> Result<Device, Error> {
	let handles = instance.physical_devices()?;
	let devices: Vec<_> = (handles.iter())
		.map(|&handle| instance.describe(handle))
		.collect::<Result<_, _>>()?;
	let choice = choose(&devices, false, true).expect("a device to draw on");
	Device::new(instance, handles[choice.device], choice.family as u32)
}

#[test]
fn a_loader_without_drivers_fails_to_create_an_instance() {
	let Some(_) = entry() else { return };
	let nowhere = "/nonexistent/poopterm-driver.json";
	let output = run_child(&[("VK_DRIVER_FILES", nowhere), ("VK_ICD_FILENAMES", nowhere)]);
	// VK_ERROR_INCOMPATIBLE_DRIVER: the loader found no driver.
	assert!(output.contains("child: VkResult -9"), "{output}");
}

#[test]
fn instances_pass_the_khronos_validation_layer() {
	let Some(_) = entry() else { return };
	let output = run_child(&[
		("VK_INSTANCE_LAYERS", "VK_LAYER_KHRONOS_validation"),
		("VK_LOADER_DEBUG", "layer"),
	]);
	let active = (output.lines())
		.any(|line| line.contains("Insert instance layer") && line.contains("KHRONOS_validation"));
	if !active {
		assert!(
			!required(),
			"the validation layer is not installed:\n{output}"
		);
		return eprintln!("skipped: no validation layer");
	}
	assert!(
		output.contains("child: ") && output.contains(" devices"),
		"{output}"
	);
	assert!(
		output.contains("child: a device drawing on family"),
		"{output}"
	);
	for purpose in ["Upload", "Device", "Readback"] {
		let line = format!("child: {purpose} buffer: true");
		assert!(output.contains(&line), "{output}");
	}
	for (name, ..) in IMAGES {
		let line = format!("child: {name} image: true");
		assert!(output.contains(&line), "{output}");
	}
	assert!(
		output.contains("child: timeline waited: Ok(true)"),
		"{output}"
	);
	assert!(output.contains("child: round trip: Ok(true)"), "{output}");
	assert!(output.contains("child: quad pipeline: true"), "{output}");
	assert!(output.contains("child: quad set: true"), "{output}");
	assert!(output.contains("child: drawn: Ok(true)"), "{output}");
	assert!(
		output.contains("child: unlike software: Ok([])"),
		"{output}"
	);
	let headless = output.contains("child: headless shows: Ok(true)");
	let unsupported = output.contains("child: headless shows: Err(Missing(");
	assert!(headless || (unsupported && !required()), "{output}");
	for problem in ["Validation Error", "Validation Warning", "VUID-"] {
		assert!(!output.contains(problem), "{output}");
	}
}

#[test]
fn closing_one_loader_leaves_another_usable() {
	// The system counts opens, so dropping one entry keeps the library
	// loaded for the other.
	let (Some(first), Some(second)) = (entry(), entry()) else {
		return;
	};
	let first = first.create_instance(c"first").expect("instance");
	drop(first);
	let second = second.create_instance(c"second").expect("instance");
	assert!(!second.physical_devices().expect("devices").is_empty());
	let third = entry().map(|entry| entry.create_instance(c"third"));
	assert!(third.is_some_and(|instance| instance.is_ok()));
}

#[test]
fn every_device_is_described_and_one_can_draw_offscreen() {
	let Some(entry) = entry() else { return };
	let instance = entry.create_instance(c"poopterm tests").expect("instance");
	let handles = instance.physical_devices().expect("devices");
	let devices: Vec<_> = (handles.iter())
		.map(|&handle| instance.describe(handle).expect("description"))
		.collect();
	for device in &devices {
		assert!(
			!device.name.is_empty() && !device.name.contains('\0'),
			"{device:?}"
		);
		assert!(major_minor(device.api) >= (1, 0), "{device:?}");
		let draws = device
			.families
			.iter()
			.any(|family| family.flags & GRAPHICS != 0);
		assert!(
			draws,
			"a conformant device has a graphics queue: {device:?}"
		);
		assert!(device.families.iter().all(|family| !family.present));
	}
	// Mesa's llvmpipe, at least, is Vulkan 1.3 and draws.
	let choice = choose(&devices, false, true).expect("a device to draw on");
	let chosen = &devices[choice.device];
	assert!(chosen.families[choice.family].flags & GRAPHICS != 0);
	if let Some(gpu) = devices.iter().position(|device| device.kind != Kind::Cpu) {
		assert_ne!(chosen.kind, Kind::Cpu, "{:?} is a GPU", devices[gpu]);
	}
	// Describing again gives the same answer.
	assert_eq!(
		instance.describe(handles[choice.device]).as_ref(),
		Ok(chosen)
	);
}

#[test]
fn a_device_is_made_with_the_needed_features_and_a_queue_that_draws() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = offscreen(instance.clone()).expect("device");
	assert!(!device.queue().is_null());
	// A second device on the same GPU is independent of the first.
	let again = offscreen(instance).expect("device");
	assert_eq!(again.family(), device.family());
	drop(device);
	assert!(!again.queue().is_null());
}

#[test]
fn every_device_has_memory_for_each_use() {
	let Some(entry) = entry() else { return };
	let instance = entry.create_instance(c"poopterm tests").expect("instance");
	for device in instance.physical_devices().expect("devices") {
		let types = instance.memory_types(device);
		// The specification promises a device-local type and a host-visible
		// coherent one.
		let has = |bits| types.iter().any(|kind| kind.flags & bits == bits);
		assert!(has(DEVICE_LOCAL), "{types:?}");
		assert!(has(HOST_VISIBLE | HOST_COHERENT), "{types:?}");
		assert!(types.len() <= 32);
		for usage in [Usage::Upload, Usage::Device, Usage::Readback] {
			let chosen = choose_memory(&types, u32::MAX, usage);
			assert!(
				chosen.is_some_and(|index| (index as usize) < types.len()),
				"{usage:?} {types:?}"
			);
		}
	}
}

/// Each purpose of memory, with buffer usage flags it suits: vertices,
/// storage written by the GPU, and pixels copied out.
const PURPOSES: [(Usage, u32); 3] = [
	(Usage::Upload, 0x80 | 0x1),
	(Usage::Device, 0x20 | 0x2),
	(Usage::Readback, 0x2),
];

#[test]
fn buffers_get_memory_for_their_purpose_and_the_cpu_sees_what_it_should() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let size = NonZeroUsize::new(65_536 + 3).unwrap();
	for (purpose, usage) in PURPOSES {
		let mut buffer = Buffer::new(&device, size, usage, purpose).expect("buffer");
		assert_ne!(buffer.handle(), 0);
		if purpose == Usage::Device {
			assert!(buffer.bytes().is_none());
			continue;
		}
		let bytes = buffer.bytes_mut().expect("mapped");
		assert_eq!(bytes.len(), size.get());
		for (i, byte) in bytes.iter_mut().enumerate() {
			*byte = (i * 7 % 251) as u8;
		}
		let read = buffer.bytes().unwrap();
		assert!(
			read
				.iter()
				.enumerate()
				.all(|(i, &byte)| byte == (i * 7 % 251) as u8)
		);
	}
	// A buffer outliving its device's last other owner keeps it alive.
	let last = Buffer::new(&device, NonZeroUsize::MIN, 0x80, Usage::Upload).unwrap();
	drop(device);
	assert!(last.bytes().is_some_and(|bytes| bytes.len() == 1));
}

/// The images the renderer makes: the glyph atlas in pages of
/// VK_FORMAT_B8G8R8A8_UNORM, sampled and copied to, and a target of
/// _R8G8B8A8_UNORM drawn to and copied from.
const IMAGES: [(&str, Format, [u32; 3], u32); 2] = [
	("atlas", Format::Bgra, [2048, 2048, 4], 0x4 | 0x2),
	("target", Format::Rgba, [800, 600, 1], 0x10 | 0x1),
];

#[test]
fn images_are_made_within_the_devices_limits() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let images: Vec<_> = (IMAGES.iter())
		.map(|&(_, format, size, usage)| Image::new(&device, format, size, usage).expect("image"))
		.collect();
	assert!(
		images
			.iter()
			.all(|image| image.handle() != 0 && image.view() != 0)
	);
	assert_ne!(images[0].handle(), images[1].handle());
	// Every device allows 4096 wide and 256 layers; none allows 2^20.
	let wide = Image::new(&device, Format::Bgra, [4096, 1, 256], 0x4);
	assert!(wide.is_ok());
	let huge = [1 << 20, 1, 1];
	assert_eq!(
		Image::new(&device, Format::Bgra, huge, 0x4).err(),
		Some(Error::ImageSize(huge))
	);
}

#[test]
fn a_timeline_starts_at_zero_and_waiting_past_it_times_out() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let timeline = Timeline::new(&device).expect("timeline");
	assert_eq!(timeline.reached(), Ok(0));
	assert_eq!(timeline.wait(0, Duration::ZERO), Ok(true));
	// Nothing will signal 1: the driver gives up after the timeout.
	assert_eq!(timeline.wait(1, Duration::ZERO), Ok(false));
	assert_eq!(timeline.wait(1, Duration::from_millis(5)), Ok(false));
	// A timeline outliving its device's last other owner keeps it alive.
	drop(device);
	assert_eq!(timeline.reached(), Ok(0));
}

#[test]
fn submitted_commands_raise_the_timeline_once_the_gpu_finishes() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let timeline = Rc::new(Timeline::new(&device).expect("timeline"));
	let mut first = Commands::new(&timeline).expect("commands");
	let mut second = Commands::new(&timeline).expect("commands");
	// Each records again once the GPU finished its last submission.
	for round in 1..=3 {
		assert_eq!(first.record().unwrap().submit(), Ok(2 * round - 1));
		assert_eq!(second.record().unwrap().submit(), Ok(2 * round));
	}
	assert_eq!(timeline.wait(6, Duration::from_secs(10)), Ok(true));
	assert_eq!(timeline.reached(), Ok(6));
	// Dropping waits for the GPU; a recording dropped unsubmitted is
	// discarded.
	drop(first);
	second.record().unwrap();
	drop(device);
	assert_eq!(second.record().unwrap().submit(), Ok(7));
	assert_eq!(second.finish(), Ok(()));
	assert_eq!(timeline.reached(), Ok(7));
}
