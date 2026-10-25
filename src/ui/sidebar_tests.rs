//! Tests of the sidebar model: what the pointer is over, and what each row
//! and button shows.
use super::{
	contrast::tab_text_colors,
	header::{TabInfo, header_colors},
	sidebar::{SidebarInput, SidebarState, SidebarTarget, row_rect, sidebar, sidebar_target},
	tabs::{VerticalTabLayout, VerticalTabOptions, vertical_tab_layout},
};
use crate::{
	fonts::{BUNDLED, Font},
	render::{
		glyphs::FontSet,
		header::{Badge, HeaderTab, Icon, Rect, close_rect},
		painter::Painter,
		scene::paint,
		sidebar::{draw_sidebar, rail_close_rect},
	},
	term::status::{Progress, ProgressState},
	theme::{Theme, ThemeKind},
	ui::tabs::Marker,
};

const HEIGHT: usize = 640;

fn theme() -> Theme {
	Theme::from_kind(ThemeKind::MacOsDark)
}

/// A sidebar `HEIGHT` tall for `count` tabs, scrolled to `scroll`, on the
/// rail when `rail`, with both buttons when `buttons`.
fn layout(count: usize, scroll: usize, rail: bool, buttons: bool) -> VerticalTabLayout {
	let options = VerticalTabOptions {
		top_reserve: 0,
		status_height: 26,
		scroll,
		active_tab: 0,
		compact_tabs: rail,
		show_plus: buttons,
		show_settings: buttons,
	};
	vertical_tab_layout(HEIGHT, count, 1, options)
}

fn center((x, y, w, h): Rect) -> (f64, f64) {
	((x + w / 2) as f64, (y + h / 2) as f64)
}

fn info(title: &str) -> TabInfo {
	TabInfo {
		title: title.to_owned(),
		style: Default::default(),
		marker: None,
		progress: None,
	}
}

/// The visible tabs of `layout` out of `count`, `active` one active and
/// the pointer over `target`.
fn state(
	layout: &VerticalTabLayout,
	count: usize,
	active: usize,
	target: Option<SidebarTarget>,
) -> SidebarState {
	let shown = layout.first_visible..count.min(layout.first_visible + layout.visible_count);
	SidebarState {
		tabs: shown.map(|index| info(&format!("tab {index}"))).collect(),
		first_visible: layout.first_visible,
		count,
		active,
		target,
	}
}

fn input<'a>(
	layout: &'a VerticalTabLayout,
	state: &'a SidebarState,
	theme: &'a Theme,
) -> SidebarInput<'a> {
	SidebarInput {
		layout,
		state,
		bounds: (0, HEIGHT),
		px: 12.0,
		theme,
	}
}

fn hover(index: usize) -> Option<SidebarTarget> {
	Some(SidebarTarget::Tab {
		index,
		close: false,
	})
}

#[test]
fn rows_are_inset_from_the_sidebar_and_stacked_with_a_gap() {
	let layout = layout(5, 0, false, true);
	assert_eq!(row_rect(&layout, 0), (6, 8, 172, 28));
	assert_eq!(row_rect(&layout, 1), (6, 40, 172, 28));
	let rail = self::layout(5, 0, true, true);
	assert_eq!(row_rect(&rail, 2), (6, 88, 52, 36));
}

#[test]
fn the_pointer_over_a_row_targets_its_tab() {
	let layout = layout(5, 0, false, true);
	for index in 0..5 {
		let target = sidebar_target(&layout, 5, center(row_rect(&layout, index)));
		assert_eq!(target, hover(index), "row {index}");
	}
}

#[test]
fn row_edges_are_inclusive_at_the_start_and_exclusive_at_the_end() {
	let layout = layout(5, 0, false, true);
	let (x, y, w, h) = row_rect(&layout, 1);
	let at = |x: usize, y: usize| sidebar_target(&layout, 5, (x as f64, y as f64));
	assert_eq!(at(x, y), hover(1));
	assert_eq!(at(x + 20, y + h - 1), hover(1));
	// The gap below the row, and the insets beside it, belong to no tab.
	assert_eq!(at(x + 20, y + h), None);
	assert_eq!(at(x - 1, y + 4), None);
	assert_eq!(at(x + w, y + 4), None);
	assert_eq!(at(x + 20, y - 1), None);
}

