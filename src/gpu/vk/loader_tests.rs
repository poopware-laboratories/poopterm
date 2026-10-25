//! Tests of the system's Vulkan loader: a missing library or function is an
//! error, never a crash; the loader creates instances that see a device and
//! pass the Khronos validation layer; and without drivers it fails cleanly.
//!
//! Tests that need a loader skip without one, unless `POOPTERM_REQUIRE_VULKAN`
//! is 1, as on Linux CI, which installs Mesa's software driver and the
//! validation layer.
use super::{call::Error, entry::Entry, library::Library, version::major_minor};
use std::{env, process::Command, ptr};

fn required() -> bool {
	env::var("POOPTERM_REQUIRE_VULKAN").is_ok_and(|value| value == "1")
}

/// The system's Vulkan, or `None` where there is none and none is required.
fn entry() -> Option<Entry> {
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
		Ok(instance) => println!(
			"child: {} devices",
			instance.physical_devices().unwrap().len()
		),
		Err(error) => println!("child: {error}"),
	}
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
