//! Tests of the window chrome as quads: a scene drawn with the quad pen, from
//! the atlas pixels its uploads sent, paints what replaying it in software
//! paints; rounded corners and UI glyphs upload once, and never share an
//! atlas slot with grid glyphs.
use super::{
	quads::{Fill, QuadPen},
	quads_tests::Gpu,
};
use crate::{
	render::{
		glyphs::{FontSet, Presentation, Shape},
		grid_case_tests::{BG, fonts},
		header::draw_header,
		painter::{ButtonStyle, Painter},
		pen::{Ink, Pen},
		popover::draw_popover,
		scene::{Command, Scene},
		sidebar::draw_sidebar,
		status_bar::draw_status_bar,
	},
	shell_integration::ShellIntegration,
	theme::{Theme, ThemeKind},
	ui::{
		header::{HeaderInput, TabInfo, header},
		overlays::{Place, command_palette, search_box},
		sidebar::{SidebarInput, SidebarState, sidebar},
		status_bar::{StatusInput, status_bar},
		tabs::{TabStripOptions, VerticalTabOptions, tab_layout_with_compaction, vertical_tab_layout},
	},
};

const ATLAS: (usize, usize) = (256, 4);

fn style(scale: usize) -> ButtonStyle {
	ButtonStyle {
		scale,
		background: 0x1e1e2e,
		border: 0x9090a0,
		accent: 0x3060ff,
	}
}

/// `scene` replayed in software over the background, and drawn as quads
/// by the stand-in GPU, with the pen that drew it.
fn both(scene: &Scene) -> (Vec<u32>, Vec<u32>, QuadPen) {
	let size = (scene.width, scene.height);
	let mut painted = vec![BG; size.0 * size.1];
	scene.replay(
		&mut Painter::new(&mut painted, size.0, size.1),
		&mut fonts(),
	);
	let mut pen = QuadPen::new(size, ATLAS);
	pen.begin_frame(size);
	scene.replay(&mut pen, &mut fonts());
	let mut gpu = Gpu::default();
	gpu.upload(&pen.uploads);
	(painted, gpu.draw(&pen), pen)
}

fn assert_draws_like_software(scene: &Scene, what: &str) {
	let (painted, drawn, pen) = both(scene);
	assert!(pen.complete, "{what}: the atlas ran out");
	assert!(painted.iter().any(|&pixel| pixel != BG), "{what}: blank");
	let differ = painted.iter().zip(&drawn).filter(|(a, b)| a != b).count();
	assert_eq!(differ, 0, "{what}: {differ} pixels differ");
}

#[test]
fn rounded_rectangles_of_every_radius_and_shape_draw_as_software_paints_them() {
	for radius in [0, 1, 2, 3, 6, 7, 12, 40] {
		for (w, h) in [(30, 20), (20, 30), (9, 9), (8, 8), (1, 1), (2, 40), (41, 3)] {
			let mut scene = Scene::new(64, 48);
			scene.round_rect(5, 4, w, h, radius, 0xd0a060);
			assert_draws_like_software(&scene, &format!("radius {radius}, {w}x{h}"));
		}
	}
}

#[test]
fn empty_rounded_rectangles_add_no_quads() {
	for (w, h) in [(0, 0), (0, 9), (9, 0)] {
		let mut pen = QuadPen::new((32, 32), ATLAS);
		pen.round_rect((4, 4, w, h), 3, 0xffffff);
		assert!(pen.quads.is_empty(), "{w}x{h}: {:?}", pen.quads);
		assert!(pen.uploads.is_empty());
	}
}

#[test]
fn rounded_rectangles_running_off_the_target_are_cut_alike() {
	for (x, y) in [(50, 4), (4, 36), (58, 42), (63, 47)] {
		let mut scene = Scene::new(64, 48);
		scene.rect(0, 0, 64, 48, 0x202020);
		scene.round_rect(x, y, 20, 16, 6, 0xd0a060);
		assert_draws_like_software(&scene, &format!("at {x},{y}"));
	}
}

