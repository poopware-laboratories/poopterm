//! Pixel tests of the popover painter: its outlined panel, the text field
//! with its caret, placeholder and trailing count, and rows below.
use super::{
	blend::blend,
	glyphs::FontSet,
	header::{HeaderColors, Rect, Style, pressed_fill},
	popover::{Field, Popover, PopoverButton, PopoverRow, draw_popover},
};
use crate::{
	fonts::{BUNDLED, Font},
	render::{painter::Painter, scene::paint},
};

/// What a selected row is filled with: the step past hover.
fn selected() -> u32 {
	pressed_fill(COLORS)
}

const WIDTH: usize = 400;
const HEIGHT: usize = 200;
const BACKGROUND: u32 = 0x000000;
// Text colors stand out by channel: red for fg, green for dim.
const COLORS: HeaderColors = HeaderColors {
	bar: 0x101010,
	separator: 0x0000a0,
	hover: 0x202020,
	raised: 0x303030,
	fg: 0xff0000,
	dim: 0x00ff00,
};
const PANEL: Rect = (20, 20, 300, 150);
const FIELD: Rect = (28, 28, 284, 34);

fn row(index: usize, text: &str, selected: bool) -> PopoverRow<'_> {
	PopoverRow {
		rect: (28, 66 + 32 * index, 284, 32),
		text,
		hint: None,
		selected,
	}
}

fn popover<'a>(text: &'a str, trailing: Option<&str>) -> Popover<'a> {
	Popover {
		rect: PANEL,
		style: Style {
			scale: 1,
			px: 13.0,
			colors: COLORS,
		},
		field: Field {
			rect: FIELD,
			text,
			placeholder: "Find",
			trailing: trailing.map(str::to_owned),
		},
		rows: Vec::new(),
		note: None,
		buttons: Vec::new(),
	}
}

struct Drawn(Vec<u32>);

impl Drawn {
	fn at(&self, x: usize, y: usize) -> u32 {
		self.0[y * WIDTH + x]
	}

	fn tinted(pixel: u32, color: u32) -> bool {
		let channel = |shift: u32| (pixel >> shift) & 0xff;
		let (red, green, blue) = (channel(16), channel(8), channel(0));
		if color == COLORS.fg {
			red > green.max(blue) + 0x30
		} else {
			green > red.max(blue) + 0x30
		}
	}

	/// The pixels in `rect` tinted toward `color`.
	fn points(&self, (x, y, w, h): Rect, color: u32) -> Vec<(usize, usize)> {
		(y..y + h)
			.flat_map(|row| (x..x + w).map(move |column| (column, row)))
			.filter(|&(column, row)| Self::tinted(self.at(column, row), color))
			.collect()
	}

	/// The bounds of the pixels in `rect` tinted toward `color`.
	fn ink(&self, rect: Rect, color: u32) -> Option<Rect> {
		let points = self.points(rect, color);
		let left = points.iter().map(|point| point.0).min()?;
		let right = points.iter().map(|point| point.0).max()?;
		let top = points.iter().map(|point| point.1).min()?;
		let bottom = points.iter().map(|point| point.1).max()?;
		Some((left, top, right - left + 1, bottom - top + 1))
	}
}

fn draw(popover: &Popover) -> Drawn {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![BACKGROUND; WIDTH * HEIGHT];
	let p = &mut Painter::new(&mut pixels, WIDTH, HEIGHT);
	paint(p, &mut fonts, |scene, fonts| {
		draw_popover(scene, fonts, popover)
	});
	Drawn(pixels)
}

#[test]
fn the_panel_is_outlined_and_rounded() {
	let drawn = draw(&popover("", None));
	// Top, bottom, left and right edges, then just inside them.
	for (edge, inside) in [
		((170, 20), (170, 21)),
		((170, 169), (170, 168)),
		((20, 100), (21, 100)),
		((319, 100), (318, 100)),
	] {
		assert_eq!(drawn.at(edge.0, edge.1), COLORS.separator, "{edge:?}");
		assert_eq!(drawn.at(inside.0, inside.1), COLORS.bar, "{inside:?}");
	}
	for (x, y) in [(19, 100), (170, 19), (320, 100), (170, 170)] {
		assert_eq!(drawn.at(x, y), BACKGROUND, "outside at ({x}, {y})");
	}
	// The corners are cut round.
	for (x, y) in [(20, 20), (319, 20), (20, 169), (319, 169)] {
		assert_eq!(drawn.at(x, y), BACKGROUND, "corner ({x}, {y})");
	}
}

