//! Tests for finding installed fonts for missing characters: which faces
//! are candidates and in what order, and the bounds on what is loaded.
use super::{
	Font,
	coverage::pages,
	db::{Cache, FontFile, Stamp},
	fallback::*,
	sfnt::FaceInfo,
};
use std::{cell::Cell, fs, path::PathBuf, rc::Rc};

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

/// The bundled font with a `cmap` mapping only `ch`, to the glyph of 'A'.
pub(crate) fn font_with_only(ch: char) -> Vec<u8> {
	font_mapping(&[(ch, 'A')])
}

/// The bundled font with a `cmap` mapping only the first character of each
/// pair to the bundled glyph of the second.
pub(crate) fn font_mapping(pairs: &[(char, char)]) -> Vec<u8> {
	let bundled = Font::from_bytes(BUNDLED, 0).unwrap();
	let mut pairs = pairs.to_vec();
	pairs.sort_unstable();
	let mut cmap = Vec::new();
	for value in [0u16, 1, 3, 10, 0, 12, 12, 0] {
		cmap.extend(value.to_be_bytes());
	}
	let groups = pairs.len() as u32;
	for value in [16 + 12 * groups, 0, groups] {
		cmap.extend(value.to_be_bytes());
	}
	for (ch, like) in pairs {
		let glyph = u32::from(bundled.lookup_glyph_index(like));
		for value in [u32::from(ch), u32::from(ch), glyph] {
			cmap.extend(value.to_be_bytes());
		}
	}
	let be = |at: usize, len: usize| {
		BUNDLED[at..at + len]
			.iter()
			.fold(0, |n, &b| n << 8 | b as usize)
	};
	let count = be(4, 2);
	let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..count)
		.map(|i| {
			let at = 12 + 16 * i;
			let (offset, len) = (be(at + 8, 4), be(at + 12, 4));
			(
				BUNDLED[at..at + 4].try_into().unwrap(),
				BUNDLED[offset..offset + len].to_vec(),
			)
		})
		.collect();
	tables
		.iter_mut()
		.filter(|(tag, _)| tag == b"cmap")
		.for_each(|(_, table)| *table = cmap.clone());
	let mut out = BUNDLED[..12].to_vec();
	let mut offset = 12 + 16 * count;
	let mut data = Vec::new();
	for (tag, table) in &tables {
		out.extend(tag);
		out.extend([0; 4]);
		out.extend((offset as u32).to_be_bytes());
		out.extend((table.len() as u32).to_be_bytes());
		data.extend(table);
		data.resize(data.len().div_ceil(4) * 4, 0);
		offset = 12 + 16 * count + data.len();
	}
	out.extend(data);
	out
}

fn face(index: u32, ranges: &[(u32, u32)], weight: u16, monospace: bool) -> FaceInfo {
	FaceInfo {
		index,
		family: "Fallback".into(),
		style: String::new(),
		full_name: String::new(),
		postscript: String::new(),
		weight,
		italic: false,
		monospace,
		drawable: true,
		color: false,
		pages: pages(ranges.iter().copied()),
	}
}

fn file(path: &str, size: u64, faces: Vec<FaceInfo>) -> FontFile {
	FontFile {
		path: path.into(),
		stamp: Stamp { mtime: 0, size },
		faces,
	}
}

fn cache(files: Vec<FontFile>) -> Cache {
	Cache {
		files,
		..Cache::default()
	}
}