#[test]
fn buttons_draw_as_software_paints_them_at_every_scale_hovered_or_not() {
	for scale in [1, 2, 3] {
		for hovered in [false, true] {
			for (w, h) in [(60, 24), (3, 3), (2 * scale, 2 * scale)] {
				let mut scene = Scene::new(80, 40);
				scene.button((6, 6, w * scale.min(2), h), style(scale), hovered);
				let what = format!("scale {scale}, hovered {hovered}, {w}x{h}");
				assert_draws_like_software(&scene, &what);
			}
		}
	}
}

#[test]
fn text_draws_as_software_paints_it_clipped_at_every_edge() {
	let text = "Tab 1 — ~/src/poopterm é ✓";
	for clip in [
		(0, 0, 200, 40),
		(10, 0, 40, 40),
		(0, 12, 200, 6),
		(30, 8, 1, 1),
		(190, 0, 10, 40),
	] {
		let mut scene = Scene::new(200, 40);
		scene.text_clipped(&mut fonts(), 4, 8, text, 16.0, 13, 0xe0e0e0, clip);
		// A box that shows nothing paints nothing in either.
		let (painted, drawn, _) = both(&scene);
		assert_eq!(painted, drawn, "clip {clip:?}");
	}
	let mut scene = Scene::new(200, 40);
	scene.text(&mut fonts(), 4, 8, text, 16.0, 13, 0xe0e0e0);
	assert_draws_like_software(&scene, "unclipped");
}

#[test]
fn text_stops_at_the_right_edge_of_its_clip_in_both() {
	let mut scene = Scene::new(120, 30);
	scene.text_clipped(
		&mut fonts(),
		0,
		4,
		&"m".repeat(500),
		16.0,
		13,
		0xffffff,
		(0, 0, 60, 30),
	);
	let (_, _, pen) = both(&scene);
	// One quad per character up to the edge, not five hundred.
	assert!(pen.quads.len() < 12, "{} quads", pen.quads.len());
	assert_draws_like_software(&scene, "a long line");
}

#[test]
fn a_fill_and_overlapping_operations_keep_their_order() {
	let mut scene = Scene::new(96, 48);
	scene.rect(2, 2, 10, 10, 0xff0000);
	scene.fill(0x334455);
	scene.round_rect(4, 4, 60, 30, 8, 0x8899aa);
	scene.text(&mut fonts(), 8, 10, "over", 16.0, 13, 0x101010);
	scene.button((40, 14, 50, 26), style(1), true);
	scene.rect(44, 18, 6, 6, 0x00ff00);
	assert_draws_like_software(&scene, "layered");
	let (_, _, pen) = both(&scene);
	assert_eq!(pen.quads[0].fill, Fill::Solid);
	assert_eq!((pen.quads[0].at, pen.quads[0].size), ((2, 2), (10, 10)));
	assert_eq!(
		(pen.quads[1].at, pen.quads[1].size),
		((0, 0), (96, 48)),
		"the fill"
	);
}

fn next(state: &mut u64) -> usize {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	(*state >> 16) as usize
}

#[test]
fn random_overlapping_scenes_draw_as_software_paints_them() {
	let mut state = 0x9e37_79b9_7f4a_7c15;
	for round in 0..40 {
		let mut scene = Scene::new(120, 80);
		scene.fill(next(&mut state) as u32 & 0x3f3f3f);
		for _ in 0..12 {
			let (x, y) = (next(&mut state) % 130, next(&mut state) % 90);
			let (w, h) = (next(&mut state) % 70, next(&mut state) % 50);
			let color = next(&mut state) as u32 & 0xff_ffff;
			match next(&mut state) % 4 {
				0 => scene.rect(x, y, w, h, color),
				1 => scene.round_rect(x, y, w, h, next(&mut state) % 20, color),
				2 => scene.button(
					(x, y, w, h),
					style(1 + next(&mut state) % 3),
					next(&mut state) % 2 == 0,
				),
				_ => scene.text_clipped(
					&mut fonts(),
					x,
					y,
					"Ab✓é",
					10.0 + (next(&mut state) % 20) as f32,
					12,
					color,
					(x / 2, y / 2, w + 10, h + 10),
				),
			}
		}
		let (painted, drawn, pen) = both(&scene);
		assert!(pen.complete, "round {round}");
		let differ = painted.iter().zip(&drawn).filter(|(a, b)| a != b).count();
		assert_eq!(differ, 0, "round {round}: {differ} pixels differ");
	}
}