#[test]
fn the_field_is_a_rounded_shade_inside_the_panel() {
	let drawn = draw(&popover("", None));
	assert_eq!(drawn.at(30, 45), COLORS.hover);
	assert_eq!(drawn.at(310, 45), COLORS.hover);
	assert_eq!(drawn.at(27, 45), COLORS.bar, "left of the field");
	assert_eq!(drawn.at(170, 62), COLORS.bar, "below the field");
	assert_eq!(drawn.at(28, 28), COLORS.bar, "the field's corner is round");
}

#[test]
fn an_empty_field_shows_the_caret_then_the_placeholder_dim() {
	let drawn = draw(&popover("", None));
	let caret = drawn.ink(FIELD, COLORS.fg).expect("the caret is drawn");
	assert_eq!((caret.0, caret.2), (40, 2), "{caret:?}");
	let placeholder = drawn.ink(FIELD, COLORS.dim).expect("the placeholder");
	assert!((44..48).contains(&placeholder.0), "{placeholder:?}");
	assert!(placeholder.0 + placeholder.2 < 120, "{placeholder:?}");
}

#[test]
fn the_caret_spans_a_line_centered_down_the_field() {
	let drawn = draw(&popover("", None));
	let (_, top, _, height) = drawn.ink(FIELD, COLORS.fg).unwrap();
	assert!((12..24).contains(&height), "{height}");
	let middle = 2 * top + height;
	assert!(middle.abs_diff(2 * 28 + 34) <= 2, "{top} + {height}");
}

#[test]
fn text_replaces_the_placeholder_and_the_caret_follows_it() {
	let drawn = draw(&popover("poop", None));
	assert!(drawn.ink(FIELD, COLORS.dim).is_none(), "no placeholder");
	let ink = drawn.ink(FIELD, COLORS.fg).unwrap();
	assert!((40..43).contains(&ink.0), "{ink:?}");
	// The caret is the rightmost ink: two columns a line tall.
	let right = ink.0 + ink.2 - 1;
	assert!(right > 60, "past the text: {ink:?}");
	let column = |x| drawn.points((x, 28, 1, 34), COLORS.fg).len();
	assert!(column(right) >= 12 && column(right - 1) >= 12, "{ink:?}");
}

#[test]
fn a_long_text_keeps_its_end_in_view() {
	// Wide glyphs at one end and narrow ones at the other: the end kept
	// decides how much ink lies just before the caret.
	let wide_end = format!("{}{}", "i".repeat(200), "W".repeat(8));
	let narrow_end = format!("{}{}", "W".repeat(200), "i".repeat(8));
	let near_caret = |text: &str| {
		let drawn = draw(&popover(text, None));
		let ink = drawn.ink(FIELD, COLORS.fg).unwrap();
		assert!(ink.0 + ink.2 <= 28 + 284 - 12, "{ink:?}");
		drawn
			.points((ink.0 + ink.2 - 40, 28, 36, 34), COLORS.fg)
			.len()
	};
	let (wide, narrow) = (near_caret(&wide_end), near_caret(&narrow_end));
	assert!(2 * wide > 3 * narrow, "{wide} and {narrow}");
}

#[test]
fn a_long_text_starts_with_an_ellipsis_at_the_padding() {
	let long = "W".repeat(100);
	let drawn = draw(&popover(&long, None));
	let ink = drawn.ink(FIELD, COLORS.fg).unwrap();
	assert!((40..44).contains(&ink.0), "{ink:?}");
	// The ellipsis is dots on the baseline, not a capital letter's height.
	let first = drawn.points((ink.0, 28, 4, 34), COLORS.fg);
	let top = first.iter().map(|point| point.1).min().unwrap();
	assert!(top > ink.1 + 5, "dots at {top}, ink from {}", ink.1);
}

