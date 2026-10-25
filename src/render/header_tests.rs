//! Pixel tests of the header bar: tabs, their parts and the icon buttons.
use super::{
	glyphs::FontSet,
	header::{
		Badge, Header, HeaderButton, HeaderColors, HeaderTab, Icon, Rect, close_rect, draw_header, pill,
	},
	label::Label,
};
use crate::{
	fonts::{BUNDLED, Font},
	render::painter::Painter,
};

const WIDTH: usize = 400;
const HEIGHT: usize = 100;
const COLORS: HeaderColors = HeaderColors {
	bar: 0x202020,
	separator: 0x505050,
	hover: 0x303030,
	raised: 0x404040,
	fg: 0xffffff,
	dim: 0x808080,
};

fn tab(x: usize, width: usize, title: &str) -> HeaderTab<'_> {
	HeaderTab {
		x,
		width,
		label: Label {
			title,
			status: None,
			colors: (COLORS.fg, COLORS.dim),
		},
		fill: None,
		badge: None,
		progress: None,
		close: None,
	}
}

fn header<'a>(scale: usize, tabs: Vec<HeaderTab<'a>>, buttons: Vec<HeaderButton>) -> Header<'a> {
	Header {
		y: 4,
		height: 44 * scale,
		scale,
		px: 15.0 * scale as f32,
		colors: COLORS,
		tabs,
		buttons,
	}
}

struct Drawn(Vec<u32>);

impl Drawn {
	fn at(&self, x: usize, y: usize) -> u32 {
		self.0[y * WIDTH + x]
	}

	/// Whether any pixel of `rect` is `color`.
	fn any(&self, (x, y, w, h): Rect, color: u32) -> bool {
		(y..y + h).any(|row| (x..x + w).any(|column| self.at(column, row) == color))
	}

	/// Whether any pixel of `rect` is tinted toward white, as text is.
	fn inked(&self, (x, y, w, h): Rect) -> bool {
		let light = |pixel: u32| (pixel & 0xff) > 0x50 && pixel != COLORS.dim;
		(y..y + h).any(|row| (x..x + w).any(|column| light(self.at(column, row))))
	}

	/// The first and last rows holding text ink.
	fn ink_rows(&self, (x, y, w, h): Rect) -> (usize, usize) {
		let rows: Vec<_> = (y..y + h)
			.filter(|&row| self.inked((x, row, w, 1)))
			.collect();
		(rows[0], *rows.last().unwrap())
	}
}

fn draw(header: &Header) -> Drawn {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![0; WIDTH * HEIGHT];
	draw_header(
		&mut Painter::new(&mut pixels, WIDTH, HEIGHT),
		&mut fonts,
		header,
	);
	Drawn(pixels)
}

#[test]
fn the_bar_fills_the_header_above_a_separator() {
	let drawn = draw(&header(1, Vec::new(), Vec::new()));
	assert_eq!(drawn.at(0, 3), 0, "nothing above the header");
	assert_eq!(drawn.at(0, 4), COLORS.bar);
	assert_eq!(drawn.at(WIDTH - 1, 46), COLORS.bar);
	assert_eq!(drawn.at(0, 47), COLORS.separator);
	assert_eq!(drawn.at(WIDTH - 1, 47), COLORS.separator);
	assert_eq!(drawn.at(0, 48), 0, "nothing below the header");
}

#[test]
fn a_filled_tab_is_a_rounded_surface_inset_from_the_bar() {
	let mut raised = tab(20, 160, "");
	raised.fill = Some(COLORS.raised);
	let drawn = draw(&header(1, vec![raised], Vec::new()));
	let (x, y, w, h) = pill(20, 160, (4, 44), 1);
	assert_eq!((x, y, w, h), (20, 10, 160, 32));
	assert_eq!(drawn.at(x + w / 2, y), COLORS.raised);
	assert_eq!(drawn.at(x + w / 2, y + h - 1), COLORS.raised);
	assert_eq!(drawn.at(x + w / 2, y - 1), COLORS.bar);
	assert_eq!(drawn.at(x + w / 2, y + h), COLORS.bar);
	// Rounded: the corner pixel stays the bar's.
	assert_eq!(drawn.at(x, y), COLORS.bar);
	assert_eq!(drawn.at(x + w - 1, y + h - 1), COLORS.bar);
}

#[test]
fn a_tab_without_fill_shows_the_bar() {
	let drawn = draw(&header(1, vec![tab(20, 160, "")], Vec::new()));
	assert!(!drawn.any((0, 4, WIDTH, 43), COLORS.raised));
	assert_eq!(drawn.at(100, 25), COLORS.bar);
}