/// Every built-in theme.
fn themes() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

const TITLES: [&str; 4] = [
	"zsh",
	"~/src/poopterm — cargo test",
	"vim TODO.md",
	"ssh box ✓",
];

/// A plain tab titled `title`.
fn tab(title: &str) -> TabInfo {
	TabInfo {
		title: title.to_owned(),
		style: Default::default(),
		marker: None,
		progress: None,
	}
}

#[test]
fn the_window_chrome_draws_as_software_paints_it_in_every_theme() {
	let shell = ShellIntegration::default();
	for theme in themes() {
		for (width, scale) in [(420, 1), (900, 2)] {
			let what = format!("{:?} at {scale}x", theme.kind);
			let tabs: Vec<_> = TITLES.iter().map(|title| tab(title)).collect();
			let options = TabStripOptions {
				left_reserve: 0,
				scroll: 0,
				show_plus: true,
				show_settings: true,
				active_tab: 1,
				compact_tabs: false,
			};
			let layout = tab_layout_with_compaction(width, tabs.len(), scale, options);
			let shown =
				&tabs[layout.first_visible..tabs.len().min(layout.first_visible + layout.visible_count)];
			let model = header(&HeaderInput {
				layout: &layout,
				tabs: shown,
				count: tabs.len(),
				active: 1,
				cursor: (layout.tab_x(0) as f64 + 10.0, 10.0),
				bounds: (0, 44 * scale),
				px: 14.0 * scale as f32,
				theme: &theme,
			});
			let height = 200 * scale;
			let mut scene = Scene::new(width, height);
			draw_header(&mut scene, &mut fonts(), &model);
			let input = StatusInput {
				link: Some("https://example.com"),
				place: Some("root@box"),
				cwd: "src/poopterm",
				shell: &shell,
				size: (80, 24),
				theme: theme.kind.name(),
			};
			let rect = (0, height - 26 * scale, width, 26 * scale);
			draw_status_bar(
				&mut scene,
				&mut fonts(),
				&status_bar(&input, rect, (scale, 12.0 * scale as f32), &theme),
			);
			let place = Place {
				window: (width, height),
				top: 44 * scale,
				scale,
			};
			let px = 12.0 * scale as f32;
			for model in [
				search_box(place, ("needle", 12, 3), px, &theme),
				command_palette(place, ("sp", false), px, &theme),
			] {
				draw_popover(&mut scene, &mut fonts(), &model);
			}
			assert!(
				scene
					.commands
					.iter()
					.any(|command| matches!(command, Command::RoundRect(..))),
				"{what}"
			);
			assert_draws_like_software(&scene, &what);
		}
	}
}

#[test]
fn the_vertical_tab_sidebar_draws_as_software_paints_it_as_rows_or_a_rail() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	for (compact, scale) in [(false, 1), (true, 1), (false, 2), (true, 2)] {
		let what = format!("compact {compact} at {scale}x");
		let height = 300 * scale;
		let options = VerticalTabOptions {
			top_reserve: 0,
			status_height: 26 * scale,
			scroll: 0,
			active_tab: 1,
			compact_tabs: compact,
			show_plus: true,
			show_settings: true,
		};
		let layout = vertical_tab_layout(height, TITLES.len(), scale, options);
		let state = SidebarState {
			tabs: TITLES.iter().map(|title| tab(title)).collect(),
			first_visible: 0,
			count: TITLES.len(),
			active: 1,
			target: None,
		};
		let model = sidebar(&SidebarInput {
			layout: &layout,
			state: &state,
			bounds: (0, height - 26 * scale),
			px: 13.0 * scale as f32,
			theme: &theme,
		});
		let mut scene = Scene::new(240 * scale, height);
		draw_sidebar(&mut scene, &mut fonts(), &model);
		assert!(
			scene
				.commands
				.iter()
				.any(|command| matches!(command, Command::Text { .. })),
			"{what}"
		);
		assert_draws_like_software(&scene, &what);
	}
}