#[test]
fn the_trailing_count_ends_at_the_padding_in_dim() {
	let drawn = draw(&popover("poop", Some("1 of 8")));
	let count = drawn.ink(FIELD, COLORS.dim).expect("the count is drawn");
	let right = count.0 + count.2;
	assert!(
		(28 + 284 - 15..=28 + 284 - 12).contains(&right),
		"{count:?}"
	);
	let text = drawn.ink(FIELD, COLORS.fg).unwrap();
	assert!(text.0 + text.2 < count.0, "{text:?} before {count:?}");
}

#[test]
fn a_long_text_stops_short_of_the_trailing_count() {
	let long = "W".repeat(100);
	let drawn = draw(&popover(&long, Some("No matches")));
	let text = drawn.ink(FIELD, COLORS.fg).unwrap();
	let count = drawn.ink(FIELD, COLORS.dim).unwrap();
	let gap = count.0 - (text.0 + text.2);
	assert!((8..20).contains(&gap), "a padding between them: {gap}");
}

#[test]
fn rows_show_their_text_and_the_selected_one_is_raised() {
	let mut popover = popover("", None);
	popover.rows = vec![row(0, "New tab", true), row(1, "Close tab", false)];
	let drawn = draw(&popover);
	assert_eq!(drawn.at(30, 66 + 16), selected(), "selected");
	assert_eq!(drawn.at(310, 66 + 16), selected(), "selected, right end");
	assert_eq!(drawn.at(28, 66), COLORS.bar, "a rounded corner");
	assert_eq!(drawn.at(30, 98 + 16), COLORS.bar, "not selected");
	for index in 0..2 {
		let rect = (28, 66 + 32 * index, 284, 32);
		let ink = drawn.ink(rect, COLORS.fg).expect("row text");
		assert!((40..43).contains(&ink.0), "row {index}: {ink:?}");
		let middle = 2 * ink.1 + ink.3;
		assert!(
			middle.abs_diff(2 * rect.1 + 32) <= 4,
			"row {index}: {ink:?}"
		);
	}
}

#[test]
fn a_long_row_is_cut_before_its_padding() {
	let long = "W".repeat(100);
	let mut popover = popover("", None);
	popover.rows = vec![row(0, &long, false)];
	let drawn = draw(&popover);
	let ink = drawn.ink((28, 66, 284, 32), COLORS.fg).unwrap();
	assert!(ink.0 + ink.2 <= 28 + 284 - 12, "{ink:?}");
	assert!(ink.0 + ink.2 > 28 + 284 - 40, "it fills the room: {ink:?}");
}

#[test]
fn a_note_is_dim_where_rows_would_go() {
	let mut popover = popover("zzz", None);
	popover.note = Some(((28, 66, 284, 32), "No matching commands"));
	let drawn = draw(&popover);
	let ink = drawn.ink((28, 66, 284, 32), COLORS.dim).expect("the note");
	assert!((40..43).contains(&ink.0), "{ink:?}");
	assert!(drawn.ink((28, 66, 284, 32), COLORS.fg).is_none());
}

#[test]
fn nothing_is_drawn_outside_the_panel() {
	let long = "W".repeat(100);
	let mut popover = popover(&long, Some("1 of 8"));
	popover.rows = vec![row(0, &long, true), row(1, &long, false)];
	popover.note = Some(((28, 130, 284, 32), "note"));
	let drawn = draw(&popover);
	for y in 0..HEIGHT {
		for x in 0..WIDTH {
			let outside = !(20..320).contains(&x) || !(20..170).contains(&y);
			if outside {
				assert_eq!(drawn.at(x, y), BACKGROUND, "({x}, {y})");
			}
		}
	}
}

#[test]
fn twice_the_scale_thickens_the_outline_and_caret_and_widens_the_padding() {
	let mut popover = popover("", None);
	popover.style.scale = 2;
	popover.style.px = 26.0;
	popover.field.rect = (24, 24, 292, 68);
	let drawn = draw(&popover);
	assert_eq!(drawn.at(170, 20), COLORS.separator);
	assert_eq!(drawn.at(170, 21), COLORS.separator);
	assert_eq!(drawn.at(170, 22), COLORS.bar);
	let caret = drawn.ink(popover.field.rect, COLORS.fg).unwrap();
	assert_eq!((caret.0, caret.2), (24 + 24, 4), "{caret:?}");
	let placeholder = drawn.ink(popover.field.rect, COLORS.dim).unwrap();
	assert!((56..62).contains(&placeholder.0), "{placeholder:?}");
}

