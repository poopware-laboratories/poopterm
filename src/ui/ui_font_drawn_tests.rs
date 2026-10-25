//! Tests that widths measured with a real font cover what text drawing inks.
use super::ui_font::{ellipsize_to_width, fit_label, text_width};
use crate::{
	fonts::{BUNDLED, Font},
	render::{glyphs::FontSet, painter::Painter},
};

const PX: f32 = 15.0;
const LEFT: usize = 8;

fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new())
}

/// The column after the rightmost pixel `text` inks when drawn at `LEFT`.
fn inked_right(fonts: &mut FontSet, text: &str) -> usize {
	let (width, height) = (400, 32);
	let mut pixels = vec![0; width * height];
	let mut painter = Painter::new(&mut pixels, width, height);
	let ascent = fonts.primary().horizontal_line_metrics(PX).unwrap().ascent as i32;
	painter.text(fonts, LEFT, 4, text, PX, ascent, 0xffffff);
	(0..width)
		.rev()
		.find(|&x| (0..height).any(|y| pixels[y * width + x] != 0))
		.map_or(LEFT, |x| x + 1)
}

#[test]
fn ellipsized_labels_ink_nothing_past_their_room() {
	let mut fonts = fonts();
	let text = "cargo build --release";
	for room in [0, 8, 9, 10, 40, 41, 100, 150] {
		let label = ellipsize_to_width(text, room, |ch| fonts.advance(ch, PX));
		let right = inked_right(&mut fonts, &label);
		assert!(
			right <= LEFT + room,
			"{label:?} inks to {right}, room ends at {}",
			LEFT + room
		);
	}
}

#[test]
fn the_measured_width_is_where_the_next_text_starts() {
	let mut fonts = fonts();
	let (label, status) = fit_label("vim main.rs", Some("modified"), 260, |ch| {
		fonts.advance(ch, PX)
	});
	assert_eq!(
		(label.as_str(), status.as_str()),
		("vim main.rs", "modified")
	);
	// Drawing the label and then the status after it inks the same as
	// drawing both as one string with the two-space gap.
	let offset = text_width(&label, |ch| fonts.advance(ch, PX)) + 2 * fonts.advance(' ', PX);
	let joined = inked_right(&mut fonts, &format!("{label}  {status}"));
	let status_alone = inked_right(&mut fonts, &status);
	assert_eq!(joined, status_alone + offset);
}
