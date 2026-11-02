//! Tests of the window button icons as quads: drawn by the GPU as software
//! paints them, uploaded once per icon and scale, never sharing a slot.
use super::{
	quads::{Fill, QuadPen},
	quads_chrome_tests::{ATLAS, assert_draws_like_software, both},
};
use crate::render::{
	grid_case_tests::fonts,
	scene::Scene,
	window_icons::{WindowIcon, WindowIcon::*},
};

const ICONS: [WindowIcon; 4] = [Minimize, Maximize, Restore, Close];

#[test]
fn every_icon_draws_as_software_paints_it_at_every_scale() {
	for icon in ICONS {
		for scale in 1..=3 {
			let mut scene = Scene::new(48, 48);
			scene.rect(0, 0, 48, 48, 0x2c1e11);
			scene.icon((5, 7), icon, scale, 0xe8d5b0);
			assert_draws_like_software(&scene, &format!("{icon:?} at {scale}"));
		}
	}
}

#[test]
fn icons_on_a_red_close_hover_draw_alike_in_white() {
	let mut scene = Scene::new(40, 34);
	scene.round_rect(2, 1, 36, 32, 6, 0xc42b1c);
	scene.icon((14, 11), Close, 1, 0xffffff);
	assert_draws_like_software(&scene, "close hovered");
}

#[test]
fn an_icon_is_one_coverage_quad_its_size_at_its_place() {
	let mut scene = Scene::new(48, 48);
	scene.icon((5, 7), Restore, 2, 0x123456);
	let (_, _, pen) = both(&scene);
	assert_eq!(pen.quads.len(), 1);
	let quad = pen.quads[0];
	assert_eq!(
		(quad.at, quad.size, quad.color),
		((5, 7), (24, 24), 0x123456)
	);
	assert!(matches!(quad.fill, Fill::Coverage(_)), "{:?}", quad.fill);
}

#[test]
fn each_icon_and_scale_uploads_once_and_keeps_its_own_slot() {
	let mut pen = QuadPen::new((96, 48), ATLAS);
	let mut fonts = fonts();
	pen.begin_frame((96, 48));
	let mut scene = Scene::new(96, 48);
	for (index, icon) in ICONS.into_iter().enumerate() {
		scene.icon((index * 24, 0), icon, 1, 0xffffff);
		scene.icon((index * 24, 12), icon, 2, 0xffffff);
	}
	// Each again, in another color: the same bitmaps.
	for (index, icon) in ICONS.into_iter().enumerate() {
		scene.icon((index * 24, 36), icon, 1, 0x00ff00);
	}
	scene.replay(&mut pen, &mut fonts);
	assert_eq!(pen.uploads.len(), 8, "four icons at two scales");
	let slot = |quad: usize| match pen.quads[quad].fill {
		Fill::Coverage(slot) => (slot.page, slot.x, slot.y, slot.w, slot.h),
		fill => panic!("{fill:?}"),
	};
	let slots: Vec<_> = (0..8).map(slot).collect();
	for (a, first) in slots.iter().enumerate() {
		for second in &slots[a + 1..] {
			assert_ne!(first, second);
		}
	}
	for index in 0..4 {
		assert_eq!(slot(8 + index), slots[2 * index], "{:?}", ICONS[index]);
	}
}