#[test]
fn a_panel_too_small_for_its_outline_draws_without_panicking() {
	let mut popover = popover("poop", Some("1 of 8"));
	popover.rect = (20, 20, 1, 1);
	popover.field.rect = (20, 20, 0, 0);
	popover.rows = vec![PopoverRow {
		rect: (20, 20, 0, 0),
		text: "x",
		hint: None,
		selected: true,
	}];
	let drawn = draw(&popover);
	assert_eq!(drawn.at(100, 100), BACKGROUND);
}

#[test]
fn the_outline_stays_a_hairline_around_the_corners() {
	// The fill's corners follow the outline's, a pixel in; were they as
	// round as the outline's, it would thicken into solid wedges there.
	let drawn = draw(&popover("", None));
	for (left, top) in [(20, 20), (312, 20), (20, 162), (312, 162)] {
		let corner = (top..top + 8).flat_map(|y| (left..left + 8).map(move |x| (x, y)));
		let solid: Vec<_> = corner
			.filter(|&(x, y)| drawn.at(x, y) == COLORS.separator)
			.collect();
		assert!(solid.is_empty(), "corner at ({left}, {top}): {solid:?}");
	}
}

#[test]
fn the_caret_after_a_long_text_stays_inside_the_padding() {
	// Some width lets the text fill its room to the pixel; the caret must
	// still fit before the padding.
	let long = "i".repeat(200);
	for width in 200..240 {
		let mut popover = popover(&long, None);
		popover.field.rect = (28, 28, width, 34);
		let drawn = draw(&popover);
		let ink = drawn.ink(popover.field.rect, COLORS.fg).unwrap();
		assert!(ink.0 + ink.2 <= 28 + width - 12, "width {width}: {ink:?}");
	}
}

#[test]
fn a_long_count_leaves_the_text_half_the_field() {
	let long = "8".repeat(100);
	let drawn = draw(&popover("poop", Some(&long)));
	let count = drawn.ink(FIELD, COLORS.dim).unwrap();
	// The room inside the padding is 260 pixels; the count takes the right half.
	assert!(count.0 >= 28 + 12 + 130, "{count:?}");
	assert!(count.0 + count.2 <= 28 + 284 - 12, "{count:?}");
	let text = drawn.ink(FIELD, COLORS.fg).unwrap();
	assert!(text.0 + text.2 < count.0, "{text:?} before {count:?}");
}

/// The columns of `rect` holding pixels tinted toward `color`.
fn columns(drawn: &Drawn, (x, y, w, h): Rect, color: u32) -> Vec<bool> {
	(x..x + w)
		.map(|column| !drawn.points((column, y, 1, h), color).is_empty())
		.collect()
}

#[test]
fn a_long_placeholder_is_cut_before_the_padding() {
	let mut popover = popover("", None);
	popover.field.placeholder = "Run a command, any command at all";
	popover.field.rect = (28, 28, 120, 34);
	let drawn = draw(&popover);
	let ink = drawn.ink(popover.field.rect, COLORS.dim).unwrap();
	assert!(ink.0 + ink.2 <= 28 + 120 - 12, "{ink:?}");
	assert!(ink.0 + ink.2 > 28 + 120 - 40, "it fills the room: {ink:?}");
}

#[test]
fn a_placeholder_stops_a_padding_short_of_the_count() {
	let mut popover = popover("", Some("No matches"));
	popover.field.placeholder = "Run a command, any command at all, or several";
	let drawn = draw(&popover);
	let inked = columns(&drawn, FIELD, COLORS.dim);
	// The widest run of empty columns inside the dim text: the gap.
	let first = inked.iter().position(|&ink| ink).unwrap();
	let last = inked.iter().rposition(|&ink| ink).unwrap();
	let mut widest = 0;
	let mut run = 0;
	for &ink in &inked[first..=last] {
		run = if ink { 0 } else { run + 1 };
		widest = widest.max(run);
	}
	// At least the padding; at most that plus a glyph that did not fit.
	assert!((12..32).contains(&widest), "gap of {widest}");
}