#[test]
fn titles_are_centered_vertically_in_the_header() {
	// "Hg" reaches from the cap height to the descender.
	for scale in [1, 2] {
		let drawn = draw(&header(scale, vec![tab(10, 180, "Hg")], Vec::new()));
		let height = 44 * scale;
		let (top, bottom) = drawn.ink_rows((10, 4, 180, height));
		let (above, below) = (top - 4, 4 + height - 1 - bottom);
		assert!(
			above.abs_diff(below) <= 3 * scale,
			"{above} above, {below} below at {scale}x"
		);
	}
}

#[test]
fn titles_start_after_the_padding_and_never_reach_the_close_button() {
	let mut closable = tab(10, 120, "cargo build --release --all-targets");
	closable.close = Some(false);
	let drawn = draw(&header(1, vec![closable], Vec::new()));
	let (cx, ..) = close_rect(10, 120, (4, 44), 1);
	assert!(!drawn.inked((10, 10, 12, 32)), "ink in the left padding");
	assert!(drawn.inked((22, 10, 20, 32)));
	// Between the title's room and the close button's glyph.
	assert!(
		!drawn.inked((cx - 2, 10, 4, 32)),
		"the title runs into the close button"
	);
}

#[test]
fn the_close_button_shows_only_when_asked_and_lights_up_when_hovered() {
	let rect = close_rect(10, 160, (4, 44), 1);
	assert_eq!(rect, (10 + 160 - 28, 15, 22, 22));
	let mut shown = tab(10, 160, "");
	shown.close = Some(false);
	let drawn = draw(&header(1, vec![shown], Vec::new()));
	assert!(drawn.inked(rect));
	assert!(!drawn.any(rect, COLORS.hover) && drawn.at(rect.0 + 11, rect.1 + 1) == COLORS.bar);
	let drawn = draw(&header(1, vec![tab(10, 160, "")], Vec::new()));
	assert!(!drawn.inked(rect));
	let mut hovered = tab(10, 160, "");
	hovered.close = Some(true);
	let drawn = draw(&header(1, vec![hovered], Vec::new()));
	// A round light: lit just inside its top, not in its corner.
	assert_ne!(drawn.at(rect.0 + 11, rect.1 + 1), COLORS.bar);
	assert_eq!(drawn.at(rect.0, rect.1), COLORS.bar);
}

#[test]
fn a_dot_badge_comes_before_the_title_which_moves_right() {
	let mut dotted = tab(10, 200, "Hg");
	dotted.badge = Some(Badge::Dot(0x00ff00));
	let drawn = draw(&header(1, vec![dotted], Vec::new()));
	// A 6-pixel dot 12 pixels in, centered on the header.
	assert_eq!(drawn.at(22 + 3, 4 + 22), 0x00ff00);
	assert!(!drawn.any((40, 4, 170, 44), 0x00ff00));
	assert!(!drawn.inked((10, 10, 26, 32)), "the title overlaps the dot");
	assert!(drawn.inked((36, 10, 30, 32)));
}

#[test]
fn a_bell_badge_is_a_mark_in_its_color() {
	let mut ringing = tab(10, 200, "");
	ringing.badge = Some(Badge::Bell(0xff0000));
	let drawn = draw(&header(1, vec![ringing], Vec::new()));
	let red = |pixel: u32| pixel >> 16 > 0x80 && pixel & 0xff < 0x40;
	let columns: Vec<_> = (10..210)
		.filter(|&x| (4..48).any(|y| red(drawn.at(x, y))))
		.collect();
	assert!(!columns.is_empty() && columns[0] >= 22 && *columns.last().unwrap() < 36);
}

#[test]
fn progress_fills_its_share_of_a_track_along_the_tab_bottom() {
	let mut busy = tab(10, 224, "");
	busy.fill = Some(COLORS.raised);
	busy.progress = Some((Some(50), 0x0000ff));
	let drawn = draw(&header(1, vec![busy], Vec::new()));
	// The track runs 12 pixels in from each side, 3 above the tab's bottom.
	let (x, y, w, h) = pill(10, 224, (4, 44), 1);
	let row = y + h - 3;
	let blue = |x| drawn.at(x, row) == 0x0000ff;
	assert!(blue(x + 12) && blue(x + 12 + 99));
	assert!(!blue(x + 12 + 101) && drawn.at(x + 12 + 101, row) != COLORS.raised);
	assert_eq!(
		drawn.at(x + w - 12, row),
		COLORS.raised,
		"the track ends 12 in"
	);
	assert_eq!(drawn.at(x + 11, row), COLORS.raised);
	// An unknown share is a full, dimmer bar.
	let mut unknown = tab(10, 224, "");
	unknown.fill = Some(COLORS.raised);
	unknown.progress = Some((None, 0x0000ff));
	let drawn = draw(&header(1, vec![unknown], Vec::new()));
	let end = drawn.at(x + w - 13, row);
	assert!(end != COLORS.raised && end != 0x0000ff && end & 0xff > 0x40);
}

