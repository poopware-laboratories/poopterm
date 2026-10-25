//! Tests of choosing the GPU: integrated before discrete before virtual,
//! never one too old or without a queue that can draw (and show, for a
//! window), and a CPU device only when asked for one.
use super::{
	choose::{Choice, Device, GRAPHICS, Kind, QueueFamily, TRANSFER, choose},
	version::api_version,
};

fn family(flags: u32, present: bool) -> QueueFamily {
	QueueFamily {
		flags,
		count: 1,
		present,
	}
}

fn device(kind: Kind) -> Device {
	Device {
		name: format!("{kind:?}"),
		kind,
		api: api_version(1, 3),
		families: vec![family(GRAPHICS | TRANSFER, true)],
		swapchain: true,
	}
}

/// The device and family chosen for a window, then offscreen.
fn chosen(devices: &[Device]) -> (Option<Choice>, Option<Choice>) {
	(choose(devices, true, false), choose(devices, false, false))
}

const fn at(device: usize, family: usize) -> Option<Choice> {
	Some(Choice { device, family })
}

#[test]
fn integrated_gpus_come_before_discrete_before_virtual() {
	let kinds = [
		Kind::Cpu,
		Kind::Virtual,
		Kind::Discrete,
		Kind::Integrated,
		Kind::Other,
	];
	let devices: Vec<_> = kinds.into_iter().map(device).collect();
	assert_eq!(choose(&devices, true, false), at(3, 0));
	assert_eq!(choose(&devices[..3], true, false), at(2, 0));
	assert_eq!(choose(&devices[..2], true, false), at(1, 0));
	assert_eq!(
		choose(&devices[4..], true, false),
		at(0, 0),
		"an unknown kind still draws"
	);
	let virtual_and_other = [device(Kind::Other), device(Kind::Virtual)];
	assert_eq!(choose(&virtual_and_other, true, false), at(1, 0));
}

#[test]
fn a_cpu_device_is_chosen_only_when_asked_for() {
	// Our own software renderer is faster than drawing on the CPU through
	// Vulkan, so a CPU device is for tests.
	let cpu = [device(Kind::Cpu)];
	assert_eq!(chosen(&cpu), (None, None));
	assert_eq!(choose(&cpu, false, true), at(0, 0));
	assert_eq!(choose(&cpu, true, true), at(0, 0));
	// Asked for, it still comes after every GPU.
	let both = [device(Kind::Cpu), device(Kind::Virtual)];
	assert_eq!(choose(&both, false, true), at(1, 0));
}

#[test]
fn devices_older_than_1_3_are_never_chosen() {
	let mut old = device(Kind::Integrated);
	old.api = api_version(1, 2) | 250;
	let mut current = device(Kind::Discrete);
	current.api = api_version(1, 3);
	assert_eq!(chosen(&[old.clone()]), (None, None));
	assert_eq!(chosen(&[old, current]), (at(1, 0), at(1, 0)));
}

#[test]
fn among_devices_of_a_kind_the_newer_version_then_the_first_wins() {
	let mut newer = device(Kind::Discrete);
	newer.api = api_version(1, 4);
	let older = device(Kind::Discrete);
	assert_eq!(
		chosen(&[older.clone(), newer.clone()]),
		(at(1, 0), at(1, 0))
	);
	assert_eq!(
		chosen(&[newer.clone(), older.clone()]),
		(at(0, 0), at(0, 0))
	);
	// The patch level does not count.
	let mut patched = older.clone();
	patched.api |= 280;
	assert_eq!(chosen(&[older.clone(), patched]), (at(0, 0), at(0, 0)));
	assert_eq!(chosen(&[older.clone(), older]), (at(0, 0), at(0, 0)));
	// Kind comes before version.
	assert_eq!(
		chosen(&[newer, device(Kind::Integrated)]),
		(at(1, 0), at(1, 0))
	);
}

#[test]
fn a_window_needs_a_family_that_draws_and_shows_and_the_swapchain() {
	let mut split = device(Kind::Integrated);
	split.families = vec![
		family(TRANSFER, true),
		family(GRAPHICS, false),
		family(GRAPHICS, true),
	];
	// The family that does both, not the first that does either.
	assert_eq!(chosen(&[split.clone()]), (at(0, 2), at(0, 1)));
	// Drawing and showing in different families is not supported.
	split.families.pop();
	assert_eq!(chosen(&[split.clone()]), (None, at(0, 1)));
	// Without the swapchain extension it can only draw offscreen.
	let mut offscreen = device(Kind::Integrated);
	offscreen.swapchain = false;
	assert_eq!(chosen(&[offscreen.clone(), split]), (None, at(0, 0)));
	let fallback = [offscreen, device(Kind::Discrete)];
	assert_eq!(chosen(&fallback), (at(1, 0), at(0, 0)));
}