/// A directory for one test, holding `fonts` written to `0.ttf`, `1.ttf`, ….
pub(crate) fn installed(name: &str, fonts: &[Vec<u8>], ranges: &[(u32, u32)]) -> (PathBuf, Cache) {
	let dir = std::env::temp_dir().join(format!("poopterm-fallback-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	let files = fonts
		.iter()
		.enumerate()
		.map(|(i, bytes)| {
			let path = dir.join(format!("{i}.ttf"));
			fs::write(&path, bytes).unwrap();
			file(
				path.to_str().unwrap(),
				bytes.len() as u64,
				vec![face(0, ranges, 400, false)],
			)
		})
		.collect();
	(dir, cache(files))
}

pub(crate) const HAN: (u32, u32) = (0x4e00, 0x9fff);

#[test]
fn only_drawable_faces_covering_the_character_are_candidates() {
	let mut bitmap = face(0, &[HAN], 400, false);
	bitmap.drawable = false;
	let fonts = cache(vec![
		file("/latin.ttf", 10, vec![face(0, &[(0x20, 0x7e)], 400, false)]),
		file("/han.ttf", 10, vec![face(0, &[HAN], 400, false)]),
		file("/emoji.ttf", 10, vec![bitmap]),
	]);
	let paths = |ch| -> Vec<_> {
		candidates(&fonts, ch, false)
			.into_iter()
			.map(|(p, _)| p.to_owned())
			.collect()
	};
	assert_eq!(paths('漢'), [PathBuf::from("/han.ttf")]);
	assert_eq!(paths('a'), [PathBuf::from("/latin.ttf")]);
	assert!(paths('😀').is_empty());
}

#[test]
fn candidates_prefer_monospace_then_regular_upright_then_small_files() {
	let mut italic = face(0, &[HAN], 400, false);
	italic.italic = true;
	let fonts = cache(vec![
		file("/a-bold.ttf", 1, vec![face(0, &[HAN], 700, false)]),
		file("/b-big.ttf", 900, vec![face(0, &[HAN], 400, false)]),
		file("/c-italic.ttf", 1, vec![italic]),
		file("/d-small.ttf", 100, vec![face(0, &[HAN], 400, false)]),
		file("/e-mono.ttf", 999, vec![face(0, &[HAN], 300, true)]),
		file("/f-light.ttf", 1, vec![face(0, &[HAN], 350, false)]),
	]);
	let order: Vec<_> = candidates(&fonts, '中', false)
		.into_iter()
		.map(|(p, _)| p.to_str().unwrap())
		.collect();
	assert_eq!(
		order,
		[
			"/e-mono.ttf",
			"/d-small.ttf",
			"/b-big.ttf",
			"/f-light.ttf",
			"/a-bold.ttf",
			"/c-italic.ttf"
		]
	);
}

#[test]
fn collections_list_each_covering_face_with_its_index() {
	let fonts = cache(vec![file(
		"/cjk.ttc",
		10,
		vec![
			face(0, &[(0x20, 0x7e)], 400, false),
			face(3, &[HAN], 400, false),
			face(1, &[HAN], 400, false),
		],
	)]);
	let found: Vec<_> = candidates(&fonts, '字', false)
		.into_iter()
		.map(|(_, index)| index)
		.collect();
	assert_eq!(found, [1, 3]);
}

#[test]
fn files_too_large_to_load_are_not_candidates() {
	let fonts = cache(vec![
		file("/huge.ttf", 65 << 20, vec![face(0, &[HAN], 400, false)]),
		file("/limit.ttf", 64 << 20, vec![face(0, &[HAN], 400, false)]),
	]);
	assert_eq!(
		candidates(&fonts, '字', false),
		[(PathBuf::from("/limit.ttf").as_path(), 0)]
	);
}

#[test]
fn the_font_database_is_opened_only_when_a_character_is_missing() {
	let opened = Rc::new(Cell::new(0));
	let counter = opened.clone();
	let mut system = SystemFallback::new(move || {
		counter.set(counter.get() + 1);
		Cache::default()
	});
	assert_eq!(opened.get(), 0, "not at startup");
	let mut fonts = Vec::new();
	assert_eq!(system.find('漢', false, &mut fonts), None);
	assert_eq!(system.find('字', false, &mut fonts), None);
	assert_eq!(opened.get(), 1, "once");
}

#[test]
fn a_covering_font_is_loaded_and_returned() {
	let (dir, fonts) = installed("found", &[font_with_only('漢')], &[HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = vec![Font::from_bytes(BUNDLED, 0).unwrap()];
	assert_eq!(system.find('漢', false, &mut loaded), Some(1));
	assert_eq!(loaded.len(), 2);
	assert_ne!(loaded[1].lookup_glyph_index('漢'), 0);
	// Loaded once: asked again, it is found among the loaded fonts.
	assert_eq!(system.find('漢', false, &mut loaded), Some(1));
	assert_eq!(loaded.len(), 2);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fonts_whose_pages_promise_more_than_they_have_are_kept_for_later() {
	// Both claim the whole Han block; only the second has '字'.
	let (dir, fonts) = installed(
		"kept",
		&[font_with_only('漢'), font_with_only('字')],
		&[HAN],
	);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('字', false, &mut loaded), Some(1));
	assert_ne!(
		loaded[0].lookup_glyph_index('漢'),
		0,
		"the first stays loaded"
	);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_character_found_nowhere_is_not_searched_for_again() {
	let fonts: Vec<_> = (0..6).map(|_| font_with_only('漢')).collect();
	let (dir, fonts) = installed("missing", &fonts, &[HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('字', false, &mut loaded), None);
	assert_eq!(loaded.len(), 4, "at most four fonts per character");
	assert_eq!(system.find('字', false, &mut loaded), None);
	assert_eq!(loaded.len(), 4, "remembered as missing");
	// Another character goes on with the fonts not tried yet.
	assert_eq!(system.find('中', false, &mut loaded), None);
	assert_eq!(loaded.len(), 6);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unreadable_and_broken_files_are_skipped() {
	let (dir, mut fonts) = installed(
		"broken",
		&[b"not a font".to_vec(), font_with_only('漢')],
		&[HAN],
	);
	fonts.files.insert(
		0,
		file(
			"/nonexistent/font.ttf",
			1,
			vec![face(0, &[HAN], 400, false)],
		),
	);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('漢', false, &mut loaded), Some(0));
	assert_eq!(loaded.len(), 1, "only the real font");
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn no_more_than_32_fonts_are_ever_loaded() {
	let fonts: Vec<_> = (0..40).map(|_| font_with_only('A')).collect();
	let (dir, fonts) = installed("bounded", &fonts, &[HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	for ch in ('\u{4e00}'..).take(20) {
		system.find(ch, false, &mut loaded);
	}
	assert_eq!(loaded.len(), 32);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_character_is_searched_again_once_the_missing_list_is_reset() {
	// Five fonts claim the Han block; only the last has '字'.
	let mut fonts: Vec<_> = (0..4).map(|_| font_with_only('漢')).collect();
	fonts.push(font_with_only('字'));
	let (dir, fonts) = installed("reset", &fonts, &[HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(
		system.find('字', false, &mut loaded),
		None,
		"four tries, all wrong"
	);
	// Characters no font covers fill the list without loading anything.
	for ch in ('\u{ac00}'..).take(4095) {
		assert_eq!(system.find(ch, false, &mut loaded), None);
	}
	assert_eq!(
		system.find('字', false, &mut loaded),
		None,
		"still remembered"
	);
	system.find('\u{bc00}', false, &mut loaded);
	assert_eq!(
		system.find('字', false, &mut loaded),
		Some(4),
		"searched again"
	);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fonts_that_fail_to_load_count_toward_the_limit() {
	let fonts: Vec<_> = (0..40).map(|_| b"broken".to_vec()).collect();
	let (dir, mut fonts) = installed("failures", &fonts, &[HAN]);
	fonts.files.push(file(
		dir.join("real.ttf").to_str().unwrap(),
		64 << 20,
		vec![face(0, &[HAN], 400, false)],
	));
	fs::write(dir.join("real.ttf"), font_with_only('漢')).unwrap();
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	for ch in ('\u{4e00}'..).take(20) {
		system.find(ch, false, &mut loaded);
	}
	assert!(loaded.is_empty());
	assert_eq!(
		system.find('漢', false, &mut loaded),
		None,
		"no file is opened after 32"
	);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn faces_of_one_collection_are_loaded_by_their_index() {
	// A collection with the bundled font as face 0 and a Han font as face 1.
	let (bundled, han) = (BUNDLED.to_vec(), font_with_only('漢'));
	let header = 12 + 8;
	let mut ttc = b"ttcf".to_vec();
	ttc.extend(0x0001_0000u32.to_be_bytes());
	ttc.extend(2u32.to_be_bytes());
	ttc.extend((header as u32).to_be_bytes());
	ttc.extend(((header + bundled.len()) as u32).to_be_bytes());
	ttc.extend(rebase(&bundled, header));
	ttc.extend(rebase(&han, header + bundled.len()));
	let dir = std::env::temp_dir().join(format!("poopterm-fallback-{}-ttc", std::process::id()));
	fs::create_dir_all(&dir).unwrap();
	let path = dir.join("fonts.ttc");
	fs::write(&path, &ttc).unwrap();
	let faces = vec![
		face(0, &[(0x20, 0x7e)], 400, true),
		face(1, &[HAN], 400, false),
	];
	let fonts = cache(vec![file(path.to_str().unwrap(), ttc.len() as u64, faces)]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('漢', false, &mut loaded), Some(0));
	assert_eq!(loaded[0].lookup_glyph_index('a'), 0, "face 1, not face 0");
	fs::remove_dir_all(dir).unwrap();
}

/// `font` with its table offsets moved as if it started at `base`.
fn rebase(font: &[u8], base: usize) -> Vec<u8> {
	let mut out = font.to_vec();
	let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
	for i in 0..count {
		let at = 12 + 16 * i + 8;
		let offset = u32::from_be_bytes(font[at..at + 4].try_into().unwrap());
		out[at..at + 4].copy_from_slice(&(offset + base as u32).to_be_bytes());
	}
	out
}

const EMOJI: (u32, u32) = (0x1f300, 0x1faff);

/// A face with color bitmaps covering the emoji pages.
fn color_face() -> FaceInfo {
	FaceInfo {
		color: true,
		..face(0, &[EMOJI], 400, false)
	}
}

/// A color font with one red bitmap, for `ch`.
fn color_bytes(ch: char) -> Vec<u8> {
	use super::cbdt_tests::{Image, Strike, color_font, png};
	let strike = Strike {
		ppem: 16,
		first: 1,
		index_format: 1,
		image_format: 17,
		images: vec![Some(Image {
			metrics: [1, 1, 0, 1, 1],
			png: png(1, 1, &[0xffff0000]),
		})],
	};
	color_font(&[ch], &[strike])
}

#[test]
fn color_lookups_consider_only_color_faces() {
	let fonts = cache(vec![
		file("/symbols.ttf", 1, vec![face(0, &[EMOJI], 400, false)]),
		file("/emoji.ttf", 9, vec![color_face()]),
	]);
	let paths = |color| -> Vec<_> {
		let found = candidates(&fonts, '😀', color);
		found
			.into_iter()
			.map(|(path, _)| path.to_str().unwrap())
			.collect()
	};
	assert_eq!(paths(true), ["/emoji.ttf"]);
	assert_eq!(
		paths(false),
		["/symbols.ttf", "/emoji.ttf"],
		"both can draw it"
	);
}

#[test]
fn a_color_lookup_skips_fonts_with_only_an_outline() {
	let (dir, mut fonts) = installed(
		"color",
		&[font_with_only('😀'), color_bytes('😀')],
		&[EMOJI],
	);
	fonts.files[1].faces[0].color = true;
	fonts.files[1].stamp.size = 64 << 20;
	// Mark the outline font as color too, and so tried first: its glyph
	// still does not count.
	fonts.files[0].faces[0].color = true;
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('😀', true, &mut loaded), Some(1));
	assert!(loaded[1].has_color('😀') && !loaded[0].has_color('😀'));
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_in_color_and_missing_at_all_are_remembered_apart() {
	let (dir, fonts) = installed("apart", &[font_with_only('😀')], &[EMOJI]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('😀', true, &mut loaded), None, "no color font");
	assert!(loaded.is_empty(), "no color face was a candidate");
	assert_eq!(system.find('😀', false, &mut loaded), Some(0));
	assert_eq!(system.find('😀', true, &mut loaded), None);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn has_checks_the_glyph_or_its_color_bitmap() {
	let (outline, color) = (
		Font::from_bytes(font_with_only('😀'), 0).unwrap(),
		Font::from_bytes(color_bytes('😀'), 0).unwrap(),
	);
	assert!(has(&outline, '😀', false) && !has(&outline, '😀', true));
	assert!(has(&color, '😀', false) && has(&color, '😀', true));
	assert!(!has(&color, '😁', false) && !has(&color, '😁', true));
}

pub(crate) const KANA: (u32, u32) = (0x3000, 0x30ff);

#[test]
fn the_best_ranked_font_wins_over_one_loaded_earlier() {
	// 0.ttf, the smaller, ranks first for 漢; 1.ttf also has ア, which
	// loads it first.
	let (dir, mut fonts) = installed(
		"ranked",
		&[
			font_mapping(&[('漢', 'A')]),
			font_mapping(&[('ア', 'B'), ('漢', 'C')]),
		],
		&[HAN],
	);
	fonts.files[1].faces[0].pages = pages([KANA, HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(
		system.find('ア', false, &mut loaded),
		Some(0),
		"only 1.ttf covers kana"
	);
	assert_eq!(
		system.find('漢', false, &mut loaded),
		Some(1),
		"0.ttf, newly loaded"
	);
	assert_eq!(loaded[1].lookup_glyph_index('ア'), 0);
	assert_eq!(system.find('漢', false, &mut loaded), Some(1), "and again");
	assert_eq!(system.find('ア', false, &mut loaded), Some(0));
	assert_eq!(loaded.len(), 2);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ranked_lists_are_shared_by_the_characters_of_a_page() {
	// Every character of a page has the same candidates: a font that
	// lacks one is still asked about the next.
	let (dir, fonts) = installed(
		"pages",
		&[
			font_mapping(&[('丁', 'A')]),
			font_mapping(&[('七', 'B'), ('丁', 'C')]),
		],
		&[HAN],
	);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('七', false, &mut loaded), Some(1));
	assert_eq!(system.find('丁', false, &mut loaded), Some(0));
	assert_eq!(system.find('七', false, &mut loaded), Some(1));
	assert_eq!(loaded.len(), 2);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn loaded_fonts_still_serve_once_no_more_may_load() {
	// 32 fonts fill the limit; the 33rd, best ranked for 字, never loads,
	// and a loaded font that has 字 is found instead.
	let mut bytes: Vec<_> = (0..32)
		.map(|i| font_mapping(&[(char::from_u32(0x4e00 + i).unwrap(), 'A'), ('字', 'B')]))
		.collect();
	bytes.insert(0, font_with_only('字'));
	let (dir, mut fonts) = installed("full", &bytes, &[HAN]);
	fonts.files[0].stamp.size = 0;
	fonts.files[0].faces[0].pages = pages([(0x5b00, 0x5bff)]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	for ch in (0x4e00..0x4e20).map(|c| char::from_u32(c).unwrap()) {
		system.find(ch, false, &mut loaded);
	}
	assert_eq!(loaded.len(), 32);
	let found = system
		.find('字', false, &mut loaded)
		.expect("a loaded font");
	assert_eq!(loaded.len(), 32, "nothing more loads");
	assert_ne!(loaded[found].lookup_glyph_index('字'), 0);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ranked_lists_are_kept_for_at_most_512_pages() {
	let mut system = SystemFallback::new(Cache::default);
	let mut loaded = Vec::new();
	let pages = (0x100..).step_by(0x100).filter_map(char::from_u32);
	for (count, ch) in pages.take(512).enumerate() {
		system.find(ch, false, &mut loaded);
		assert_eq!(system.ranked_pages(), count + 1);
	}
	system.find('\u{10ff00}', false, &mut loaded);
	assert_eq!(system.ranked_pages(), 1, "reset, then the new page");
	system.find('\u{10ff00}', true, &mut loaded);
	assert_eq!(system.ranked_pages(), 2, "color has its own list");
}