#[test]
fn a_selected_row_at_twice_the_scale_has_rounder_corners_and_wider_padding() {
	let mut popover = popover("", None);
	popover.style.scale = 2;
	popover.style.px = 26.0;
	popover.rows = vec![PopoverRow {
		rect: (28, 70, 284, 64),
		text: "New tab",
		hint: None,
		selected: true,
	}];
	let drawn = draw(&popover);
	assert_eq!(drawn.at(30, 102), selected());
	// A 12-pixel radius leaves the pixel two in from the corner unfilled.
	assert_eq!(drawn.at(30, 72), COLORS.bar);
	let ink = drawn.ink((28, 70, 284, 64), COLORS.fg).unwrap();
	assert!((52..57).contains(&ink.0), "{ink:?}");
	assert!(ink.3 > 14, "the text doubles too: {ink:?}");
}

#[test]
fn a_note_at_twice_the_scale_keeps_twice_the_padding() {
	let mut popover = popover("", None);
	popover.style.scale = 2;
	popover.style.px = 26.0;
	popover.note = Some(((28, 70, 284, 64), "No matching commands"));
	let drawn = draw(&popover);
	let ink = drawn.ink((28, 70, 284, 64), COLORS.dim).unwrap();
	assert!((52..57).contains(&ink.0), "{ink:?}");
}

#[test]
fn rows_below_the_first_keep_their_text_inside_their_own_rect() {
	let mut popover = popover("", None);
	popover.rows = (0..3).map(|index| row(index, "Hxg", index == 1)).collect();
	let drawn = draw(&popover);
	for index in 0..3 {
		let rect = (28, 66 + 32 * index, 284, 32);
		let ink = drawn.ink(rect, COLORS.fg).unwrap();
		assert!(
			ink.1 > rect.1 + 2 && ink.1 + ink.3 < rect.1 + 30,
			"row {index}: {ink:?}"
		);
	}
	// Between the field and the first row, nothing but the panel.
	assert!(drawn.ink((28, 62, 284, 4), COLORS.fg).is_none());
}

#[test]
fn the_selected_row_fill_stays_inside_its_row() {
	let mut popover = popover("", None);
	popover.rows = vec![row(0, "New tab", false), row(1, "Close tab", true)];
	let drawn = draw(&popover);
	for x in [40, 170, 300] {
		assert_eq!(drawn.at(x, 97), COLORS.bar, "above, column {x}");
		assert_eq!(drawn.at(x, 98), selected(), "top row, column {x}");
		assert_eq!(drawn.at(x, 129), selected(), "bottom row, column {x}");
		assert_eq!(drawn.at(x, 130), COLORS.bar, "below, column {x}");
	}
	assert_eq!(drawn.at(27, 114), COLORS.bar, "left of the row");
	assert_eq!(drawn.at(312, 114), COLORS.bar, "right of the row");
}

/// Draws the search box and palette as the models place them, for every
/// theme, over a window of `fill`.
fn draw_models(
	window: (usize, usize),
	scale: usize,
	fill: u32,
	mut check: impl FnMut(&str, &Popover, &[u32]),
) {
	use crate::{
		theme::{Theme, ThemeKind},
		ui::overlays::{Place, command_palette, search_box},
	};
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let place = Place {
		window,
		top: 44 * scale,
		scale,
	};
	let mut kind = ThemeKind::MacOsDark;
	loop {
		let theme = Theme::from_kind(kind);
		let px = 12.0 * scale as f32;
		let long = "W".repeat(80);
		for popover in [
			search_box(place, (&long, 12, 3), px, &theme),
			command_palette(place, ("", false), px, &theme),
			command_palette(place, (&long, false), px, &theme),
			command_palette(place, (&long, true), px, &theme),
		] {
			let mut pixels = vec![fill; window.0 * window.1];
			let mut painter = Painter::new(&mut pixels, window.0, window.1);
			paint(&mut painter, &mut fonts, |scene, fonts| {
				draw_popover(scene, fonts, &popover)
			});
			check(kind.name(), &popover, &pixels);
		}
		kind = kind.next();
		if kind == ThemeKind::MacOsDark {
			break;
		}
	}
}