fn button(icon: Icon, hovered: bool, enabled: bool) -> HeaderButton {
	HeaderButton {
		rect: (300, 10, 36, 32),
		icon,
		hovered,
		enabled,
	}
}

#[test]
fn the_plus_icon_is_a_cross_of_two_bars() {
	let drawn = draw(&header(
		1,
		Vec::new(),
		vec![button(Icon::Plus, false, true)],
	));
	let (cx, cy) = (318, 26);
	for (x, y) in [
		(cx, cy),
		(cx - 5, cy),
		(cx + 5, cy),
		(cx, cy - 5),
		(cx, cy + 5),
	] {
		assert_eq!(drawn.at(x, y), COLORS.fg, "({x}, {y})");
	}
	for (x, y) in [
		(cx - 4, cy - 4),
		(cx + 4, cy + 4),
		(cx - 8, cy),
		(cx, cy + 8),
	] {
		assert_eq!(drawn.at(x, y), COLORS.bar, "({x}, {y})");
	}
}

#[test]
fn the_menu_icon_is_three_bars() {
	let drawn = draw(&header(
		1,
		Vec::new(),
		vec![button(Icon::Menu, false, true)],
	));
	let (cx, cy) = (318, 26);
	for y in [cy - 5, cy, cy + 5] {
		assert_eq!(drawn.at(cx - 6, y), COLORS.fg, "row {y}");
		assert_eq!(drawn.at(cx + 6, y), COLORS.fg, "row {y}");
	}
	for y in [cy - 3, cy + 3, cy - 8, cy + 8] {
		assert_eq!(drawn.at(cx, y), COLORS.bar, "row {y}");
	}
}

#[test]
fn hovered_buttons_get_a_rounded_light_and_flat_ones_none() {
	let drawn = draw(&header(
		1,
		Vec::new(),
		vec![button(Icon::Plus, false, true)],
	));
	assert!(!drawn.any((300, 10, 36, 32), COLORS.hover));
	let drawn = draw(&header(1, Vec::new(), vec![button(Icon::Plus, true, true)]));
	assert_eq!(drawn.at(302, 26), COLORS.hover);
	assert_eq!(drawn.at(300, 10), COLORS.bar, "rounded corner");
	// A disabled button does not light up.
	let drawn = draw(&header(
		1,
		Vec::new(),
		vec![button(Icon::Plus, true, false)],
	));
	assert!(!drawn.any((300, 10, 36, 32), COLORS.hover));
}

#[test]
fn arrows_are_centered_and_dim_when_disabled() {
	for icon in [Icon::Left, Icon::Right] {
		let drawn = draw(&header(1, Vec::new(), vec![button(icon, false, true)]));
		assert!(drawn.inked((310, 16, 16, 20)), "{icon:?}");
		assert!(!drawn.inked((300, 10, 8, 32)) && !drawn.inked((328, 10, 8, 32)));
		// Disabled: drawn, but no brighter than the dim color.
		let drawn = draw(&header(1, Vec::new(), vec![button(icon, false, false)]));
		let pixels = (300..336).flat_map(|x| (10..42).map(move |y| (x, y)));
		let shades: Vec<_> = pixels.map(|(x, y)| drawn.at(x, y) & 0xff).collect();
		assert!(
			shades.iter().any(|&shade| shade > 0x20),
			"{icon:?} not drawn"
		);
		assert!(
			shades.iter().all(|&shade| shade <= 0x80),
			"{icon:?} bright while disabled"
		);
	}
}

#[test]
fn every_size_doubles_at_twice_the_scale() {
	assert_eq!(pill(20, 300, (8, 88), 2), (20, 20, 300, 64));
	assert_eq!(close_rect(20, 300, (8, 88), 2), (20 + 300 - 56, 30, 44, 44));
	let mut raised = tab(20, 300, "");
	raised.fill = Some(COLORS.raised);
	let drawn = draw(&header(2, vec![raised], Vec::new()));
	assert_eq!(drawn.at(170, 4 + 12), COLORS.raised);
	assert_eq!(drawn.at(170, 4 + 11), COLORS.bar);
	assert_eq!(drawn.at(0, 4 + 86), COLORS.separator);
	assert_eq!(drawn.at(0, 4 + 85), COLORS.bar);
}