#[test]
fn corners_upload_once_for_each_radius_and_quadrant() {
	let mut scene = Scene::new(100, 60);
	scene.round_rect(2, 2, 30, 20, 6, 0xff0000);
	scene.round_rect(40, 2, 50, 40, 6, 0x00ff00);
	scene.round_rect(2, 30, 20, 20, 4, 0x0000ff);
	let mut pen = QuadPen::new((100, 60), ATLAS);
	pen.begin_frame((100, 60));
	scene.replay(&mut pen, &mut fonts());
	// Four corners of radius 6, then four of radius 4.
	assert_eq!(pen.uploads.len(), 8);
	assert!(pen.uploads.iter().all(|upload| {
		upload
			.pixels
			.iter()
			.all(|texel| texel & 0xff_ffff == 0xff_ffff)
	}));
	let quads = pen.quads.clone();
	pen.begin_frame((100, 60));
	scene.replay(&mut pen, &mut fonts());
	assert!(pen.uploads.is_empty(), "a second frame uploads nothing");
	assert_eq!(pen.quads, quads);
}

#[test]
fn ui_glyphs_and_grid_glyphs_of_one_character_and_size_keep_their_own_slots() {
	// The grid and the chrome draw with different font sets.
	let (mut grid, mut ui) = (
		fonts(),
		FontSet::new(
			crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap(),
			Vec::new(),
		),
	);
	let mut pen = QuadPen::new((40, 40), ATLAS);
	pen.begin_frame((40, 40));
	let shape = Shape::Char('a', Presentation::Default);
	pen.glyph(
		grid.glyph('a', 16.0),
		Ink::Text(shape, 16.0),
		(0, 0),
		(0, 0, 40, 40),
		false,
		1,
	);
	pen.glyph(
		ui.glyph('a', 16.0),
		Ink::Ui('a', 16.0),
		(20, 0),
		(0, 0, 40, 40),
		false,
		1,
	);
	pen.glyph(
		ui.glyph('a', 16.0),
		Ink::Ui('a', 16.0),
		(20, 20),
		(0, 0, 40, 40),
		false,
		1,
	);
	// A cluster of one character has a key as long as a UI glyph's.
	pen.glyph(
		grid.glyph('a', 16.0),
		Ink::Text(Shape::Cluster(&['a']), 16.0),
		(0, 20),
		(0, 0, 40, 40),
		false,
		1,
	);
	assert_eq!(pen.uploads.len(), 3, "one each, the second UI glyph reused");
	let slot = |index: usize| match pen.quads[index].fill {
		Fill::Coverage(slot) => slot,
		fill => panic!("{fill:?}"),
	};
	assert_ne!(slot(0), slot(1));
	assert_eq!(slot(1), slot(2));
	assert_ne!(slot(3), slot(1), "the cluster is not the UI glyph");
}

#[test]
fn a_full_atlas_leaves_a_chrome_frame_incomplete() {
	let mut pen = QuadPen::new((64, 64), (8, 1));
	pen.begin_frame((64, 64));
	pen.round_rect((0, 0, 60, 60), 20, 0xffffff);
	assert!(
		!pen.complete,
		"a 20 pixel corner does not fit an 8 pixel page"
	);
	assert_eq!(pen.quads.len(), 3, "the straight parts are still drawn");
}