#[test]
fn rows_past_the_last_tab_target_nothing() {
	let layout = layout(3, 0, false, true);
	assert_eq!(
		sidebar_target(&layout, 3, center(row_rect(&layout, 3))),
		None
	);
	assert_eq!(
		sidebar_target(&layout, 3, center(row_rect(&layout, 2))),
		hover(2)
	);
}

#[test]
fn a_scrolled_sidebar_targets_tabs_from_the_first_visible() {
	let layout = layout(40, 10, false, true);
	assert_eq!(layout.first_visible, 10);
	let first = row_rect(&layout, 10);
	assert_eq!(first.1, 8, "the first visible tab sits at the top");
	assert_eq!(sidebar_target(&layout, 40, center(first)), hover(10));
	let last = layout.first_visible + layout.visible_count - 1;
	assert_eq!(
		sidebar_target(&layout, 40, center(row_rect(&layout, last))),
		hover(last)
	);
	// The row after the last visible one would cover the buttons.
	let past = row_rect(&layout, last + 1);
	assert_eq!(sidebar_target(&layout, 40, (100.0, past.1 as f64)), None);
}

#[test]
fn the_close_button_of_a_row_targets_closing_it() {
	let layout = layout(5, 0, false, true);
	let (x, y, w, h) = row_rect(&layout, 2);
	let close = close_rect(x, w, (y, h), 1);
	assert_eq!(
		sidebar_target(&layout, 5, center(close)),
		Some(SidebarTarget::Tab {
			index: 2,
			close: true
		})
	);
	// Just left of the button, the row itself.
	let left = ((close.0 - 1) as f64, center(close).1);
	assert_eq!(sidebar_target(&layout, 5, left), hover(2));
}

#[test]
fn a_lone_tab_cannot_be_closed() {
	let layout = layout(1, 0, false, true);
	let (x, y, w, h) = row_rect(&layout, 0);
	let close = center(close_rect(x, w, (y, h), 1));
	assert_eq!(sidebar_target(&layout, 1, close), hover(0));
	let rail = self::layout(1, 0, true, true);
	let close = center(rail_close_rect(row_rect(&rail, 0), 1));
	assert_eq!(sidebar_target(&rail, 1, close), hover(0));
}

#[test]
fn on_the_rail_the_close_button_is_in_the_top_right_corner() {
	let layout = layout(5, 0, true, true);
	let row = row_rect(&layout, 1);
	let close = rail_close_rect(row, 1);
	let closing = Some(SidebarTarget::Tab {
		index: 1,
		close: true,
	});
	assert_eq!(sidebar_target(&layout, 5, center(close)), closing);
	// The middle of the row, where the number is, only selects the tab.
	assert_eq!(sidebar_target(&layout, 5, center(row)), hover(1));
	let below = (center(close).0, (close.1 + close.3) as f64);
	assert_eq!(sidebar_target(&layout, 5, below), hover(1));
}

#[test]
fn buttons_target_their_icon_and_the_gap_between_them_nothing() {
	let layout = layout(5, 0, false, true);
	let y = layout.plus_y.unwrap();
	let size = (layout.button_width, layout.button_height);
	let plus = (layout.plus_x, y, size.0, size.1);
	let menu = (layout.settings_x, y, size.0, size.1);
	assert_eq!(
		sidebar_target(&layout, 5, center(plus)),
		Some(SidebarTarget::Button(Icon::Plus))
	);
	assert_eq!(
		sidebar_target(&layout, 5, center(menu)),
		Some(SidebarTarget::Button(Icon::Menu))
	);
	let gap = ((plus.0 + plus.2) as f64, center(plus).1);
	assert_eq!(sidebar_target(&layout, 5, gap), None);
	let above = (center(plus).0, (y - 1) as f64);
	assert_eq!(sidebar_target(&layout, 5, above), None);
}

#[test]
fn hidden_buttons_target_nothing() {
	let shown = layout(5, 0, false, true);
	let hidden = layout(5, 0, false, false);
	let plus = (
		shown.plus_x,
		shown.plus_y.unwrap(),
		shown.button_width,
		shown.button_height,
	);
	assert_eq!(sidebar_target(&hidden, 5, center(plus)), None);
	assert!(
		sidebar(&input(&hidden, &state(&hidden, 5, 0, None), &theme()))
			.buttons
			.is_empty()
	);
}

