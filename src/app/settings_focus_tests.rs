//! Tests of keyboard focus in the Settings window: which rectangles can be
//! focused and in what order, how Tab steps through them, and the ring.
use super::settings_focus::{center, focus_order, ring, step_focus};
use crate::render::{
	painter::ButtonStyle,
	scene::{Rect, Scene},
};

const STYLE: ButtonStyle = ButtonStyle {
	scale: 1,
	background: 0,
	border: 0,
	accent: 0,
};
const NAV: usize = 100;

fn everywhere(_: (usize, usize)) -> bool {
	true
}

#[test]
fn the_sidebar_comes_first_then_the_page_top_to_bottom_and_left_to_right() {
	let mut scene = Scene::new(600, 400);
	scene.button((300, 200, 40, 20), STYLE, false);
	scene.button((200, 50, 40, 20), STYLE, false);
	scene.focusable((8, 24, 80, 32));
	scene.button((300, 50, 40, 20), STYLE, false);
	scene.focusable((8, 60, 80, 32));
	assert_eq!(
		focus_order(&scene, NAV, everywhere),
		[
			(8, 24, 80, 32),
			(8, 60, 80, 32),
			(200, 50, 40, 20),
			(300, 50, 40, 20),
			(300, 200, 40, 20),
		]
	);
}

#[test]
fn a_rectangle_nothing_answers_a_click_on_is_left_out() {
	let mut scene = Scene::new(600, 400);
	scene.button((200, 50, 40, 20), STYLE, false);
	scene.button((300, 50, 40, 20), STYLE, false);
	let order = focus_order(&scene, NAV, |(x, _)| x < 250);
	assert_eq!(order, [(200, 50, 40, 20)]);
}

#[test]
fn the_click_that_decides_is_at_the_rectangles_center() {
	let mut scene = Scene::new(600, 400);
	scene.button((200, 50, 41, 21), STYLE, false);
	let mut seen = Vec::new();
	focus_order(&scene, NAV, |point| {
		seen.push(point);
		true
	});
	assert_eq!(seen, [(220, 60)]);
}

#[test]
fn a_rectangle_listed_twice_is_focused_once() {
	let mut scene = Scene::new(600, 400);
	scene.button((200, 50, 40, 20), STYLE, false);
	scene.focusable((200, 50, 40, 20));
	assert_eq!(focus_order(&scene, NAV, everywhere), [(200, 50, 40, 20)]);
}

#[test]
fn a_scene_with_nothing_focusable_has_an_empty_order() {
	assert!(focus_order(&Scene::new(10, 10), NAV, everywhere).is_empty());
}

#[test]
fn rows_a_pixel_apart_keep_their_vertical_order_whatever_their_columns() {
	let mut scene = Scene::new(600, 400);
	scene.button((500, 51, 10, 10), STYLE, false);
	scene.button((200, 50, 10, 10), STYLE, false);
	let order = focus_order(&scene, NAV, everywhere);
	assert_eq!(order, [(200, 50, 10, 10), (500, 51, 10, 10)]);
}

#[test]
fn tab_with_no_focus_goes_to_the_first_and_shift_tab_to_the_last() {
	assert_eq!(step_focus(None, 5, 1), Some(0));
	assert_eq!(step_focus(None, 5, -1), Some(4));
}

#[test]
fn tab_moves_one_control_and_wraps_at_both_ends() {
	assert_eq!(step_focus(Some(0), 5, 1), Some(1));
	assert_eq!(step_focus(Some(4), 5, 1), Some(0));
	assert_eq!(step_focus(Some(0), 5, -1), Some(4));
	assert_eq!(step_focus(Some(3), 5, -1), Some(2));
}

#[test]
fn nothing_to_focus_leaves_nothing_focused() {
	for current in [None, Some(0), Some(3)] {
		for delta in [-1, 1] {
			assert_eq!(step_focus(current, 0, delta), None);
		}
	}
}

#[test]
fn a_focus_past_the_controls_a_page_now_has_counts_as_none() {
	assert_eq!(step_focus(Some(9), 3, 1), Some(0));
	assert_eq!(step_focus(Some(9), 3, -1), Some(2));
}

#[test]
fn one_control_is_its_own_neighbor_both_ways() {
	assert_eq!(step_focus(Some(0), 1, 1), Some(0));
	assert_eq!(step_focus(Some(0), 1, -1), Some(0));
}

#[test]
fn a_whole_lap_of_tabs_visits_every_control_once_and_returns() {
	for count in 1..=8 {
		let mut at = None;
		let mut seen = Vec::new();
		for _ in 0..count {
			at = step_focus(at, count, 1);
			seen.push(at.unwrap());
		}
		assert_eq!(seen, (0..count).collect::<Vec<_>>(), "{count} controls");
		assert_eq!(step_focus(at, count, 1), Some(0));
	}
}

