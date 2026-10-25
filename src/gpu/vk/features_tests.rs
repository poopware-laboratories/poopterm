//! Tests of the feature chain: how it is linked, which features the
//! renderer turns on, and which missing one a device is refused for.
use super::{
	features::{Features, Flag, NEEDED},
	types::*,
};
use std::ptr;

/// Every flag of the 1.2 and 1.3 structures.
fn every_flag() -> impl Iterator<Item = Flag> {
	(0..47).map(Flag::V12).chain((0..15).map(Flag::V13))
}

#[test]
fn the_chain_links_the_1_2_then_the_1_3_features_and_survives_a_move() {
	let features = Features::none();
	let moved = [features];
	let features = &moved[0];
	assert_eq!(
		features.head.s_type,
		STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2
	);
	assert_eq!(
		(features.v12.s_type, features.v13.s_type),
		(51, 53),
		"VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES and _1_3_"
	);
	assert!(ptr::eq(features.head.next.cast(), &features.v12));
	assert!(ptr::eq(features.v12.next.cast(), &features.v13));
	assert!(features.v13.next.is_null());
}

#[test]
fn nothing_is_on_until_the_driver_says_so() {
	let features = Features::none();
	assert!(every_flag().all(|flag| !features.get(flag)));
	assert!(features.head.features.iter().all(|&flag| flag == 0));
}

#[test]
fn the_renderer_turns_on_dynamic_rendering_synchronization2_and_timeline_semaphores() {
	let names: Vec<_> = NEEDED.iter().map(|&(_, name)| name).collect();
	assert_eq!(
		names,
		["dynamicRendering", "synchronization2", "timelineSemaphore"]
	);
	// Their places in VkPhysicalDeviceVulkan13Features and
	// VkPhysicalDeviceVulkan12Features.
	let flags: Vec<_> = NEEDED.iter().map(|&(flag, _)| flag).collect();
	assert_eq!(flags, [Flag::V13(12), Flag::V13(9), Flag::V12(37)]);
	let needed = Features::needed();
	let on: Vec<_> = every_flag().filter(|&flag| needed.get(flag)).collect();
	assert_eq!(on, [Flag::V12(37), Flag::V13(9), Flag::V13(12)]);
	// As VkBool32 VK_TRUE, which is 1, not just nonzero.
	assert_eq!(
		(
			needed.v12.flags[37],
			needed.v13.flags[9],
			needed.v13.flags[12]
		),
		(1, 1, 1)
	);
	assert!(needed.head.features.iter().all(|&flag| flag == 0));
	assert!(ptr::eq(needed.head.next.cast(), &needed.v12));
}

/// Features with every flag on but `off`.
fn all_but(off: &[Flag]) -> Box<Features> {
	let mut features = Features::none();
	features.v12.flags = [1; 47];
	features.v13.flags = [1; 15];
	for &flag in off {
		match flag {
			Flag::V12(index) => features.v12.flags[index] = 0,
			Flag::V13(index) => features.v13.flags[index] = 0,
		}
	}
	features
}

#[test]
fn a_device_lacks_the_first_needed_feature_it_does_not_have() {
	assert_eq!(all_but(&[]).missing(), None);
	for (flag, name) in NEEDED {
		assert_eq!(all_but(&[flag]).missing(), Some(name), "{flag:?}");
	}
	assert_eq!(
		all_but(&[Flag::V12(37), Flag::V13(9)]).missing(),
		Some("synchronization2")
	);
	assert_eq!(Features::none().missing(), Some("dynamicRendering"));
	// Only the needed ones count.
	let unneeded: Vec<_> = every_flag()
		.filter(|flag| NEEDED.iter().all(|(needed, _)| needed != flag))
		.collect();
	assert_eq!(unneeded.len(), 47 + 15 - 3);
	assert_eq!(all_but(&unneeded).missing(), None);
}

#[test]
fn any_nonzero_vk_bool32_counts_as_true() {
	// Drivers write VK_TRUE, but a nonzero value is read as true in C too.
	let mut features = all_but(&[]);
	features.v13.flags[12] = 0xff;
	features.v12.flags[37] = 2;
	assert!(features.get(Flag::V13(12)) && features.get(Flag::V12(37)));
	assert_eq!(features.missing(), None);
}
