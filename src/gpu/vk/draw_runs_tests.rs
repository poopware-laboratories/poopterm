//! Tests of drawing runs of quads, each with its image bound, against the
//! fake driver of `driver_tests`: runs drawn in order from their first
//! quad, a draw of no runs rendering nothing, the furthest run checked
//! against the quads, and the texture sets held until the GPU is done.
use super::{
	call::Error,
	draw::Draw,
	draw_tests::{drawn, scene, texture},
	driver_tests::finish,
};
use std::rc::Rc;

#[test]
fn runs_draw_in_order_from_their_first_quad_with_their_image_bound() {
	let (mut commands, scene) = scene();
	let pipeline = &scene.set.pipeline;
	let (a, b) = (texture(pipeline), texture(pipeline));
	let runs = [(0..1, a.clone()), (1..2, b.clone()), (2..3, a.clone())];
	let mut recording = commands.record().unwrap();
	let draw = Draw {
		runs: &runs,
		..scene.draw(0)
	};
	assert_eq!(recording.draw(draw), Ok(()));
	recording.submit().unwrap();
	drop((commands, runs, a, b, scene));
	let driver = finish();
	let drawn = drawn(&driver);
	let from = drawn.iter().position(|call| call.starts_with("push"));
	// Each image made readable once per use, then each run bound and drawn.
	let expected = [
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 0",
		"bind image set 0x660004",
		"draw 6 vertices of 1 instances from 1",
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 2",
		"end rendering",
	];
	assert_eq!(drawn[from.unwrap() + 1..][..expected.len()], expected);
	let barriers = drawn.iter().filter(|call| call.starts_with("barrier"));
	let expected = [
		"barrier 0xa00001 0 to 5",
		"barrier 0xa00004 0 to 5",
		"barrier 0xa00005 0 to 5",
		"barrier 0xa00004 5 to 5",
		"barrier 0xa00002 0 to 2",
	];
	assert_eq!(barriers.copied().collect::<Vec<_>>(), expected);
}

#[test]
fn a_draw_of_no_runs_renders_nothing_over_the_target() {
	let (mut commands, scene) = scene();
	let mut recording = commands.record().unwrap();
	assert_eq!(recording.draw(scene.draw(0)), Ok(()));
	recording.submit().unwrap();
	drop((commands, scene));
	let driver = finish();
	let drawn = drawn(&driver);
	// Rendering begins and ends, so the target is ready to show, with no
	// image bound and nothing drawn.
	assert!(drawn.contains(&"end rendering"), "{drawn:?}");
	let images = drawn
		.iter()
		.filter(|call| call.starts_with("bind image set"));
	assert_eq!(images.count(), 0);
	assert!(
		!drawn.iter().any(|call| call.starts_with("draw")),
		"{drawn:?}"
	);
}

#[test]
fn the_furthest_run_and_not_the_last_must_fit_the_quads() {
	let (mut commands, scene) = scene();
	let image = &scene.runs[1][0].1;
	let mut recording = commands.record().unwrap();
	// The buffer holds three quads; a run of four, then one of one.
	let past = [(0..4, image.clone()), (0..1, image.clone())];
	let draw = Draw {
		runs: &past,
		..scene.draw(0)
	};
	assert_eq!(recording.draw(draw), Err(Error::Draw));
	// Ending at the third quad, in any run, fits.
	let fits = [(2..3, image.clone()), (0..2, image.clone())];
	let draw = Draw {
		runs: &fits,
		..scene.draw(0)
	};
	assert_eq!(recording.draw(draw), Ok(()));
	drop(recording);
	drop((past, fits, commands, scene));
	finish();
}

#[test]
fn texture_sets_drawn_are_held_until_the_gpu_is_done() {
	let (mut commands, scene) = scene();
	let image = scene.runs[1][0].1.clone();
	let held = Rc::strong_count(&image);
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw(1)).unwrap();
	recording.submit().unwrap();
	assert_eq!(Rc::strong_count(&image), held + 1);
	// Recording again waits for the GPU, then lets go.
	commands.record().unwrap();
	assert_eq!(Rc::strong_count(&image), held);
	drop((commands, scene, image));
	finish();
}
