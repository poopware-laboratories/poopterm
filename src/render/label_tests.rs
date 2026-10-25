//! Tests of drawing a tab's title and status fitted into a box.
use super::{
	glyphs::FontSet,
	label::{Label, draw_label},
	painter::Painter,
};
use crate::fonts::{BUNDLED, Font};

const PX: f32 = 15.0;
const ASCENT: i32 = 12;
const WIDTH: usize = 320;
const HEIGHT: usize = 30;
const TITLE: u32 = 0xff_ffffff;
const STATUS: u32 = 0xff_ff8000;

/// The pixels of `label` drawn from x = 10 in a box `room` wide over rows
/// 4..24.
fn drawn(title: &str, status: Option<&str>, room: usize) -> Vec<u32> {
	drawn_in(title, status, (room, 20))
}

/// The pixels of `label` drawn from x = 10 in a box `(width, height)` from
/// row 4.
fn drawn_in(title: &str, status: Option<&str>, (room, height): (usize, usize)) -> Vec<u32> {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![0; WIDTH * HEIGHT];
	let mut painter = Painter::new(&mut pixels, WIDTH, HEIGHT);
	let label = Label {
		title,
		status,
		colors: (TITLE, STATUS),
	};
	draw_label(
		&mut painter,
		&mut fonts,
		&label,
		4,
		(PX, ASCENT),
		(10, 4, room, height),
	);
	pixels
}

/// The columns holding any pixel tinted like `color`: grey for the white
/// title, more red than blue for the orange status, at any coverage.
fn columns(pixels: &[u32], color: u32) -> Vec<usize> {
	let tinted = |pixel: u32| {
		let (red, blue) = ((pixel >> 16) & 0xff, pixel & 0xff);
		match color {
			TITLE => red > 0 && red == blue,
			_ => red > blue + 8,
		}
	};
	(0..WIDTH)
		.filter(|&x| (0..HEIGHT).any(|y| tinted(pixels[y * WIDTH + x])))
		.collect()
}

/// `text` drawn alone from x = 10, without fitting.
fn plain(text: &str) -> Vec<u32> {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![0; WIDTH * HEIGHT];
	let mut painter = Painter::new(&mut pixels, WIDTH, HEIGHT);
	painter.text(&mut fonts, 10, 4, text, PX, ASCENT, TITLE);
	pixels
}

#[test]
fn a_title_that_fits_is_drawn_whole_in_the_title_color() {
	assert!(drawn("vim", None, 200) == plain("vim"));
	assert!(columns(&drawn("vim", None, 200), STATUS).is_empty());
}

#[test]
fn a_long_title_is_drawn_ellipsized() {
	let advance = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new()).advance('a', PX);
	// Room for four characters: three and the ellipsis.
	assert!(drawn("cargo build", None, 4 * advance) == plain("car…"));
}

#[test]
fn the_status_follows_the_title_after_a_gap_in_its_own_color() {
	let pixels = drawn("vim", Some("ok"), 200);
	let title = columns(&pixels, TITLE);
	let status = columns(&pixels, STATUS);
	assert!(!title.is_empty() && !status.is_empty());
	let advance = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new()).advance('a', PX);
	// "vim" and two spaces are five cells of the monospace bundled font.
	assert!(
		status[0] >= 10 + 5 * advance,
		"status starts at {}",
		status[0]
	);
	assert!(
		status[0] < 10 + 6 * advance,
		"status starts at {}",
		status[0]
	);
	assert!(title.last() < status.first());
}

#[test]
fn nothing_is_drawn_outside_the_box() {
	// Six rows cut through the glyphs; twenty hold them.
	for (room, height) in [
		(0, 20),
		(5, 20),
		(17, 20),
		(40, 20),
		(63, 6),
		(90, 6),
		(200, 6),
	] {
		let pixels = drawn_in("cargo build --release", Some("compiling"), (room, height));
		assert!(
			room < 17 || pixels.iter().any(|&pixel| pixel != 0),
			"{room}×{height} is blank"
		);
		for (index, &pixel) in pixels.iter().enumerate() {
			let (x, y) = (index % WIDTH, index / WIDTH);
			let inside = (10..10 + room).contains(&x) && (4..4 + height).contains(&y);
			assert!(inside || pixel == 0, "{room}×{height} inks ({x}, {y})");
		}
	}
}

#[test]
fn an_empty_box_draws_nothing() {
	assert!(drawn("vim", Some("ok"), 0).iter().all(|&pixel| pixel == 0));
}

#[test]
fn the_status_ink_starts_after_the_title_ink_at_every_room() {
	for room in (0..=240).step_by(3) {
		let pixels = drawn("cargo build", Some("compiling poopterm"), room);
		let (title, status) = (columns(&pixels, TITLE), columns(&pixels, STATUS));
		if let (Some(end), Some(start)) = (title.last(), status.first()) {
			assert!(
				end < start,
				"room {room}: title ends at {end}, status starts at {start}"
			);
		}
	}
}

#[test]
fn a_shortened_status_keeps_its_color_and_ellipsis() {
	let advance = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new()).advance('a', PX);
	// Twelve cells: half is six, so "car…", the gap, then "compi…".
	let pixels = drawn("cargo", Some("compiling"), 12 * advance);
	let status = columns(&pixels, STATUS);
	assert!(status.first() >= Some(&(10 + 6 * advance)));
	assert!(status.last() < Some(&(10 + 12 * advance)));
	// The ellipsis inks the status color in the last of the six cells.
	assert!(status.iter().any(|&x| x >= 10 + 11 * advance));
}

#[test]
fn a_box_too_small_for_the_status_shows_the_title() {
	let advance = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new()).advance('a', PX);
	let pixels = drawn("cargo", Some("compiling"), 3 * advance);
	assert!(columns(&pixels, STATUS).is_empty());
	assert!(pixels == plain("ca…"));
}
