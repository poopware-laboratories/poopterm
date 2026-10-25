//! Tests of the window chrome as recorded scenes: each piece draws only
//! inside its own region, in every theme, size and scale, and keeps its
//! text as text a renderer can shape.
use super::{
	glyphs::FontSet,
	header::{draw_header, line},
	label::{Label, draw_label},
	popover::draw_popover,
	scene::{Command, Rect, Scene},
	sidebar::draw_sidebar,
	status_bar::draw_status_bar,
};
use crate::{
	fonts::{BUNDLED, Font},
	shell_integration::ShellIntegration,
	theme::{Theme, ThemeKind},
	ui::{
		header::{HeaderInput, TabInfo, header},
		overlays::{Place, command_palette, search_box},
		sidebar::{SidebarInput, SidebarState, sidebar},
		status_bar::{StatusInput, status_bar},
		tabs::{TabStripOptions, VerticalTabOptions, tab_layout_with_compaction, vertical_tab_layout},
		ui_font::text_width,
	},
};

fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new())
}

/// Every built-in theme.
fn themes() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

fn info(title: &str) -> TabInfo {
	TabInfo {
		title: title.to_owned(),
		style: Default::default(),
		marker: None,
		progress: None,
	}
}

fn inside((x, y, w, h): Rect, (bx, by, bw, bh): Rect) -> bool {
	w == 0 || h == 0 || (x >= bx && y >= by && x + w <= bx + bw && y + h <= by + bh)
}

/// Checks that nothing in `scene` reaches outside `bounds`: shapes by
/// their rectangle, clipped text by its clip, and other text by its advance
/// and line height.
fn assert_inside(scene: &Scene, bounds: Rect, what: &str) {
	let fonts = &mut fonts();
	let whole = (0, 0, scene.width, scene.height);
	assert!(!scene.commands.is_empty(), "{what}: nothing recorded");
	for command in &scene.commands {
		let reach = match *command {
			Command::Fill(_) => whole,
			Command::Rect(rect, _) | Command::RoundRect(rect, ..) | Command::Button(rect, ..) => rect,
			Command::Text { clip, .. } if clip != whole => clip,
			Command::Text {
				at, ref text, px, ..
			} => {
				let width = text_width(text, |ch| fonts.advance(ch, px));
				(at.0, at.1, width.max(1), line(fonts, px).0.max(1))
			}
		};
		assert!(
			inside(reach, bounds),
			"{what}: {command:?} leaves {bounds:?}"
		);
	}
}

fn texts(scene: &Scene) -> Vec<&str> {
	let mut texts = Vec::new();
	for command in &scene.commands {
		if let Command::Text { text, .. } = command {
			texts.push(text.as_str());
		}
	}
	texts
}

const TITLES: [&str; 5] = [
	"Shell",
	"cargo build",
	"a long title for a tab",
	"日本語",
	"🐱 cat",
];

#[test]
fn the_header_draws_only_inside_its_bar() {
	for theme in themes() {
		for (width, scale, count) in [(320, 1, 1), (800, 1, 3), (800, 2, 5), (1600, 2, 40)] {
			let tabs: Vec<_> = (0..count).map(|i| info(TITLES[i % TITLES.len()])).collect();
			let options = TabStripOptions {
				left_reserve: 0,
				scroll: 0,
				show_plus: true,
				show_settings: true,
				active_tab: count - 1,
				compact_tabs: count > 10,
			};
			let layout = tab_layout_with_compaction(width, count, scale, options);
			let shown =
				&tabs[layout.first_visible..count.min(layout.first_visible + layout.visible_count)];
			let height = 44 * scale;
			let model = header(&HeaderInput {
				layout: &layout,
				tabs: shown,
				count,
				active: count - 1,
				cursor: (layout.tab_x(0) as f64 + 10.0, 10.0),
				bounds: (0, height),
				px: 14.0 * scale as f32,
				theme: &theme,
			});
			let mut scene = Scene::new(width, height + 100);
			draw_header(&mut scene, &mut fonts(), &model);
			let what = format!("{:?} {width}x{scale} {count} tabs", theme.kind);
			assert_inside(&scene, (0, 0, width, height), &what);
		}
	}
}

