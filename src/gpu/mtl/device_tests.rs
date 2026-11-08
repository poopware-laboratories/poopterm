//! Tests of the Metal device: the system's default, with a queue and a
//! name, on the real GPU when there is one; and how failures read.
use super::{Device, Error, owned, retained};
use crate::objc::{Missing, NIL};
use std::rc::Rc;

/// The system's Metal device, or `None`, said, when it has none (a VM
/// without a GPU): the tests that need one then pass without it.
pub(crate) fn device() -> Option<Rc<Device>> {
	match Device::new() {
		Ok(device) => Some(Rc::new(device)),
		Err(error) => {
			eprintln!("no Metal device, skipped: {error}");
			None
		}
	}
}

#[test]
fn the_default_device_has_a_name_and_a_queue_of_its_own() {
	let Some(device) = device() else { return };
	assert!(!device.name().is_empty());
	assert!(device.queue.responds(c"commandBuffer"));
	assert!(device.device.responds(c"newBufferWithLength:options:"));
	// The device and queue are owned once each, and the queue by the device.
	assert_ne!(device.device, device.queue);
}

#[test]
fn nil_where_metal_makes_an_object_is_refused_by_kind() {
	// SAFETY: nil is allowed.
	let made = unsafe { owned(NIL, "texture") };
	assert_eq!(made, Err(Error::Refused("texture")));
	// SAFETY: as above.
	let made = unsafe { retained(NIL, "encoder") };
	assert_eq!(made, Err(Error::Refused("encoder")));
}

#[test]
fn every_failure_says_what_failed() {
	let said = [
		(Error::NoDevice, "no Metal device"),
		(
			Error::Missing(Missing(c"CAMetalLayer")),
			"no Objective-C class CAMetalLayer",
		),
		(Error::Refused("buffer"), "Metal made no buffer"),
		(
			Error::Shader("line 3: oops".into()),
			"Metal shaders failed: line 3: oops",
		),
		(Error::Gpu("hang".into()), "the GPU failed a frame: hang"),
		(Error::Draw, "a frame the GPU cannot hold"),
		(Error::Region, "a region outside its texture"),
	];
	for (error, words) in said {
		assert_eq!(error.to_string(), words);
	}
	assert_eq!(
		Error::from(Missing(c"MTLX")),
		Error::Missing(Missing(c"MTLX"))
	);
}

#[test]
fn a_device_takes_its_name_and_a_queue_of_its_own_from_metal() {
	let device = super::super::fake_tests::fake(0);
	assert_eq!(device.name(), super::super::fake_tests::FAKE);
	assert_ne!(device.queue, device.device);
}

#[test]
fn a_device_whose_queue_or_name_metal_refuses_is_refused_and_let_go() {
	use super::super::fake_tests::{NAME, QUEUE, fake_device};
	for (refused, what) in [
		(QUEUE, "command queue"),
		(NAME, "device name"),
		(QUEUE | NAME, "command queue"),
	] {
		let object = fake_device(refused);
		let kept = object.clone();
		// SAFETY: `retainCount` takes nothing and returns an `NSUInteger`.
		let count = || unsafe { kept.send::<usize>(c"retainCount", ()) };
		assert_eq!(count(), 2);
		assert_eq!(Device::with(object).err(), Some(Error::Refused(what)));
		// The device given was released with the failure, once.
		assert_eq!(count(), 1, "{what}");
	}
}

#[test]
fn a_device_dropped_lets_go_of_its_device_and_queue_and_keeps_no_name() {
	use crate::{
		gpu::mtl::fake_tests::{fake_device, passed},
		objc::probe_tests::Weak,
	};
	let object = fake_device(0);
	let mut gpu = Weak::new(&object);
	let device = Device::with(object).unwrap();
	let mut queue = Weak::new(&device.queue);
	// The name was copied, and its string released with the pool.
	let mut names = passed();
	assert_eq!(names.len(), 1);
	assert!(!names[0].alive());
	assert!(gpu.alive() && queue.alive());
	drop(device);
	assert!(!gpu.alive());
	assert!(!queue.alive());
}

#[test]
fn a_device_refused_its_name_lets_go_of_the_queue_it_was_given() {
	use crate::{
		gpu::mtl::fake_tests::{NAME, fake_device},
		objc::probe_tests::Weak,
	};
	let object = fake_device(NAME);
	let mut gpu = Weak::new(&object);
	assert_eq!(
		Device::with(object).err(),
		Some(Error::Refused("device name"))
	);
	// The queue made before the refusal went with the device.
	assert!(!gpu.alive());
}

#[test]
fn two_devices_on_one_gpu_share_its_name_but_not_a_queue() {
	let (Some(one), Some(two)) = (device(), device()) else {
		return;
	};
	assert_eq!(one.name(), two.name());
	assert_eq!(one.device, two.device);
	assert_ne!(one.queue, two.queue);
}