#[test]
fn the_ring_is_four_strokes_just_outside_the_rectangle() {
	let r: Rect = (50, 40, 100, 30);
	let strokes = ring(r, 1);
	assert_eq!(
		strokes,
		[
			(48, 38, 104, 2),
			(48, 70, 104, 2),
			(48, 38, 2, 34),
			(150, 38, 2, 34)
		]
	);
}

#[test]
fn the_ring_doubles_at_twice_the_scale_and_never_runs_off_the_left_or_top() {
	let strokes = ring((50, 40, 100, 30), 2);
	assert_eq!(strokes[0], (46, 36, 108, 4));
	let corner = ring((1, 1, 10, 10), 2);
	assert_eq!((corner[0].0, corner[0].1), (0, 0));
	assert!(corner.iter().all(|&(x, y, ..)| x < 20 && y < 20));
}

#[test]
fn the_center_of_a_rectangle_is_the_middle_pixel_rounded_down() {
	assert_eq!(center((10, 20, 40, 20)), (30, 30));
	assert_eq!(center((10, 20, 41, 21)), (30, 30));
	assert_eq!(center((0, 0, 1, 1)), (0, 0));
}

#[test]
fn drawing_the_ring_records_its_four_strokes_in_the_given_color() {
	use super::settings_focus::draw_ring;
	use crate::render::scene::Command;
	let mut scene = Scene::new(300, 200);
	draw_ring(&mut scene, (50, 40, 100, 30), 1, 0x3366cc);
	let strokes: Vec<_> = scene
		.commands
		.iter()
		.map(|command| match command {
			Command::Rect(rect, color) => (*rect, *color),
			other => panic!("{other:?}"),
		})
		.collect();
	let wanted: Vec<_> = ring((50, 40, 100, 30), 1)
		.into_iter()
		.map(|rect| (rect, 0x3366cc))
		.collect();
	assert_eq!(strokes, wanted);
}

#[test]
fn the_ring_leaves_the_controls_own_pixels_alone() {
	use super::settings_focus::draw_ring;
	use crate::render::painter::Painter;
	let mut scene = Scene::new(300, 200);
	draw_ring(&mut scene, (50, 40, 100, 30), 1, 0xff0000);
	let mut pixels = vec![0u32; 300 * 200];
	let fonts = &mut crate::render::glyphs::FontSet::new(
		crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap(),
		Vec::new(),
	);
	scene.replay(&mut Painter::new(&mut pixels, 300, 200), fonts);
	for y in 40..70 {
		for x in 50..150 {
			assert_eq!(pixels[y * 300 + x], 0, "inside at ({x}, {y})");
		}
	}
	assert_eq!(pixels[38 * 300 + 100], 0xff0000);
	assert_eq!(pixels[55 * 300 + 49], 0xff0000);
	assert_eq!(pixels[55 * 300 + 150], 0xff0000);
	assert_eq!(pixels[70 * 300 + 100], 0xff0000);
}

mod pages {
	//! The focus order of the real pages, composed as the window composes them.
	use super::super::{
		SettingsEditor,
		settings_focus::focus_order,
		settings_fonts::FontPicker,
		settings_hit::{SettingsPage, SettingsTarget},
		settings_nav::{NAV_WIDTH, compose_nav, window_target},
		settings_pen_tests::record,
		settings_view::{
			FONT_ROW, MIN_SIZE, SCROLLBACK_ROW, STRENGTH_ROW, THEME_ROW, compose_layout_settings,
			compose_main_settings,
		},
	};
	use crate::{
		config::Preferences,
		input::shortcuts::Shortcuts,
		render::scene::Scene,
		theme::{Theme, ThemeKind},
		ui::{chrome_colors::chrome_colors, diagnostics_page::Section},
	};

	fn editor(page: SettingsPage) -> SettingsEditor {
		SettingsEditor {
			theme: ThemeKind::MacOsDark,
			preferences: Preferences::default(),
			wallpaper_path: None,
			wallpaper_strength: 35,
			status: String::new(),
			page,
			recording_shortcut: None,
			shortcuts: Shortcuts::from_config(""),
			fonts: FontPicker::new(Vec::new(), ""),
			diagnostics_section: Section::Log,
			copied: String::new(),
		}
	}