#[test]
fn every_theme_draws_the_popovers_opaque_and_only_inside_their_rects() {
	const FILL: u32 = 0xff00ff;
	for (window, scale) in [((800, 500), 1), ((1600, 1000), 2), ((260, 300), 1)] {
		draw_models(window, scale, FILL, |name, popover, pixels| {
			let (x, y, width, height) = popover.rect;
			let radius = 10 * scale;
			// Text that overflows lands near the popover.
			let near = |start: usize, length: usize, end: usize| {
				start.saturating_sub(64)..(start + length + 64).min(end)
			};
			for row in near(y, height, window.1) {
				for column in near(x, width, window.0) {
					let pixel = pixels[row * window.0 + column];
					let inside = (x..x + width).contains(&column) && (y..y + height).contains(&row);
					if !inside {
						assert_eq!(pixel, FILL, "{name}: ({column}, {row}) outside");
					}
					// Clear of the round corners, nothing shows through.
					let core = (x + radius..x + width - radius).contains(&column)
						|| (y + radius..y + height - radius).contains(&row);
					if inside && core {
						assert_ne!(pixel, FILL, "{name}: ({column}, {row}) shows through");
					}
				}
			}
		});
	}
}

#[test]
fn every_theme_draws_the_popover_text_in_its_foreground() {
	draw_models((800, 500), 1, 0, |name, popover, pixels| {
		let colors = popover.style.colors;
		let (x, y, width, _) = popover.field.rect;
		// The caret is a solid column of the foreground.
		let field: Vec<_> = (y..y + 34)
			.flat_map(|row| (x..x + width).map(move |column| (column, row)))
			.filter(|&(column, row)| pixels[row * 800 + column] == colors.fg)
			.collect();
		assert!(
			field.len() >= 2 * 12,
			"{name}: {} foreground pixels",
			field.len()
		);
		for row in &popover.rows {
			let (x, y, width, height) = row.rect;
			let background = if row.selected {
				pressed_fill(colors)
			} else {
				colors.bar
			};
			let ink = (y..y + height)
				.flat_map(|line| (x..x + width).map(move |column| line * 800 + column))
				.filter(|&index| pixels[index] != background)
				.count();
			assert!(ink > 20, "{name}: row {:?} has no text", row.text);
		}
	});
}

#[test]
fn a_long_note_is_cut_before_its_padding() {
	let mut popover = popover("", None);
	popover.note = Some(((28, 66, 120, 32), "No matching commands at all"));
	let drawn = draw(&popover);
	let ink = drawn.ink((0, 66, WIDTH, 32), COLORS.dim).unwrap();
	assert!(ink.0 + ink.2 <= 28 + 120 - 12, "{ink:?}");
	assert!(ink.0 + ink.2 > 28 + 120 - 40, "it fills the room: {ink:?}");
}

#[test]
fn rows_draw_their_text_from_their_own_left_edge() {
	let mut popover = popover("", None);
	popover.rows = vec![PopoverRow {
		rect: (100, 66, 200, 32),
		text: "New tab",
		hint: None,
		selected: false,
	}];
	let drawn = draw(&popover);
	let ink = drawn.ink((28, 66, 284, 32), COLORS.fg).unwrap();
	assert!((112..115).contains(&ink.0), "{ink:?}");
}

#[test]
fn the_count_at_twice_the_scale_ends_twice_the_padding_in() {
	let mut popover = popover("poop", Some("1 of 8"));
	popover.style.scale = 2;
	popover.style.px = 26.0;
	popover.field.rect = (24, 24, 292, 68);
	let drawn = draw(&popover);
	let count = drawn.ink(popover.field.rect, COLORS.dim).unwrap();
	let right = count.0 + count.2;
	assert!(
		(24 + 292 - 28..=24 + 292 - 24).contains(&right),
		"{count:?}"
	);
	assert!(count.3 > 14, "the count doubles too: {count:?}");
}

const ROW: Rect = (28, 66, 284, 32);

fn hinted<'a>(text: &'a str, hint: Option<&str>, selected: bool) -> Popover<'a> {
	let mut panel = popover("", None);
	let mut line = row(0, text, selected);
	line.hint = hint.map(str::to_owned);
	panel.rows = vec![line];
	panel
}

