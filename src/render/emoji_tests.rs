//! Color emoji on the grid: drawn in their own colors within their two
//! cells, fitted when too large, and upright in italic text.
use crate::{
	fonts::{
		Font,
		cbdt_tests::{Image, Strike, color_font, png},
	},
	parser::Parser,
	render::{glyphs::FontSet, grid::GridView, painter::Painter},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const BLUE: u32 = 0x0000ff;
const WIDTH: usize = 40;

/// A font whose 😀 is a blue `side`-pixel square at 16 pixels per em.
fn emoji_font(side: u8) -> Font {
	let pixels = vec![0xff00_00ff; usize::from(side) * usize::from(side)];
	let strike = Strike {
		ppem: 16,
		first: 1,
		index_format: 1,
		image_format: 17,
		images: vec![Some(Image {
			metrics: [side, side, 2, 14, side + 4],
			png: png(u32::from(side), u32::from(side), &pixels),
		})],
	};
	Font::from_bytes(color_font(&['😀'], &[strike]), 0).unwrap()
}

/// Draws `input` in four 10×20 cells, white on black, and returns the pixels.
fn render(input: &str, side: u8) -> Vec<u32> {
	let mut term = Term::new(4, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	let input = format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m{input}");
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.cursor = None;
	let mut pixels = vec![BG; WIDTH * 20];
	let mut painter = Painter::new(&mut pixels, WIDTH, 20);
	let view = GridView {
		frame: &frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 16.0,
		ascent: 14,
		scale: 1,
		default_bg: BG,
		link_color: 0,
		selection_colors: (0, 0),
		match_colors: (0, 0),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
	};
	let bundled = Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	let mut fonts = FontSet::new(bundled, vec![emoji_font(side)]);
	crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, None);
	pixels
}

/// The columns of pixels painted `color`, over every row.
fn columns(pixels: &[u32], color: u32) -> Vec<usize> {
	let mut found: Vec<usize> = (0..pixels.len())
		.filter(|&i| pixels[i] == color)
		.map(|i| i % WIDTH)
		.collect();
	found.sort_unstable();
	found.dedup();
	found
}

#[test]
fn an_emoji_draws_in_its_own_color_across_two_cells() {
	let pixels = render("😀", 16);
	assert_eq!(columns(&pixels, BLUE), (2..18).collect::<Vec<_>>());
	assert!(!pixels.contains(&FG), "not in the text color");
	let blue = pixels.iter().filter(|&&p| p == BLUE).count();
	assert_eq!(blue, 16 * 16);
}

#[test]
fn an_emoji_after_text_starts_in_its_own_cell() {
	let pixels = render("a😀", 16);
	assert_eq!(columns(&pixels, BLUE), (12..28).collect::<Vec<_>>());
	assert!(
		columns(&pixels, FG).iter().all(|&x| x < 10),
		"the a stays in cell 0"
	);
}

#[test]
fn an_emoji_too_large_for_its_cells_is_shrunk_to_fit() {
	// 40 pixels square, in two cells 20 pixels wide and tall.
	let pixels = render("😀", 40);
	let blue = columns(&pixels, BLUE);
	assert!(!blue.is_empty() && *blue.last().unwrap() < 20, "{blue:?}");
	let rows = (0..pixels.len())
		.filter(|&i| pixels[i] == BLUE)
		.map(|i| i / WIDTH);
	assert!(rows.max().unwrap() < 20);
	assert!(columns(&pixels, BLUE).len() >= 15, "not shrunk to a dot");
}

#[test]
fn italic_text_leaves_emoji_upright() {
	assert_eq!(render("\x1b[3m😀", 16), render("😀", 16));
	assert_ne!(
		render("\x1b[3ma", 16),
		render("a", 16),
		"but slants letters"
	);
}
