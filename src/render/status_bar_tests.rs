//! Pixel tests of the status bar: its fill and edge, and text at both ends
//! that never collides.
use super::{
	glyphs::FontSet,
	header::{HeaderColors, Rect, Style},
	status_bar::{StatusBar, draw_status_bar},
};
use crate::{
	fonts::{BUNDLED, Font},
	render::painter::Painter,
};

const WIDTH: usize = 400;
const HEIGHT: usize = 80;
const BACKGROUND: u32 = 0x000000;
const LEFT: u32 = 0xff0000;
const RIGHT: u32 = 0x00ff00;
const COLORS: HeaderColors = HeaderColors {
	bar: 0x202020,
	separator: 0x505050,
	hover: 0x303030,
	raised: 0x404040,
	fg: 0xffffff,
	dim: 0x808080,
};

fn bar(left: &str, right: &str) -> StatusBar {
	StatusBar {
		rect: (0, 40, WIDTH, 26),
		style: Style {
			scale: 1,
			px: 13.0,
			colors: COLORS,
		},
		left: (left.to_owned(), LEFT),
		right: (right.to_owned(), RIGHT),
	}
}

struct Drawn(Vec<u32>);

impl Drawn {
	fn at(&self, x: usize, y: usize) -> u32 {
		self.0[y * WIDTH + x]
	}

	/// The bounds of the pixels in `rect` tinted toward `color`'s channel:
	/// red for the left text, green for the right.
	fn ink(&self, (x, y, w, h): Rect, color: u32) -> Option<Rect> {
		let tinted = |pixel: u32| {
			let channel = |shift: u32| (pixel >> shift) & 0xff;
			if color == LEFT {
				channel(16) > channel(8) + 0x30
			} else {
				channel(8) > channel(16) + 0x30
			}
		};
		let points: Vec<_> = (y..y + h)
			.flat_map(|row| (x..x + w).map(move |column| (column, row)))
			.filter(|&(column, row)| tinted(self.at(column, row)))
			.collect();
		let left = points.iter().map(|point| point.0).min()?;
		let right = points.iter().map(|point| point.0).max()?;
		let top = points.iter().map(|point| point.1).min()?;
		let bottom = points.iter().map(|point| point.1).max()?;
		Some((left, top, right - left + 1, bottom - top + 1))
	}
}

fn draw(bar: &StatusBar) -> Drawn {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![BACKGROUND; WIDTH * HEIGHT];
	draw_status_bar(
		&mut Painter::new(&mut pixels, WIDTH, HEIGHT),
		&mut fonts,
		bar,
	);
	Drawn(pixels)
}

const ALL: Rect = (0, 0, WIDTH, HEIGHT);

#[test]
fn the_bar_fills_its_rect_under_a_separator() {
	let drawn = draw(&bar("", ""));
	for x in [0, 200, WIDTH - 1] {
		assert_eq!(drawn.at(x, 39), BACKGROUND, "above, column {x}");
		assert_eq!(drawn.at(x, 40), COLORS.separator, "top edge, column {x}");
		assert_eq!(drawn.at(x, 41), COLORS.bar, "column {x}");
		assert_eq!(drawn.at(x, 65), COLORS.bar, "bottom row, column {x}");
		assert_eq!(drawn.at(x, 66), BACKGROUND, "below, column {x}");
	}
}

#[test]
fn the_separator_thickens_with_the_scale() {
	let mut bar = bar("", "");
	bar.style.scale = 2;
	let drawn = draw(&bar);
	assert_eq!(
		(drawn.at(9, 40), drawn.at(9, 41)),
		(COLORS.separator, COLORS.separator)
	);
	assert_eq!(drawn.at(9, 42), COLORS.bar);
}

#[test]
fn the_left_text_starts_at_the_padding_and_the_right_ends_there() {
	let drawn = draw(&bar("home", "80 × 24"));
	let left = drawn.ink(ALL, LEFT).expect("the left text is drawn");
	let right = drawn.ink(ALL, RIGHT).expect("the right text is drawn");
	// Glyphs sit a pixel or two inside their advance.
	assert!((12..15).contains(&left.0), "{left:?}");
	assert!(
		(WIDTH - 15..=WIDTH - 12).contains(&(right.0 + right.2)),
		"{right:?}"
	);
}