#[test]
fn a_hint_is_drawn_dim_against_the_right_end_of_its_row() {
	let drawn = draw(&hinted("New tab", Some("Ctrl+N"), false));
	let (x, _, w, _) = drawn.ink(ROW, COLORS.dim).expect("the hint is drawn");
	assert!(x > 28 + 142, "the hint sits in the right half, from {x}");
	let end = x + w;
	assert!(end <= 28 + 284 - 12, "inside the padding, ends at {end}");
	assert!(end > 28 + 284 - 12 - 4, "and runs up to it, ends at {end}");
	let (left, ..) = drawn.ink(ROW, COLORS.fg).expect("the command is drawn");
	assert!(left < 28 + 40, "the command stays at the left, from {left}");
}

#[test]
fn a_row_without_a_hint_draws_none() {
	let drawn = draw(&hinted("Settings", None, false));
	assert_eq!(drawn.ink(ROW, COLORS.dim), None);
}

#[test]
fn a_long_command_gives_way_to_its_hint() {
	let long = "Copy last command output to the clipboard now please";
	let drawn = draw(&hinted(long, Some("Ctrl+Shift+C"), false));
	let (x, _, w, _) = drawn.ink(ROW, COLORS.fg).expect("the command is drawn");
	let (hint, ..) = drawn.ink(ROW, COLORS.dim).expect("the hint is drawn");
	assert!(
		x + w < hint,
		"the command ends at {}, the hint starts at {hint}",
		x + w
	);
}

#[test]
fn a_selected_rows_hint_is_drawn_over_its_fill() {
	let drawn = draw(&hinted("New tab", Some("Ctrl+N"), true));
	assert_eq!(drawn.at(40, 67), selected());
	assert!(drawn.ink(ROW, COLORS.dim).is_some());
}

#[test]
fn a_hint_wider_than_the_row_is_cut_inside_its_padding() {
	let drawn = draw(&hinted(
		"A",
		Some("Ctrl+Shift+Alt+Super+PageDown+More+Keys+Here+Too"),
		false,
	));
	let (x, _, w, _) = drawn.ink(ROW, COLORS.dim).expect("some of the hint shows");
	assert!(
		x >= 28 + 12 && x + w <= 28 + 284 - 12,
		"from {x} to {}",
		x + w
	);
}

#[test]
fn a_hint_sits_twice_as_far_in_at_twice_the_scale() {
	let at = |scale: usize| {
		let mut panel = popover("", None);
		panel.style.scale = scale;
		let mut line = row(0, "New tab", false);
		line.rect = (28, 66, 284, 32);
		line.hint = Some("Ctrl+N".to_owned());
		panel.rows = vec![line];
		let drawn = draw(&panel);
		let (x, _, w, _) = drawn.ink(ROW, COLORS.dim).expect("the hint is drawn");
		28 + 284 - (x + w)
	};
	let (one, two) = (at(1), at(2));
	assert!((10..=14).contains(&one), "{one} pixels from the edge at 1x");
	assert!((22..=26).contains(&two), "{two} pixels from the edge at 2x");
}

#[test]
fn an_empty_hint_draws_nothing_and_takes_no_room() {
	let drawn = draw(&hinted("New tab", Some(""), false));
	assert_eq!(drawn.ink(ROW, COLORS.dim), None);
	let bare = draw(&hinted("New tab", None, false));
	assert_eq!(drawn.ink(ROW, COLORS.fg), bare.ink(ROW, COLORS.fg));
}

fn with_button(glyph: &str, hovered: bool, active: bool) -> Popover<'_> {
	let mut panel = popover("", None);
	panel.buttons = vec![PopoverButton {
		rect: (270, 100, 28, 28),
		glyph,
		hovered,
		active,
	}];
	panel
}

const BUTTON: Rect = (270, 100, 28, 28);

#[test]
fn a_resting_button_draws_its_glyph_dim_on_the_panel() {
	let drawn = draw(&with_button("×", false, false));
	assert_eq!(drawn.at(272, 112), COLORS.bar);
	assert!(drawn.ink(BUTTON, COLORS.dim).is_some());
	assert_eq!(drawn.ink(BUTTON, COLORS.fg), None);
}