#[test]
fn a_device_without_a_drawing_queue_is_skipped() {
	let mut transfer_only = device(Kind::Integrated);
	transfer_only.families = vec![family(TRANSFER, true)];
	let mut empty = device(Kind::Integrated);
	empty.families.clear();
	let mut no_queues = device(Kind::Integrated);
	no_queues.families[0].count = 0;
	let none = [transfer_only.clone(), empty.clone(), no_queues.clone()];
	assert_eq!(chosen(&none), (None, None));
	let devices = [transfer_only, empty, no_queues, device(Kind::Virtual)];
	assert_eq!(chosen(&devices), (at(3, 0), at(3, 0)));
}

#[test]
fn nothing_is_chosen_from_no_devices() {
	assert_eq!(chosen(&[]), (None, None));
	assert_eq!(choose(&[], true, true), None);
}

/// What [`choose`] should answer, worked out the long way: every usable
/// pairing of device and family, then the best by the stated rules.
fn expected(devices: &[Device], window: bool, software: bool) -> Option<Choice> {
	let mut best: Option<(usize, usize)> = None;
	for (index, device) in devices.iter().enumerate() {
		let usable = device.api >> 12 >= api_version(1, 3) >> 12
			&& (software || device.kind != Kind::Cpu)
			&& (device.swapchain || !window);
		let draws = |family: &QueueFamily| {
			family.count > 0 && family.flags & GRAPHICS != 0 && (family.present || !window)
		};
		let Some(family) = device.families.iter().position(draws).filter(|_| usable) else {
			continue;
		};
		let better = |(other, _): (usize, usize)| {
			let other: &Device = &devices[other];
			(device.kind < other.kind)
				|| (device.kind == other.kind && device.api >> 12 > other.api >> 12)
		};
		if best.is_none_or(better) {
			best = Some((index, family));
		}
	}
	best.map(|(device, family)| Choice { device, family })
}

#[test]
fn random_machines_get_the_device_the_rules_pick() {
	let mut rng = crate::bench::corpus::Rng::new(23);
	let kinds = [
		Kind::Integrated,
		Kind::Discrete,
		Kind::Virtual,
		Kind::Other,
		Kind::Cpu,
	];
	let flags = [
		0,
		GRAPHICS,
		TRANSFER,
		2,
		GRAPHICS | TRANSFER,
		GRAPHICS | 2 | TRANSFER,
	];
	for _ in 0..5000 {
		let devices: Vec<Device> = (0..rng.below(5))
			.map(|_| Device {
				name: String::new(),
				kind: *rng.pick(&kinds),
				api: api_version(1, 2 + rng.below(3) as u32) | rng.below(300) as u32,
				families: (0..rng.below(4))
					.map(|_| QueueFamily {
						flags: *rng.pick(&flags),
						count: rng.below(3) as u32,
						present: rng.below(2) == 1,
					})
					.collect(),
				swapchain: rng.below(4) != 0,
			})
			.collect();
		for (window, software) in [(true, false), (false, false), (true, true), (false, true)] {
			let answer = choose(&devices, window, software);
			assert_eq!(answer, expected(&devices, window, software), "{devices:#?}");
		}
	}
}

#[test]
fn device_types_map_to_kinds_and_unknown_ones_to_other() {
	// VK_PHYSICAL_DEVICE_TYPE_OTHER, _INTEGRATED_GPU, _DISCRETE_GPU,
	// _VIRTUAL_GPU and _CPU, then values a newer driver might report.
	let kinds = [
		Kind::Other,
		Kind::Integrated,
		Kind::Discrete,
		Kind::Virtual,
		Kind::Cpu,
	];
	for (value, kind) in kinds.into_iter().enumerate() {
		assert_eq!(Kind::from_type(value as i32), kind);
	}
	for unknown in [5, 1_000_000_000, -1, i32::MAX, i32::MIN] {
		assert_eq!(Kind::from_type(unknown), Kind::Other);
	}
}