#[test]
fn above_the_first_row_targets_nothing() {
	let layout = layout(5, 0, false, true);
	assert_eq!(sidebar_target(&layout, 5, (50.0, 3.0)), None);
}

#[test]
fn every_visible_tab_gets_a_numbered_row_in_its_place() {
	let layout = layout(40, 10, false, true);
	let state = state(&layout, 40, 12, None);
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	assert_eq!(sidebar.rows.len(), layout.visible_count);
	for (local, row) in sidebar.rows.iter().enumerate() {
		let index = 10 + local;
		assert_eq!(row.number, index + 1);
		assert_eq!(row.rect, row_rect(&layout, index));
		assert_eq!((row.tab.x, row.tab.width), (6, 172));
		assert_eq!(row.tab.label.title, format!("tab {index}"));
	}
}

#[test]
fn the_active_row_is_raised_and_a_hovered_one_lit() {
	let layout = layout(4, 0, false, true);
	let state = state(&layout, 4, 1, hover(3));
	let theme = theme();
	let colors = header_colors(&theme);
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let fills: Vec<_> = sidebar.rows.iter().map(|row| row.tab.fill).collect();
	assert_eq!(fills, [None, Some(colors.raised), None, Some(colors.hover)]);
}

#[test]
fn the_active_and_hovered_rows_show_a_close_button() {
	let layout = layout(4, 0, false, true);
	let state = state(&layout, 4, 1, hover(3));
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let closes: Vec<_> = sidebar.rows.iter().map(|row| row.tab.close).collect();
	assert_eq!(closes, [None, Some(false), None, Some(false)]);
}

#[test]
fn the_close_button_under_the_pointer_lights_up() {
	let layout = layout(4, 0, false, true);
	let target = Some(SidebarTarget::Tab {
		index: 2,
		close: true,
	});
	let state = state(&layout, 4, 0, target);
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let closes: Vec<_> = sidebar.rows.iter().map(|row| row.tab.close).collect();
	assert_eq!(closes, [Some(false), None, Some(true), None]);
	assert_eq!(sidebar.rows[2].tab.fill, Some(header_colors(&theme).hover));
}

#[test]
fn a_lone_tab_shows_no_close_button() {
	let layout = layout(1, 0, false, true);
	let state = state(&layout, 1, 0, hover(0));
	let theme = theme();
	assert_eq!(
		sidebar(&input(&layout, &state, &theme)).rows[0].tab.close,
		None
	);
}

#[test]
fn on_the_rail_only_the_hovered_row_shows_a_close_button() {
	let layout = layout(4, 0, true, true);
	let state = state(&layout, 4, 1, hover(2));
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let closes: Vec<_> = sidebar.rows.iter().map(|row| row.tab.close).collect();
	assert_eq!(closes, [None, None, Some(false), None]);
	assert!(sidebar.rail);
	let hovered_active = self::state(&layout, 4, 1, hover(1));
	let rows = self::sidebar(&input(&layout, &hovered_active, &theme)).rows;
	assert_eq!(rows[1].tab.close, Some(false));
}

#[test]
fn rows_carry_their_tab_badge_and_progress() {
	let layout = layout(2, 0, false, true);
	let mut state = state(&layout, 2, 0, None);
	state.tabs[1].marker = Some(Marker::Bell);
	state.tabs[1].progress = Some(Progress {
		state: ProgressState::Error,
		percent: Some(30),
	});
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let (_, accent) = tab_text_colors(&theme);
	assert_eq!(sidebar.rows[1].tab.badge, Some(Badge::Bell(accent)));
	assert_eq!(
		sidebar.rows[1].tab.progress,
		Some((Some(30), theme.palette[1]))
	);
	assert_eq!(sidebar.rows[0].tab.badge, None);
}