	/// The window's scene for `page` at `scale`, and the focus order of it,
	/// each with the target a click at its center reaches.
	fn order(page: SettingsPage, scale: usize) -> Vec<(super::Rect, Option<SettingsTarget>)> {
		let size = ((MIN_SIZE.0 + NAV_WIDTH) * scale, MIN_SIZE.1 * scale);
		let nav = NAV_WIDTH * scale;
		let theme = Theme::from_kind(ThemeKind::MacOsDark);
		let colors = chrome_colors(&theme);
		let editor = editor(page);
		let mut window = Scene::new(size.0, size.1);
		let mut content = Scene::new(size.0 - nav, size.1);
		let commands = record(scale, |pen, p, style| {
			(p.width, p.height) = (size.0 - nav, size.1);
			match page {
				SettingsPage::Layout => compose_layout_settings(&editor, style, p, pen),
				_ => compose_main_settings(&editor, style, p, pen),
			}
			pen.footer(p, style, ("Save", 120), "", &|_, _, _, _| false);
		});
		content.commands = commands;
		let nav_commands = record(scale, |pen, p, _| {
			(p.width, p.height) = size;
			compose_nav(pen, p, (page, (-1.0, -1.0)), (&colors, theme.fg));
		});
		window.commands = nav_commands;
		window.commands.extend(content.shifted(nav).commands);
		let reaches = |point| window_target(point, size, scale, page);
		focus_order(&window, nav, |point| reaches(point).is_some())
			.into_iter()
			.map(|rect| (rect, reaches(super::center(rect))))
			.collect()
	}

	#[test]
	fn the_main_page_focuses_the_sidebar_then_every_control_down_the_page() {
		let found = order(SettingsPage::Main, 1);
		let targets: Vec<_> = found.iter().map(|(_, target)| target.unwrap()).collect();
		assert_eq!(
			targets
				.iter()
				.filter(|t| matches!(t, SettingsTarget::Open(_)))
				.count(),
			7
		);
		// The seven sidebar items are the first seven, then the page.
		assert!(
			targets[..7]
				.iter()
				.all(|t| matches!(t, SettingsTarget::Open(_)))
		);
		let page = &targets[7..];
		assert_eq!(page[0], SettingsTarget::Theme(-1));
		assert_eq!(page[1], SettingsTarget::Theme(1));
		assert_eq!(page[2], SettingsTarget::ChooseWallpaper);
		assert_eq!(page[3], SettingsTarget::ClearWallpaper);
		assert!(page.contains(&SettingsTarget::Step(STRENGTH_ROW, -1)));
		assert!(page.contains(&SettingsTarget::Step(FONT_ROW, 1)));
		assert!(page.contains(&SettingsTarget::Step(SCROLLBACK_ROW, -1)));
		assert!(page.contains(&SettingsTarget::ToggleRestore));
		assert_eq!(
			page.last(),
			Some(&SettingsTarget::Save),
			"the footer is last"
		);
	}

	#[test]
	fn the_pages_own_controls_run_top_to_bottom() {
		let found = order(SettingsPage::Main, 1);
		let page: Vec<_> = found
			.iter()
			.filter(|(rect, _)| rect.0 >= NAV_WIDTH)
			.collect();
		assert!(
			page
				.windows(2)
				.all(|pair| (pair[0].0.1, pair[0].0.0) <= (pair[1].0.1, pair[1].0.0))
		);
	}

	#[test]
	fn the_layout_page_focuses_its_switches_the_two_segments_and_the_padding_stepper() {
		let found = order(SettingsPage::Layout, 1);
		let targets: Vec<_> = found
			.iter()
			.skip(6)
			.map(|(_, target)| target.unwrap())
			.collect();
		assert!(targets.contains(&SettingsTarget::ToggleTabs));
		assert!(targets.contains(&SettingsTarget::SetVerticalTabs(false)));
		assert!(targets.contains(&SettingsTarget::SetVerticalTabs(true)));
		assert!(targets.contains(&SettingsTarget::ToggleBottomBar));
		assert!(
			targets
				.iter()
				.any(|t| matches!(t, SettingsTarget::Step(..)))
		);
		assert_eq!(targets.last(), Some(&SettingsTarget::Save));
	}

	#[test]
	fn every_focusable_control_is_a_real_click_target_at_every_scale() {
		for page in [SettingsPage::Main, SettingsPage::Layout] {
			for scale in [1, 2] {
				let found = order(page, scale);
				assert!(!found.is_empty(), "{page:?} x{scale}");
				assert!(
					found.iter().all(|(_, target)| target.is_some()),
					"{page:?} x{scale}"
				);
			}
		}
	}

	#[test]
	fn the_display_only_theme_name_box_is_not_in_the_tab_order() {
		let found = order(SettingsPage::Main, 1);
		let theme_box = found
			.iter()
			.any(|(rect, _)| rect.1 == THEME_ROW && rect.2 > 100 && rect.0 > NAV_WIDTH + 60);
		assert!(!theme_box);
	}

	#[test]
	fn no_two_controls_share_a_rectangle() {
		for page in [SettingsPage::Main, SettingsPage::Layout] {
			let rects: Vec<_> = order(page, 1).into_iter().map(|(rect, _)| rect).collect();
			for (index, rect) in rects.iter().enumerate() {
				assert!(!rects[index + 1..].contains(rect), "{page:?}: {rect:?}");
			}
		}
	}
}