#[test]
fn header_titles_are_recorded_as_text_not_pixels() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let tabs: Vec<_> = TITLES.iter().map(|title| info(title)).collect();
	let options = TabStripOptions {
		left_reserve: 0,
		scroll: 0,
		show_plus: true,
		show_settings: true,
		active_tab: 0,
		compact_tabs: false,
	};
	let layout = tab_layout_with_compaction(2000, tabs.len(), 1, options);
	let model = header(&HeaderInput {
		layout: &layout,
		tabs: &tabs,
		count: tabs.len(),
		active: 0,
		cursor: (-1.0, -1.0),
		bounds: (0, 44),
		px: 14.0,
		theme: &theme,
	});
	let mut scene = Scene::new(2000, 44);
	draw_header(&mut scene, &mut fonts(), &model);
	let texts = texts(&scene);
	// Tabs have a widest size, so long titles end in an ellipsis.
	for title in TITLES {
		let start: String = title.chars().take(8).collect();
		assert!(
			texts.iter().any(|text| text.starts_with(&start)),
			"{title} not in {texts:?}"
		);
	}
	assert!(texts.contains(&"a long title for…"), "{texts:?}");
}

#[test]
fn the_sidebar_draws_only_inside_its_column_as_rows_or_a_rail() {
	for theme in themes() {
		for (rail, scale, count) in [(false, 1, 3), (true, 1, 3), (false, 2, 30), (true, 2, 30)] {
			let height = 400 * scale;
			let options = VerticalTabOptions {
				top_reserve: 0,
				status_height: 26 * scale,
				scroll: 0,
				active_tab: 1,
				compact_tabs: rail,
				show_plus: true,
				show_settings: true,
			};
			let layout = vertical_tab_layout(height, count, scale, options);
			let shown = layout.first_visible..count.min(layout.first_visible + layout.visible_count);
			let state = SidebarState {
				tabs: shown.map(|i| info(TITLES[i % TITLES.len()])).collect(),
				first_visible: layout.first_visible,
				count,
				active: 1,
				target: None,
			};
			let model = sidebar(&SidebarInput {
				layout: &layout,
				state: &state,
				bounds: (0, height),
				px: 12.0 * scale as f32,
				theme: &theme,
			});
			let mut scene = Scene::new(1000, height);
			draw_sidebar(&mut scene, &mut fonts(), &model);
			let what = format!("{:?} rail {rail} x{scale} {count} tabs", theme.kind);
			assert_inside(&scene, model.area, &what);
			assert!(
				model.area.2 <= layout.sidebar_width,
				"{what}: wider than its column"
			);
		}
	}
}

#[test]
fn the_status_bar_draws_only_inside_its_strip() {
	let shell = ShellIntegration::default();
	let long = "a/very/long/working/directory/name/that/runs/on".repeat(4);
	for theme in themes() {
		for (width, scale, link) in [
			(200, 1, None),
			(900, 1, Some("https://example.com")),
			(900, 2, None),
		] {
			let input = StatusInput {
				link,
				place: Some("root@box"),
				cwd: &long,
				shell: &shell,
				size: (80, 24),
				theme: theme.kind.name(),
			};
			let rect = (0, 300, width, 26 * scale);
			let model = status_bar(&input, rect, (scale, 12.0 * scale as f32), &theme);
			let mut scene = Scene::new(width, 400);
			draw_status_bar(&mut scene, &mut fonts(), &model);
			assert_inside(&scene, rect, &format!("{:?} {width}x{scale}", theme.kind));
		}
	}
}

#[test]
fn popovers_draw_only_inside_their_frame() {
	let long = "a query much longer than any popover is wide ".repeat(6);
	for theme in themes() {
		for (window, scale) in [((320, 200), 1), ((900, 500), 1), ((1800, 1000), 2)] {
			let place = Place {
				window,
				top: 44 * scale,
				scale,
			};
			let px = 12.0 * scale as f32;
			for model in [
				search_box(place, ("", 0, 0), px, &theme),
				search_box(place, (&long, 12, 3), px, &theme),
				command_palette(place, ("", false), px, &theme),
				command_palette(place, ("zzz", false), px, &theme),
				command_palette(place, (&long, true), px, &theme),
			] {
				let mut scene = Scene::new(window.0, window.1);
				draw_popover(&mut scene, &mut fonts(), &model);
				let what = format!("{:?} {window:?}x{scale}", theme.kind);
				assert_inside(&scene, model.rect, &what);
			}
		}
	}
}

#[test]
fn labels_draw_only_inside_their_box_however_narrow() {
	let label = Label {
		title: "a title much too long for its box",
		status: Some("running"),
		colors: (0xffffff, 0x808080),
	};
	for width in [0, 1, 8, 40, 120, 300] {
		let bounds = (4, 4, width, 22);
		let mut scene = Scene::new(320, 30);
		draw_label(&mut scene, &mut fonts(), &label, 6, (13.0, 10), bounds);
		for command in &scene.commands {
			let Command::Text { clip, .. } = *command else {
				panic!("a label is only text: {command:?}");
			};
			assert!(inside(clip, bounds), "width {width}: {command:?}");
		}
	}
}