#[test]
fn buttons_show_their_icons_where_the_layout_puts_them() {
	let layout = layout(3, 0, false, true);
	let target = Some(SidebarTarget::Button(Icon::Menu));
	let state = state(&layout, 3, 0, target);
	let theme = theme();
	let sidebar = sidebar(&input(&layout, &state, &theme));
	let y = layout.plus_y.unwrap();
	let buttons: Vec<_> = sidebar
		.buttons
		.iter()
		.map(|button| (button.rect, button.icon, button.hovered, button.enabled))
		.collect();
	assert_eq!(
		buttons,
		[
			((layout.plus_x, y, 32, 28), Icon::Plus, false, true),
			((layout.settings_x, y, 32, 28), Icon::Menu, true, true),
		]
	);
}

#[test]
fn the_sidebar_covers_its_bounds_in_the_header_colors() {
	let layout = layout(3, 0, false, true);
	let state = state(&layout, 3, 0, None);
	let theme = theme();
	let input = SidebarInput {
		bounds: (30, 500),
		px: 13.5,
		..input(&layout, &state, &theme)
	};
	let sidebar = sidebar(&input);
	assert_eq!(sidebar.area, (0, 30, 184, 500));
	assert_eq!(sidebar.style.px, 13.5);
	assert_eq!(sidebar.style.scale, 1);
	assert_eq!(sidebar.style.colors, header_colors(&theme));
	assert!(!sidebar.rail);
	let rail = self::layout(3, 0, true, true);
	assert_eq!(
		self::sidebar(&SidebarInput {
			layout: &rail,
			..input
		})
		.area
		.2,
		64
	);
}

#[test]
fn twice_the_scale_doubles_rows_and_close_buttons() {
	let options = |rail| VerticalTabOptions {
		top_reserve: 0,
		status_height: 52,
		scroll: 0,
		active_tab: 0,
		compact_tabs: rail,
		show_plus: true,
		show_settings: true,
	};
	let layout = vertical_tab_layout(2 * HEIGHT, 5, 2, options(false));
	let row = row_rect(&layout, 1);
	assert_eq!(row, (12, 80, 344, 56));
	let close = close_rect(row.0, row.2, (row.1, row.3), 2);
	let target = sidebar_target(&layout, 5, center(close));
	assert_eq!(
		target,
		Some(SidebarTarget::Tab {
			index: 1,
			close: true
		})
	);
	let rail = vertical_tab_layout(2 * HEIGHT, 5, 2, options(true));
	let close = rail_close_rect(row_rect(&rail, 1), 2);
	assert_eq!(close, (80, 100, 32, 32));
	let target = sidebar_target(&rail, 5, center(close));
	assert_eq!(
		target,
		Some(SidebarTarget::Tab {
			index: 1,
			close: true
		})
	);
}

#[test]
fn either_button_can_show_alone() {
	for (show_plus, icon) in [(false, Icon::Menu), (true, Icon::Plus)] {
		let options = VerticalTabOptions {
			top_reserve: 0,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: false,
			show_plus,
			show_settings: !show_plus,
		};
		let layout = vertical_tab_layout(HEIGHT, 3, 1, options);
		let theme = theme();
		let state = state(&layout, 3, 0, None);
		let buttons = sidebar(&input(&layout, &state, &theme)).buttons;
		assert_eq!(buttons.len(), 1);
		assert_eq!(buttons[0].icon, icon);
		// Alone, the button sits in the middle of the sidebar.
		assert_eq!(buttons[0].rect.0, (184 - 32) / 2);
		let target = sidebar_target(&layout, 3, center(buttons[0].rect));
		assert_eq!(target, Some(SidebarTarget::Button(icon)));
	}
}

/// Layouts to sweep: the full sidebar and the rail, at 1x and 2x, from the
/// top and scrolled.
fn sweep_layouts() -> Vec<(VerticalTabLayout, usize)> {
	let mut layouts = Vec::new();
	for scale in [1, 2] {
		for rail in [false, true] {
			for (count, scroll) in [(3, 0), (40, 7)] {
				let options = VerticalTabOptions {
					top_reserve: 0,
					status_height: 26 * scale,
					scroll,
					active_tab: 1,
					compact_tabs: rail,
					show_plus: true,
					show_settings: true,
				};
				let layout = vertical_tab_layout(HEIGHT * scale, count, scale, options);
				layouts.push((layout, count));
			}
		}
	}
	layouts
}