#[test]
fn text_is_centered_down_the_bar() {
	let drawn = draw(&bar("Hxg", "Hxg"));
	for color in [LEFT, RIGHT] {
		let (_, top, _, height) = drawn.ink(ALL, color).unwrap();
		// Between the ascender and the descender, the line's middle.
		let middle = 2 * top + height;
		assert!(middle.abs_diff(2 * 40 + 26) <= 4, "{top} + {height}");
	}
}

#[test]
fn a_long_left_text_stops_short_of_the_right_one() {
	let long = "a very long working directory name ".repeat(4);
	let drawn = draw(&bar(&long, "80 × 24  ·  poop"));
	let left = drawn.ink(ALL, LEFT).unwrap();
	let right = drawn.ink(ALL, RIGHT).unwrap();
	let gap = right.0 - (left.0 + left.2);
	assert!(
		(20..40).contains(&gap),
		"the left text is cut a gap before the right: {gap}"
	);
}

#[test]
fn the_right_text_keeps_its_room_and_squeezes_the_left_out() {
	let right = "x".repeat(200);
	let drawn = draw(&bar("home", &right));
	assert!(
		drawn.ink(ALL, LEFT).is_none(),
		"no room is left for the left text"
	);
	let ink = drawn.ink(ALL, RIGHT).unwrap();
	assert!(ink.0 >= 12 && ink.0 + ink.2 <= WIDTH - 12, "{ink:?}");
}

#[test]
fn short_texts_are_drawn_whole() {
	let whole = draw(&bar("home", "poop"));
	let cut = draw(&bar("home", &"x".repeat(60)));
	let width = |drawn: &Drawn| drawn.ink(ALL, LEFT).map(|ink| ink.2);
	assert!(
		width(&whole) > width(&cut),
		"{:?} and {:?}",
		width(&whole),
		width(&cut)
	);
}

#[test]
fn nothing_is_drawn_outside_the_bar() {
	let long = "w".repeat(100);
	let drawn = draw(&bar(&long, &long));
	for y in (0..40).chain(66..HEIGHT) {
		for x in 0..WIDTH {
			assert_eq!(drawn.at(x, y), BACKGROUND, "({x}, {y})");
		}
	}
}

#[test]
fn a_bar_lower_in_the_window_moves_its_text_along() {
	let mut low = bar("home", "poop");
	low.rect = (0, 50, WIDTH, 26);
	let (high, low) = (draw(&bar("home", "poop")), draw(&low));
	let shift = |color| low.ink(ALL, color).unwrap().1 - high.ink(ALL, color).unwrap().1;
	assert_eq!((shift(LEFT), shift(RIGHT)), (10, 10));
}

#[test]
fn twice_the_scale_doubles_the_padding() {
	let mut bar = bar("home", "poop");
	bar.rect = (0, 14, WIDTH, 52);
	bar.style = Style {
		scale: 2,
		px: 26.0,
		..bar.style
	};
	let drawn = draw(&bar);
	let left = drawn.ink(ALL, LEFT).unwrap();
	let right = drawn.ink(ALL, RIGHT).unwrap();
	assert!((24..28).contains(&left.0), "{left:?}");
	assert!(
		(WIDTH - 28..=WIDTH - 24).contains(&(right.0 + right.2)),
		"{right:?}"
	);
	assert!(left.3 > 14, "the text doubles too: {} tall", left.3);
}

#[test]
fn text_stays_centered_in_a_taller_bar() {
	let mut bar = bar("Hxg", "Hxg");
	bar.rect = (0, 10, WIDTH, 60);
	let drawn = draw(&bar);
	for color in [LEFT, RIGHT] {
		let (_, top, _, height) = drawn.ink(ALL, color).unwrap();
		let middle = 2 * top + height;
		assert!(middle.abs_diff(2 * 10 + 60) <= 4, "{top} + {height}");
	}
}
