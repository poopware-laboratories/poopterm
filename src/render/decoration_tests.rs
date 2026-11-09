//! Line decorations: underlines in their own SGR 58 color, the rest in the text color.
use crate::{
	parser::Parser,
	render::{glyphs::FontSet, grid::GridView, painter::Painter},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const RED: u32 = 0xff0000;

/// Draws `input` in 10×20 cells, white on black, and returns the pixels.
fn render(input: &str) -> Vec<u32> {
	let mut term = Term::new(4, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(
		&mut term,
		format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m{input}").as_bytes(),
	);
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.cursor = None;
	let (width, height) = (40, 20);
	let mut pixels = vec![BG; width * height];
	let mut painter = Painter::new(&mut pixels, width, height);
	let view = GridView {
		frame: &frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 16.0,
		ascent: 14,
		scale: 1,
		default_bg: BG,
		link_color: 0x00ff00,
		selection_colors: (0, 0),
		match_colors: (0, 0),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
		dim: None,
	};
	let font = crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	let mut fonts = FontSet::new(font, Vec::new());
	crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, None);
	pixels
}

/// The colors along pixel row `y` of cell `col`.
fn line(pixels: &[u32], col: usize, y: usize) -> Vec<u32> {
	pixels[y * 40 + col * 10..][..10].to_vec()
}

#[test]
fn underlines_take_the_underline_color() {
	// Spaces, so no glyph ink crosses the lines.
	let pixels = render("\x1b[4;58:2::255:0:0m \x1b[4:2m \x1b[59m \x1b[24m ");
	assert_eq!(line(&pixels, 0, 18), [RED; 10], "single");
	assert_eq!(line(&pixels, 1, 18), [RED; 10], "double, lower line");
	assert_eq!(line(&pixels, 1, 16), [RED; 10], "double, upper line");
	assert_eq!(
		line(&pixels, 2, 18),
		[FG; 10],
		"59 goes back to the text color"
	);
	assert_eq!(line(&pixels, 3, 18), [BG; 10], "not underlined");
}

#[test]
fn overline_and_strike_keep_the_text_color() {
	let pixels = render("\x1b[53;9;58:2::255:0:0m \x1b[4m ");
	assert_eq!(line(&pixels, 0, 1), [FG; 10], "overline");
	assert_eq!(line(&pixels, 0, 10), [FG; 10], "strike");
	assert_eq!(
		line(&pixels, 0, 18),
		[BG; 10],
		"a color alone draws no underline"
	);
	assert_eq!(line(&pixels, 1, 18), [RED; 10]);
}