#[test]
fn the_row_lit_and_the_close_button_lit_are_the_ones_under_the_pointer() {
	let theme = theme();
	let colors = header_colors(&theme);
	for (layout, count) in sweep_layouts() {
		let scale = layout.scale;
		let at = format!("{scale}x, rail {}, {count} tabs", layout.compact_rail);
		let mut targets = Vec::new();
		// Every few pixels, and the pixels on each side of every edge.
		for y in (0..HEIGHT * scale)
			.step_by(5)
			.chain(edges_y(&layout, count))
		{
			for x in (0..layout.sidebar_width).step_by(5).chain(edges_x(&layout)) {
				let cursor = (x as f64, y as f64);
				let target = sidebar_target(&layout, count, cursor);
				if let Some(SidebarTarget::Tab { index, .. }) = target {
					assert!(
						contains(row_rect(&layout, index), cursor),
						"({x}, {y}) {at}"
					);
				}
				if !targets.contains(&target) {
					targets.push(target);
				}
			}
		}
		// What the sidebar shows depends on the pointer only through the target.
		for target in targets {
			let state = state(&layout, count, 1, target);
			let sidebar = sidebar(&input(&layout, &state, &theme));
			let rows = |want: &dyn Fn(&HeaderTab) -> bool| -> Vec<usize> {
				let rows = sidebar.rows.iter().filter(|row| want(&row.tab));
				rows.map(|row| row.number - 1).collect()
			};
			let lit = rows(&|tab| tab.fill == Some(colors.hover));
			let closing = rows(&|tab| tab.close == Some(true));
			let buttons = sidebar
				.buttons
				.iter()
				.filter(|button| button.hovered)
				.count();
			match target {
				Some(SidebarTarget::Tab { index, close }) => {
					// The active row stays raised under the pointer.
					let want: Vec<_> = Some(index)
						.filter(|&index| index != 1)
						.into_iter()
						.collect();
					assert_eq!(lit, want, "{target:?} {at}");
					assert_eq!(closing, if close { vec![index] } else { vec![] }, "{at}");
					assert_eq!(buttons, 0, "{at}");
				}
				Some(SidebarTarget::Button(_)) => {
					assert_eq!((lit.len(), closing.len(), buttons), (0, 0, 1), "{at}");
				}
				None => assert_eq!((lit.len(), closing.len(), buttons), (0, 0, 0), "{at}"),
			}
			let active = sidebar.rows.iter().find(|row| row.number == 2);
			assert!(
				active.is_none_or(|row| row.tab.fill == Some(colors.raised)),
				"{at}"
			);
		}
	}
}

#[test]
fn a_close_target_lies_inside_its_row_and_its_drawn_button() {
	for (layout, count) in sweep_layouts() {
		let scale = layout.scale;
		for y in (0..HEIGHT * scale)
			.step_by(2)
			.chain(edges_y(&layout, count))
		{
			for x in (0..layout.sidebar_width).step_by(2) {
				let cursor = (x as f64, y as f64);
				let Some(SidebarTarget::Tab { index, close: true }) =
					sidebar_target(&layout, count, cursor)
				else {
					continue;
				};
				let row = row_rect(&layout, index);
				let button = if layout.compact_rail {
					rail_close_rect(row, scale)
				} else {
					close_rect(row.0, row.2, (row.1, row.3), scale)
				};
				assert!(contains(button, cursor), "({x}, {y}) at {scale}x");
				assert!(contains(row, cursor), "({x}, {y}) at {scale}x");
			}
		}
	}
}

/// The rows on each side of every row's and button's top and bottom.
fn edges_y(layout: &VerticalTabLayout, count: usize) -> Vec<usize> {
	let shown = layout.first_visible..count.min(layout.first_visible + layout.visible_count);
	let rows = shown.map(|index| row_rect(layout, index));
	let buttons = layout.plus_y.map(|y| (0, y, 0, layout.button_height));
	let tops = rows.chain(buttons).flat_map(|(_, y, _, h)| [y, y + h]);
	tops
		.flat_map(|edge| [edge.saturating_sub(1), edge])
		.collect()
}

