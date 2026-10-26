//! Tests of why a device cannot draw: the reasons `suits` gives, checked
//! in the order a user would fix them, and how a window with no device
//! that suits says so.
use super::{
	call::Error,
	choose::{Device, GRAPHICS, Kind, QueueFamily, TRANSFER, Unsuited, suits},
	version::api_version,
};

fn device(kind: Kind) -> Device {
	Device {
		name: "llvmpipe (LLVM 19.1.7, 256 bits)".to_owned(),
		kind,
		api: api_version(1, 3),
		families: vec![
			QueueFamily {
				flags: TRANSFER,
				count: 1,
				present: true,
			},
			QueueFamily {
				flags: GRAPHICS,
				count: 1,
				present: true,
			},
		],
		swapchain: true,
	}
}

#[test]
fn a_device_that_suits_gives_the_family_it_draws_on() {
	assert_eq!(suits(&device(Kind::Discrete), true, false), Ok(1));
	assert_eq!(suits(&device(Kind::Cpu), true, true), Ok(1));
}

#[test]
fn each_lack_is_reported_and_the_version_first() {
	let mut old = device(Kind::Cpu);
	old.api = api_version(1, 2) | 199;
	old.swapchain = false;
	old.families.clear();
	assert_eq!(
		suits(&old, true, false),
		Err(Unsuited::TooOld(api_version(1, 2) | 199))
	);
	let mut cpu = old.clone();
	cpu.api = api_version(1, 4);
	assert_eq!(suits(&cpu, true, false), Err(Unsuited::Cpu));
	assert_eq!(suits(&cpu, true, true), Err(Unsuited::NoSwapchain));
	// Offscreen, neither the swapchain nor showing counts.
	assert_eq!(suits(&cpu, false, true), Err(Unsuited::NoQueue));
	cpu.swapchain = true;
	assert_eq!(suits(&cpu, true, true), Err(Unsuited::NoQueue));
}

#[test]
fn a_window_needs_a_family_that_shows_where_offscreen_does_not() {
	let mut hidden = device(Kind::Integrated);
	hidden.families[1].present = false;
	assert_eq!(suits(&hidden, true, false), Err(Unsuited::NoQueue));
	assert_eq!(suits(&hidden, false, false), Ok(1));
	hidden.families[1].count = 0;
	assert_eq!(suits(&hidden, false, false), Err(Unsuited::NoQueue));
}

#[test]
fn no_device_names_each_one_and_why_it_cannot_draw() {
	assert_eq!(Error::NoDevice(Vec::new()).to_string(), "no Vulkan device");
	let unsuited = vec![
		("llvmpipe".to_owned(), Unsuited::Cpu),
		(
			"Old GPU".to_owned(),
			Unsuited::TooOld(api_version(1, 2) | 3),
		),
		("Headless".to_owned(), Unsuited::NoSwapchain),
		("Odd GPU".to_owned(), Unsuited::NoQueue),
	];
	assert_eq!(
		Error::NoDevice(unsuited).to_string(),
		"no Vulkan device suits: llvmpipe draws on the CPU, slower than software; \
		 Old GPU is Vulkan 1.2, older than 1.3; Headless cannot show in windows; \
		 Odd GPU has no queue that draws and shows in the window"
	);
}
