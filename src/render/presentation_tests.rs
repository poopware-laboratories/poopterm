//! Variation selectors on the grid: VS16 (U+FE0F) asks for a character's
//! emoji presentation and VS15 (U+FE0E) for its text presentation, whatever
//! the character's default; neither draws anything itself.
use crate::{
	fonts::{
		Font,
		cbdt_tests::{Image, Strike, color_font, png},
		fallback_tests::font_with_only,
	},
	parser::Parser,
	render::{
		glyphs::{FontSet, Presentation},
		grid::GridView,
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const BLUE: u32 = 0x0000ff;
const WIDTH: usize = 40;

/// A font whose ❤ and 😀 are blue 10-pixel squares at 16 pixels per em.
fn emoji_font() -> Font {
	let pixels = vec![0xff00_00ff; 100];
	let image = || {
		Some(Image {
			metrics: [10, 10, 0, 14, 12],
			png: png(10, 10, &pixels),
		})
	};
	let strike = Strike {
		ppem: 16,
		first: 1,
		index_format: 1,
		image_format: 17,
		images: vec![image(), image()],
	};
	Font::from_bytes(color_font(&['❤', '😀'], &[strike]), 0).unwrap()
}

/// A font drawing `ch` as the bundled font's 'A', without color.
fn text_font(ch: char) -> Font {
	Font::from_bytes(font_with_only(ch), 0).unwrap()
}

fn bundled() -> Font {
	Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap()
}

/// The fonts of these tests: text fonts for ❤ and 😀, and a color font for both.
fn fonts() -> FontSet {
	FontSet::new(
		bundled(),
		vec![text_font('❤'), emoji_font(), text_font('😀')],
	)
}

/// Draws `input` in four 10×20 cells, white on black, and returns the pixels.
fn render(input: &str, fonts: &mut FontSet) -> Vec<u32> {
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
	crate::render::grid::draw_grid(&mut painter, fonts, &view, None);
	pixels
}

fn painted(pixels: &[u32], color: u32) -> bool {
	pixels.contains(&color)
}

#[test]
fn the_fonts_here_cover_what_the_tests_assume() {
	// The bundled font draws ❤ as text, so VS16 must look past it; it has
	// no 😀, so fallbacks decide that one.
	assert_ne!(bundled().lookup_glyph_index('❤'), 0);
	assert!(!bundled().has_color('❤'));
	assert_eq!(bundled().lookup_glyph_index('😀'), 0);
}

#[test]
fn a_text_default_character_is_drawn_as_text() {
	let pixels = render("❤", &mut fonts());
	assert!(painted(&pixels, FG));
	assert!(!painted(&pixels, BLUE));
}

#[test]
fn vs16_draws_a_text_default_character_as_an_emoji() {
	let pixels = render("❤\u{fe0f}", &mut fonts());
	assert!(painted(&pixels, BLUE));
	assert!(!painted(&pixels, FG), "no text glyph and no mark drawn");
}

#[test]
fn vs15_draws_an_emoji_default_character_as_text() {
	assert!(
		painted(&render("😀", &mut fonts()), BLUE),
		"emoji by default"
	);
	let pixels = render("😀\u{fe0e}", &mut fonts());
	assert!(painted(&pixels, FG));
	assert!(!painted(&pixels, BLUE));
}

#[test]
fn a_presentation_no_font_offers_falls_back_to_the_other() {
	// Only the color font has 😀: VS15 cannot make it text.
	let mut only_color = FontSet::new(bundled(), vec![emoji_font()]);
	assert!(painted(&render("😀\u{fe0e}", &mut only_color), BLUE));
	// Only a text font has ❤: VS16 cannot make it an emoji.
	let mut only_text = FontSet::new(bundled(), vec![text_font('❤')]);
	let pixels = render("❤\u{fe0f}", &mut only_text);
	assert!(painted(&pixels, FG));
	assert!(!painted(&pixels, BLUE));
}

#[test]
fn the_selector_must_follow_the_character_directly() {
	// A combining mark in between: the selector no longer applies (UTS #51).
	let pixels = render("❤\u{301}\u{fe0f}", &mut fonts());
	assert!(!painted(&pixels, BLUE));
}

#[test]
fn presentation_comes_from_the_first_mark_only() {
	assert_eq!(Presentation::of(&[]), Presentation::Default);
	assert_eq!(Presentation::of(&['\u{fe0f}']), Presentation::Emoji);
	assert_eq!(Presentation::of(&['\u{fe0e}']), Presentation::Text);
	// Keycaps: 1 VS16 U+20E3.
	assert_eq!(
		Presentation::of(&['\u{fe0f}', '\u{20e3}']),
		Presentation::Emoji
	);
	assert_eq!(
		Presentation::of(&['\u{301}', '\u{fe0f}']),
		Presentation::Default
	);
	assert_eq!(
		Presentation::of(&['\u{200d}', '\u{fe0e}']),
		Presentation::Default
	);
	// Other variation selectors pick glyph variants, not presentation.
	assert_eq!(Presentation::of(&['\u{fe00}']), Presentation::Default);
}

#[test]
fn each_presentation_is_cached_apart() {
	let mut fonts = fonts();
	assert!(
		fonts
			.presented('😀', 16.0, Presentation::Emoji)
			.color
			.is_some()
	);
	assert!(
		fonts
			.presented('😀', 16.0, Presentation::Text)
			.color
			.is_none()
	);
	assert!(
		fonts
			.presented('😀', 16.0, Presentation::Default)
			.color
			.is_some()
	);
	assert_eq!(fonts.cached(), 3);
	assert!(
		fonts
			.presented('😀', 16.0, Presentation::Text)
			.color
			.is_none()
	);
	assert_eq!(fonts.cached(), 3);
}

#[test]
fn an_emoji_too_large_for_its_single_cell_is_shrunk_to_fit() {
	// ❤ is one cell wide; its emoji glyph is fitted, not spilled over a
	// neighbor that has text.
	let pixels = render("❤\u{fe0f}x", &mut fonts());
	let blue: Vec<usize> = (0..pixels.len())
		.filter(|&i| pixels[i] == BLUE)
		.map(|i| i % WIDTH)
		.collect();
	assert!(!blue.is_empty());
	assert!(blue.iter().all(|&x| x < 10), "{blue:?}");
}

#[test]
fn sized_text_honors_variation_selectors_too() {
	// OSC 66 text in a two-column block draws through its own path.
	let pixels = render("\x1b]66;w=2;❤\u{fe0f}\x07", &mut fonts());
	assert!(painted(&pixels, BLUE));
	assert!(!painted(&pixels, FG));
	let pixels = render("\x1b]66;w=2;😀\u{fe0e}\x07", &mut fonts());
	assert!(painted(&pixels, FG));
	assert!(!painted(&pixels, BLUE));
	let pixels = render("\x1b]66;w=2;😀\x07", &mut fonts());
	assert!(painted(&pixels, BLUE), "the default is kept");
}