/// The columns on each side of the rows' and buttons' left and right
/// edges, and of the close buttons'.
fn edges_x(layout: &VerticalTabLayout) -> Vec<usize> {
	let scale = layout.scale;
	let row = row_rect(layout, layout.first_visible);
	let close = if layout.compact_rail {
		rail_close_rect(row, scale)
	} else {
		close_rect(row.0, row.2, (row.1, row.3), scale)
	};
	let button = layout.button_width;
	[row.0, row.0 + row.2, close.0, close.0 + close.2]
		.into_iter()
		.chain([layout.plus_x, layout.plus_x + button, layout.settings_x])
		.chain([layout.settings_x + button])
		.flat_map(|edge| [edge.saturating_sub(1), edge])
		.collect()
}

/// Whether `rect` holds `(x, y)`.
fn contains((left, top, width, height): Rect, (x, y): (f64, f64)) -> bool {
	x >= left as f64 && x < (left + width) as f64 && y >= top as f64 && y < (top + height) as f64
}

/// Whether two rectangles share a pixel.
fn overlap(a: Rect, b: Rect) -> bool {
	a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

#[test]
fn rows_and_buttons_stay_inside_the_sidebar_and_clear_of_each_other_at_any_height() {
	let theme = theme();
	for scale in [1, 2, 3] {
		for rail in [false, true] {
			// One row always shows, so the sidebar must be tall enough for it.
			for height in (160..900).step_by(37) {
				let options = VerticalTabOptions {
					top_reserve: 30 * scale,
					status_height: 26 * scale,
					scroll: 0,
					active_tab: 0,
					compact_tabs: rail,
					show_plus: true,
					show_settings: true,
				};
				let layout = vertical_tab_layout(height * scale, 50, scale, options);
				let state = state(&layout, 50, 0, None);
				let sidebar = sidebar(&input(&layout, &state, &theme));
				let at = format!("height {height} at {scale}x, rail {rail}");
				let mut parts: Vec<Rect> = sidebar.rows.iter().map(|row| row.rect).collect();
				parts.extend(sidebar.buttons.iter().map(|button| button.rect));
				for (number, &part) in parts.iter().enumerate() {
					// Clear of the separator down the right edge.
					assert!(
						part.0 + part.2 < layout.sidebar_width,
						"{at}: part {number}"
					);
					for &other in &parts[number + 1..] {
						assert!(!overlap(part, other), "{at}: {part:?} and {other:?}");
					}
				}
				for row in &sidebar.rows {
					let close = if rail {
						rail_close_rect(row.rect, scale)
					} else {
						close_rect(row.rect.0, row.rect.2, (row.rect.1, row.rect.3), scale)
					};
					let (x, y, w, h) = row.rect;
					assert!(close.0 >= x && close.0 + close.2 <= x + w, "{at}");
					assert!(close.1 >= y && close.1 + close.3 <= y + h, "{at}");
				}
			}
		}
	}
}

#[test]
fn on_the_rail_the_close_button_keeps_clear_of_the_centered_number() {
	for scale in [1, 2, 3] {
		let options = VerticalTabOptions {
			top_reserve: 0,
			status_height: 26 * scale,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		};
		let layout = vertical_tab_layout(HEIGHT * scale, 5, scale, options);
		let row = row_rect(&layout, 0);
		let close = rail_close_rect(row, scale);
		// Two digits at the text size fill about 16 pixels around the middle.
		let digits = (row.0 + row.2 / 2 - 8 * scale, row.1 + row.3 / 2 - 6 * scale);
		assert!(
			close.0 >= digits.0 + 16 * scale || close.1 + close.3 <= digits.1,
			"{scale}x"
		);
	}
}

/// The sidebar for `state`, drawn into a buffer as wide as it.
fn drawn(layout: &VerticalTabLayout, state: &SidebarState, theme: &Theme) -> Vec<u32> {
	let height = HEIGHT * layout.scale;
	let mut pixels = vec![0; layout.sidebar_width * height];
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut painter = Painter::new(&mut pixels, layout.sidebar_width, height);
	let input = SidebarInput {
		bounds: (0, height),
		px: 12.0 * layout.scale as f32,
		..input(layout, state, theme)
	};
	paint(&mut painter, &mut fonts, |scene, fonts| {
		draw_sidebar(scene, fonts, &sidebar(&input))
	});
	pixels
}

/// Layouts with three tabs, the full sidebar and the rail, at 1x and 2x.
fn small_layouts() -> Vec<VerticalTabLayout> {
	sweep_layouts()
		.into_iter()
		.filter(|(_, count)| *count == 3)
		.map(|(layout, _)| layout)
		.collect()
}

#[test]
fn pointing_at_a_row_lights_up_exactly_that_row_on_screen() {
	let theme = theme();
	let colors = header_colors(&theme);
	for layout in small_layouts() {
		let calm = drawn(&layout, &state(&layout, 3, 1, None), &theme);
		let row = row_rect(&layout, 2);
		// Just inside the row's left edge, halfway down: no title or badge there.
		let point = (row.0 + layout.scale, row.1 + row.3 / 2);
		let target = sidebar_target(&layout, 3, (point.0 as f64, point.1 as f64));
		assert_eq!(target, hover(2));
		let lit = drawn(&layout, &state(&layout, 3, 1, target), &theme);
		let width = layout.sidebar_width;
		assert_eq!(calm[point.1 * width + point.0], colors.bar);
		assert_eq!(lit[point.1 * width + point.0], colors.hover);
		// Only that row changes.
		for (index, (before, after)) in calm.iter().zip(&lit).enumerate() {
			if before != after {
				let (x, y) = (index % width, index / width);
				assert!(contains(row, (x as f64, y as f64)), "({x}, {y}) changed");
			}
		}
	}
}

#[test]
fn every_pixel_of_a_drawn_close_mark_closes_its_tab() {
	let theme = theme();
	for layout in small_layouts() {
		let row = row_rect(&layout, 0);
		let close = if layout.compact_rail {
			rail_close_rect(row, layout.scale)
		} else {
			close_rect(row.0, row.2, (row.1, row.3), layout.scale)
		};
		// The active tab shows its close mark on the full sidebar; on the
		// rail the pointer has to be over the tab.
		let state = state(&layout, 3, 0, hover(0));
		let pixels = drawn(&layout, &state, &theme);
		let width = layout.sidebar_width;
		let mut ink = 0;
		for y in row.1..row.1 + row.3 {
			for x in close.0..close.0 + close.2 {
				// The mark is drawn in the text color, lighter than any shade.
				if pixels[y * width + x] & 0xff > 0xb0 {
					ink += 1;
					let target = sidebar_target(&layout, 3, (x as f64, y as f64));
					let closing = Some(SidebarTarget::Tab {
						index: 0,
						close: true,
					});
					assert_eq!(target, closing, "({x}, {y}) at {}x", layout.scale);
				}
			}
		}
		assert!(ink > 0, "the mark is drawn at {}x", layout.scale);
	}
}

#[test]
fn every_pixel_of_a_drawn_icon_presses_its_button() {
	let theme = theme();
	for layout in small_layouts() {
		let pixels = drawn(&layout, &state(&layout, 3, 1, None), &theme);
		let width = layout.sidebar_width;
		let y = layout.plus_y.unwrap();
		let mut icons = Vec::new();
		for row in y..y + layout.button_height {
			for x in 0..width {
				if pixels[row * width + x] == theme.fg {
					icons.push(sidebar_target(&layout, 3, (x as f64, row as f64)));
				}
			}
		}
		let plus = Some(SidebarTarget::Button(Icon::Plus));
		let menu = Some(SidebarTarget::Button(Icon::Menu));
		assert!(icons.contains(&plus) && icons.contains(&menu), "{icons:?}");
		assert!(
			icons.iter().all(|icon| *icon == plus || *icon == menu),
			"{icons:?}"
		);
	}
}

#[test]
fn the_rail_draws_each_tab_number_inside_its_row() {
	let theme = theme();
	let layout = small_layouts()
		.into_iter()
		.find(|layout| layout.compact_rail)
		.unwrap();
	let pixels = drawn(&layout, &state(&layout, 3, 1, None), &theme);
	let width = layout.sidebar_width;
	for index in 0..3 {
		let (x, y, w, h) = row_rect(&layout, index);
		let ink = (y..y + h)
			.flat_map(|row| (x..x + w).map(move |column| (column, row)))
			.filter(|&(column, row)| pixels[row * width + column] & 0xff > 0xb0)
			.count();
		assert!(ink > 4, "tab {} shows its number", index + 1);
	}
}
