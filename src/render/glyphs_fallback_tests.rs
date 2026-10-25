//! Tests for which fallback font draws a character: configured fonts
//! first, then installed ones in their ranked order.
use super::{FontSet, tests::bundled};
use crate::fonts::Font;
use crate::fonts::fallback::SystemFallback;

/// A system fallback whose only installed font has `ch`, counting how
/// often the font database is opened.
fn system_with(ch: char, name: &str) -> (SystemFallback, std::rc::Rc<std::cell::Cell<u32>>) {
	use crate::fonts::{coverage::pages, db, fallback_tests::font_with_only};
	let dir = std::env::temp_dir().join(format!("poopterm-glyphs-{}-{name}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("only.ttf"), font_with_only(ch)).unwrap();
	let (mut cache, _) = db::refresh(&[dir], None);
	cache.files[0].faces[0].pages = pages([(0, 0x10ffff)]);
	let opened = std::rc::Rc::new(std::cell::Cell::new(0));
	let counter = opened.clone();
	let system = SystemFallback::new(move || {
		counter.set(counter.get() + 1);
		cache
	});
	(system, opened)
}

#[test]
fn characters_no_font_has_are_drawn_from_an_installed_font() {
	let (system, opened) = system_with('漢', "found");
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	let letter_a = bundled().rasterize('A', 16.0);
	let han = fonts.glyph('漢', 16.0);
	assert_eq!((han.metrics, &han.coverage), (letter_a.0, &letter_a.1));
	assert_eq!(opened.get(), 1);
}

#[test]
fn the_installed_fonts_are_not_opened_for_characters_the_fonts_have() {
	let (system, opened) = system_with('漢', "present");
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	for ch in ['a', '─', '\u{e0b0}'] {
		fonts.glyph(ch, 16.0);
	}
	assert_eq!(opened.get(), 0);
}

#[test]
fn a_character_no_font_anywhere_has_falls_back_to_the_primary_font() {
	let (system, _) = system_with('漢', "nowhere");
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	let tofu = bundled().rasterize('字', 16.0);
	let glyph = fonts.glyph('字', 16.0);
	assert_eq!((glyph.metrics, &glyph.coverage), (tofu.0, &tofu.1));
	// The installed font that was loaded still serves its own character.
	let letter_a = bundled().rasterize('A', 16.0);
	assert_eq!(fonts.glyph('漢', 16.0).coverage, letter_a.1);
}

#[test]
fn without_a_system_fallback_missing_characters_use_the_primary_font() {
	let mut fonts = FontSet::new(bundled(), Vec::new());
	let tofu = bundled().rasterize('漢', 16.0);
	assert_eq!(fonts.glyph('漢', 16.0).coverage, tofu.1);
}

#[test]
fn a_found_font_serves_every_size_and_survives_clearing_the_cache() {
	let (system, opened) = system_with('漢', "sizes");
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	let small = fonts.glyph('漢', 12.0).metrics;
	let large = fonts.glyph('漢', 24.0).metrics;
	assert!(large.height > small.height);
	fonts.clear();
	let again = fonts.glyph('漢', 12.0).metrics;
	assert_eq!(again, small);
	assert_eq!(opened.get(), 1, "the database is read once");
}

#[test]
fn the_primary_font_wins_over_installed_fonts_that_also_have_a_character() {
	// The installed font draws 'a' as an 'A'; the primary has its own 'a'.
	let (system, opened) = system_with('a', "primary");
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	assert_eq!(
		fonts.glyph('a', 16.0).coverage,
		bundled().rasterize('a', 16.0).1
	);
	assert_eq!(opened.get(), 0);
}

/// Installed fonts for 漢: 0.ttf, the smaller and so ranked first, draws
/// it as 'A'; 1.ttf draws it as 'C', and is the only one with ア.
fn ranked(name: &str) -> (SystemFallback, std::path::PathBuf) {
	use crate::fonts::{coverage::pages, fallback_tests::*};
	let fonts = [
		font_mapping(&[('漢', 'A')]),
		font_mapping(&[('ア', 'B'), ('漢', 'C')]),
	];
	let (dir, mut cache) = installed(&format!("glyphs-{name}"), &fonts, &[HAN]);
	cache.files[1].faces[0].pages = pages([KANA, HAN]);
	(SystemFallback::new(move || cache), dir)
}

#[test]
fn drawing_one_character_first_does_not_change_the_font_of_another() {
	let (system, dir) = ranked("alone");
	let mut alone = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	let (system, other) = ranked("after");
	let mut after = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	after.glyph('ア', 16.0);
	let letter_a = bundled().rasterize('A', 16.0).1;
	assert_eq!(alone.glyph('漢', 16.0).coverage, letter_a);
	assert_eq!(after.glyph('漢', 16.0).coverage, letter_a);
	assert_eq!(
		after.glyph('ア', 16.0).coverage,
		bundled().rasterize('B', 16.0).1
	);
	std::fs::remove_dir_all(dir).unwrap();
	std::fs::remove_dir_all(other).unwrap();
}

#[test]
fn configured_fallbacks_win_over_installed_fonts_loaded_before() {
	let (system, dir) = ranked("configured");
	let configured = crate::fonts::fallback_tests::font_mapping(&[('漢', 'D')]);
	let configured = Font::from_bytes(configured, 0).unwrap();
	let mut fonts = FontSet::new(bundled(), vec![configured]).with_system_fallback(system);
	fonts.glyph('ア', 16.0);
	assert_eq!(
		fonts.glyph('漢', 16.0).coverage,
		bundled().rasterize('D', 16.0).1
	);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn loaded_fonts_serve_characters_beyond_the_pages_they_claim() {
	// 1.ttf claims only Han here, yet also has ア.
	use crate::fonts::fallback_tests::*;
	let fonts = [font_mapping(&[('ア', 'B'), ('漢', 'C')])];
	let (dir, cache) = installed("glyphs-beyond", &fonts, &[HAN]);
	let system = SystemFallback::new(move || cache);
	let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
	fonts.glyph('漢', 16.0);
	assert_eq!(
		fonts.glyph('ア', 16.0).coverage,
		bundled().rasterize('B', 16.0).1
	);
	std::fs::remove_dir_all(dir).unwrap();
}
