//! Tests of monospace UI text: a font set's mono face, its own cache, and
//! the fallback for what the face lacks.
use super::{FontSet, MAX_CACHED_GLYPHS, tests::bundled};

/// A character the bundled DejaVu Sans Mono has no glyph for.
const MISSING: char = '中';

fn with_mono() -> FontSet {
	FontSet::new(bundled(), Vec::new()).with_mono(bundled())
}

#[test]
fn the_mono_face_draws_what_it_has_at_each_size() {
	let mut fonts = with_mono();
	for px in [12.0, 24.0] {
		let (metrics, coverage) = bundled().rasterize('a', px);
		let glyph = fonts.mono_glyph('a', px);
		assert_eq!(
			(glyph.metrics, &glyph.coverage),
			(metrics, &coverage),
			"{px}"
		);
	}
	assert_eq!(fonts.mono_cached(), 2);
	assert_eq!(
		fonts.cached(),
		0,
		"mono glyphs stay out of the proportional cache"
	);
}

#[test]
fn without_a_mono_face_mono_text_comes_from_the_usual_fonts() {
	let mut fonts = FontSet::new(bundled(), Vec::new());
	assert!(fonts.mono().is_none());
	let (metrics, coverage) = {
		let glyph = fonts.glyph('a', 12.0);
		(glyph.metrics, glyph.coverage.clone())
	};
	let mono = fonts.mono_glyph('a', 12.0);
	assert_eq!((mono.metrics, &mono.coverage), (metrics, &coverage));
	assert_eq!(fonts.mono_cached(), 0);
}

#[test]
fn what_the_mono_face_lacks_comes_from_the_usual_fonts() {
	let mut fonts = with_mono();
	let (metrics, coverage) = {
		let glyph = fonts.glyph(MISSING, 12.0);
		(glyph.metrics, glyph.coverage.clone())
	};
	let mono = fonts.mono_glyph(MISSING, 12.0);
	assert_eq!((mono.metrics, &mono.coverage), (metrics, &coverage));
	assert_eq!(fonts.mono_cached(), 0, "nothing of the mono face to keep");
}

#[test]
fn a_mono_advance_is_the_faces_advance_rounded_up() {
	let mut fonts = with_mono();
	let advance = bundled().metrics('M', 13.0).advance_width;
	assert_eq!(fonts.mono_advance('M', 13.0), advance.ceil() as usize);
	assert_eq!(
		fonts.mono_advance('i', 13.0),
		advance.ceil() as usize,
		"monospaced"
	);
}

#[test]
fn the_ui_fonts_carry_the_bundled_mono_face() {
	assert!(FontSet::ui(None).mono().is_some());
	assert!(FontSet::ui(Some(bundled())).mono().is_some());
}

#[test]
fn trimming_bounds_the_mono_cache_and_clearing_empties_it() {
	let mut fonts = with_mono();
	for ch in ('!'..='~').cycle().take(MAX_CACHED_GLYPHS + 1) {
		fonts.mono_glyph(ch, 12.0);
	}
	fonts.trim();
	assert_eq!(fonts.mono_cached(), 94, "few enough glyphs are kept");
	for size in 0..=MAX_CACHED_GLYPHS / 94 {
		for ch in '!'..='~' {
			fonts.mono_glyph(ch, 8.0 + size as f32);
		}
	}
	assert!(fonts.mono_cached() > MAX_CACHED_GLYPHS);
	fonts.trim();
	assert_eq!(fonts.mono_cached(), 0);
	fonts.mono_glyph('a', 12.0);
	fonts.clear();
	assert_eq!(fonts.mono_cached(), 0);
}