#[test]
fn a_hovered_button_is_lit_with_its_glyph_at_full_strength() {
	let drawn = draw(&with_button("×", true, false));
	assert_eq!(drawn.at(273, 112), COLORS.hover);
	assert!(drawn.ink(BUTTON, COLORS.fg).is_some());
}

#[test]
fn an_active_button_is_a_step_brighter_than_a_hovered_one() {
	let hovered = draw(&with_button("Aa", true, false)).at(273, 112);
	let active = draw(&with_button("Aa", false, true)).at(273, 112);
	assert_eq!(active, blend(COLORS.hover, COLORS.fg, 32));
	assert!(active >> 16 & 0xff > hovered >> 16 & 0xff);
}

#[test]
fn a_lit_button_rounds_its_corners() {
	let drawn = draw(&with_button("×", true, false));
	assert_eq!(drawn.at(270, 100), COLORS.bar);
	assert_eq!(drawn.at(297, 127), COLORS.bar);
	assert_eq!(drawn.at(284, 100), COLORS.hover);
}

#[test]
fn a_wide_glyph_is_centered_on_its_button() {
	let drawn = draw(&with_button("Aa", false, false));
	let (x, _, w, _) = drawn.ink(BUTTON, COLORS.dim).expect("drawn");
	let (left, right) = (x - 270, 270 + 28 - (x + w));
	assert!(left.abs_diff(right) <= 2, "{left} against {right}");
}

#[test]
fn a_lit_button_rounds_by_the_control_radius_at_twice_the_scale() {
	let mut panel = with_button("×", true, false);
	panel.style.scale = 2;
	panel.buttons[0].rect = (270, 90, 56, 56);
	let drawn = draw(&panel);
	// One pixel in from a corner is far outside a 12-pixel arc.
	assert_eq!(drawn.at(271, 91), COLORS.bar);
	assert_eq!(drawn.at(298, 90), COLORS.hover);
}

#[test]
fn several_buttons_light_independently() {
	let mut panel = popover("", None);
	panel.buttons = [(0, false, false), (1, true, false), (2, false, true)]
		.into_iter()
		.map(|(index, hovered, active)| PopoverButton {
			rect: (200 + index * 34, 100, 28, 28),
			glyph: "x",
			hovered,
			active,
		})
		.collect();
	let drawn = draw(&panel);
	assert_eq!(drawn.at(203, 112), COLORS.bar);
	assert_eq!(drawn.at(237, 112), COLORS.hover);
	assert_eq!(drawn.at(271, 112), blend(COLORS.hover, COLORS.fg, 32));
}

#[test]
fn a_button_does_not_disturb_the_field_beside_it() {
	let plain = draw(&popover("hello", Some("1 of 2")));
	let mut with = popover("hello", Some("1 of 2"));
	with.buttons = vec![PopoverButton {
		rect: (270, 100, 28, 28),
		glyph: "×",
		hovered: true,
		active: false,
	}];
	let drawn = draw(&with);
	for y in 28..62 {
		for x in 28..312 {
			assert_eq!(drawn.at(x, y), plain.at(x, y), "({x}, {y})");
		}
	}
}

#[test]
fn a_button_without_a_glyph_is_only_its_light() {
	let mut panel = with_button("", true, false);
	panel.buttons[0].glyph = "";
	let drawn = draw(&panel);
	assert_eq!(drawn.at(273, 112), COLORS.hover);
	assert_eq!(drawn.ink(BUTTON, COLORS.fg), None);
}

#[test]
fn exactly_the_hovered_button_is_lit_wherever_each_sits() {
	for lit in 0..4 {
		let mut panel = popover("", None);
		panel.buttons = (0..4)
			.map(|index| PopoverButton {
				rect: (100 + index * 30, 100, 28, 28),
				glyph: "x",
				hovered: index == lit,
				active: false,
			})
			.collect();
		let drawn = draw(&panel);
		for index in 0..4 {
			let light = drawn.at(103 + index * 30, 112) == COLORS.hover;
			assert_eq!(light, index == lit, "button {index} with {lit} hovered");
		}
	}
}
