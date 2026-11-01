//! Tests for choosing the symbol fonts loaded at startup: which faces
//! count, their rank, the bounds, and that directory order does not matter.
use super::{
	coverage::tests::ranges,
	db::{self, Cache, FontFile, Stamp},
	fallback::MAX_FILE,
	fallback_tests::font_mapping,
	sfnt::FaceInfo,
	symbols::*,
};
use std::{fs, path::PathBuf};

/// A face of `family` mapping `spans`, upright unless `italic`.
fn face(family: &str, spans: &[(u32, u32)], weight: u16, italic: bool) -> FaceInfo {
	FaceInfo {
		index: 0,
		family: family.into(),
		style: String::new(),
		full_name: String::new(),
		postscript: String::new(),
		weight,
		italic,
		monospace: false,
		drawable: true,
		color: false,
		coverage: ranges(spans.iter().copied()).into(),
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

fn chosen(cache: &Cache, limit: usize) -> Vec<(String, u32)> {
	symbol_fonts(cache, limit)
		.into_iter()
		.map(|(path, index)| (path.to_string_lossy().into_owned(), index))
		.collect()
}

const PUA: (u32, u32) = (0xe000, 0xf8ff);

#[test]
fn symbol_fonts_are_found_by_file_or_family_name_in_any_case() {
	let found = chosen(
		&cache(vec![
			file(
				"/a/DejaVuSans.ttf",
				10,
				vec![face("DejaVu Sans", &[(0x2190, 0x21ff)], 400, false)],
			),
			file(
				"/a/fa.otf",
				10,
				vec![face("Font AWESOME 6", &[(0xf000, 0xf2ff)], 400, false)],
			),
			file(
				"/a/NotoSansSymbols2.ttf",
				10,
				vec![face("Noto", &[(0x2600, 0x26ff)], 400, false)],
			),
			file(
				"/a/Hack-NERD.ttc",
				10,
				vec![face("Hack", &[(0xe0a0, 0xe0d4)], 400, false)],
			),
			file(
				"/a/Iosevka.ttf",
				10,
				vec![face("Iosevka", &[PUA], 400, false)],
			),
		]),
		8,
	);
	let mut paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
	paths.sort_unstable();
	assert_eq!(
		paths,
		["/a/Hack-NERD.ttc", "/a/NotoSansSymbols2.ttf", "/a/fa.otf"]
	);
}

/// The bug: the first eight paths in directory order were eight weights
/// and slants of one Nerd Font. One face per set of characters is enough,
/// and the upright regular one draws best.
#[test]
fn weights_and_slants_of_one_font_count_once_and_the_regular_wins() {
	let nerd = &[PUA, (0xf0001, 0xf1af0)];
	let mut files: Vec<FontFile> = [
		("Bold", 700, false),
		("BoldItalic", 700, true),
		("ExtraLight", 200, false),
		("Italic", 400, true),
		("Medium", 500, false),
		("Regular", 400, false),
	]
	.into_iter()
	.map(|(style, weight, italic)| {
		let path = format!("/f/JetBrainsMonoNerdFont-{style}.ttf");
		file(
			&path,
			2_000_000,
			vec![face("JetBrainsMono Nerd Font", nerd, weight, italic)],
		)
	})
	.collect();
	files.push(file(
		"/f/NotoSansSymbols-Regular.ttf",
		300_000,
		vec![face("Noto Sans Symbols", &[(0x2190, 0x21ff)], 400, false)],
	));
	assert_eq!(
		chosen(&cache(files), 8),
		[
			("/f/JetBrainsMonoNerdFont-Regular.ttf".into(), 0),
			("/f/NotoSansSymbols-Regular.ttf".into(), 0),
		]
	);
}

#[test]
fn a_face_adding_only_part_new_characters_is_still_chosen() {
	let found = chosen(
		&cache(vec![
			file(
				"/s/symbols-a.ttf",
				10,
				vec![face("A", &[(0x2600, 0x26ff)], 400, false)],
			),
			file(
				"/s/symbols-b.ttf",
				20,
				vec![face("B", &[(0x26f0, 0x2700)], 400, false)],
			),
			file(
				"/s/symbols-c.ttf",
				30,
				vec![face("C", &[(0x2650, 0x2660)], 400, false)],
			),
		]),
		8,
	);
	// b, ranked after a, adds U+2700 alone; c adds nothing over a.
	assert_eq!(
		found,
		[
			("/s/symbols-a.ttf".into(), 0),
			("/s/symbols-b.ttf".into(), 0)
		]
	);
}

#[test]
fn faces_without_characters_or_outlines_are_skipped() {
	let mut no_outlines = face("Symbols Nerd Font", &[PUA], 400, false);
	no_outlines.drawable = false;
	let found = chosen(
		&cache(vec![
			file(
				"/n/empty-symbol.ttf",
				10,
				vec![face("Empty", &[], 400, false)],
			),
			file("/n/bitmapless-nerd.ttf", 10, vec![no_outlines]),
			file("/n/notafont-symbol.ttf", 10, Vec::new()),
		]),
		8,
	);
	assert_eq!(found, []);
}

#[test]
fn the_face_index_of_a_collection_is_kept() {
	let mut second = face("Symbols", &[(0x2190, 0x21ff)], 400, false);
	second.index = 3;
	let found = chosen(
		&cache(vec![file(
			"/c/Symbols.ttc",
			10,
			vec![face("Symbols", &[(0x2190, 0x21ff)], 700, false), second],
		)]),
		8,
	);
	assert_eq!(found, [("/c/Symbols.ttc".into(), 3)]);
}

/// Equal in every other way, the font with more characters goes first,
/// then the smaller file, then the path, so no tie is left to input order.
#[test]
fn ties_break_on_coverage_then_size_then_path() {
	let font =
		|path: &str, size, last| file(path, size, vec![face("S", &[(0x2600, last)], 400, false)]);
	let wide = font("/t/symbol-wide.ttf", 900, 0x26ff);
	let small = font("/t/symbol-small.ttf", 100, 0x2600);
	let big = font("/t/symbol-big.ttf", 200, 0x2600);
	let found = chosen(&cache(vec![big.clone(), small.clone(), wide]), 8);
	assert_eq!(found, [("/t/symbol-wide.ttf".into(), 0)]);
	let found = chosen(&cache(vec![big, small]), 8);
	assert_eq!(found, [("/t/symbol-small.ttf".into(), 0)]);

	let alike = |path: &str| file(path, 100, vec![face("S", &[(0x2600, 0x2600)], 400, false)]);
	let found = chosen(
		&cache(vec![alike("/t/symbol-z.ttf"), alike("/t/symbol-a.ttf")]),
		8,
	);
	assert_eq!(found, [("/t/symbol-a.ttf".into(), 0)]);
}

/// Nine fonts, each with characters of its own: the limit keeps the eight
/// ranked first and drops the ninth, whatever order they come in.
#[test]
fn the_limit_keeps_the_best_ranked_fonts() {
	let fonts: Vec<FontFile> = (0..9u32)
		.map(|i| {
			let first = 0x2600 + 0x100 * i;
			// Lower `i` maps more characters, so ranks first.
			let last = first + 0x80 - i;
			file(
				&format!("/l/symbol-{i}.ttf"),
				10,
				vec![face("S", &[(first, last)], 400, false)],
			)
		})
		.collect();
	let expected: Vec<(String, u32)> = (0..8).map(|i| (format!("/l/symbol-{i}.ttf"), 0)).collect();
	assert_eq!(chosen(&cache(fonts.clone()), 8), expected);
	let mut reversed = fonts.clone();
	reversed.reverse();
	assert_eq!(chosen(&cache(reversed), 8), expected);
	assert_eq!(chosen(&cache(fonts[..8].to_vec()), 8).len(), 8);
	assert_eq!(chosen(&cache(fonts.clone()), 9).len(), 9);
	assert_eq!(chosen(&cache(fonts), 0), []);
}

#[test]
fn files_over_the_size_bound_are_skipped() {
	let font = |path: &str, size| file(path, size, vec![face("S", &[(0x2600, 0x26ff)], 400, false)]);
	let found = chosen(
		&cache(vec![
			font("/b/symbol-over.ttf", MAX_FILE + 1),
			font("/b/symbol-at.ttf", MAX_FILE),
		]),
		8,
	);
	assert_eq!(found, [("/b/symbol-at.ttf".into(), 0)]);
	assert_eq!(
		chosen(&cache(vec![font("/b/symbol-over.ttf", MAX_FILE + 1)]), 8),
		[]
	);
}

/// Every order of the database's files gives the same choice.
#[test]
fn every_file_order_gives_the_same_choice() {
	let files = vec![
		file("/o/a-nerd.ttf", 30, vec![face("A", &[PUA], 400, false)]),
		file(
			"/o/b-symbol.ttf",
			20,
			vec![face("B", &[(0x2190, 0x21ff)], 400, false)],
		),
		file(
			"/o/c-symbol.ttf",
			20,
			vec![face("C", &[(0x2190, 0x21ff)], 400, false)],
		),
		file(
			"/o/d-awesome.ttf",
			10,
			vec![face("D", &[(0x2700, 0x27bf)], 700, false)],
		),
		file(
			"/o/e-nerd.ttf",
			30,
			vec![face("E", &[PUA, (0x2190, 0x2191)], 400, true)],
		),
	];
	let first = chosen(&cache(files.clone()), 3);
	assert_eq!(first.len(), 3);
	let mut order: Vec<usize> = (0..files.len()).collect();
	// Heap's algorithm, iteratively: all 120 permutations.
	let mut counters = vec![0; order.len()];
	let mut i = 0;
	while i < order.len() {
		if counters[i] < i {
			order.swap(if i % 2 == 0 { 0 } else { counters[i] }, i);
			let shuffled = order.iter().map(|&at| files[at].clone()).collect();
			assert_eq!(chosen(&cache(shuffled), 3), first, "order {order:?}");
			counters[i] += 1;
			i = 0;
		} else {
			counters[i] = 0;
			i += 1;
		}
	}
}

/// Two directories holding the same fonts, created in opposite orders and
/// so listed in different orders by the file system, and a third where
/// over 2048 other files come first (the old scan's budget): the database
/// gives the same choice each time.
#[test]
fn directory_order_and_unrelated_files_do_not_change_the_choice() {
	let base = std::env::temp_dir().join(format!("poopterm-symbols-{}", std::process::id()));
	let _ = fs::remove_dir_all(&base);
	let fonts = [
		("Weather-Symbols.ttf", font_mapping(&[('\u{2600}', 'A')])),
		(
			"NerdIcons.ttf",
			font_mapping(&[('\u{e0a0}', 'B'), ('\u{e0b0}', 'C')]),
		),
		("NerdIcons-Bold.ttf", font_mapping(&[('\u{e0a0}', 'B')])),
	];
	let mut choices = Vec::new();
	for (name, reverse, filler) in [
		("forward", false, 0),
		("reverse", true, 0),
		("crowded", false, 2100),
	] {
		let root = base.join(name);
		fs::create_dir_all(root.join("aaa")).unwrap();
		for i in 0..filler {
			fs::write(root.join("aaa").join(format!("{i:04}.txt")), "").unwrap();
		}
		let mut order: Vec<_> = fonts.iter().collect();
		if reverse {
			order.reverse();
		}
		for (file, bytes) in order {
			fs::write(root.join("zzz-fonts").join(file), bytes)
				.or_else(|_| {
					fs::create_dir_all(root.join("zzz-fonts"))?;
					fs::write(root.join("zzz-fonts").join(file), bytes)
				})
				.unwrap();
		}
		let (db, _) = db::refresh(std::slice::from_ref(&root), None);
		let names: Vec<PathBuf> = symbol_fonts(&db, 8)
			.into_iter()
			.map(|(path, _)| path.strip_prefix(&root).unwrap().to_owned())
			.collect();
		choices.push(names);
	}
	let _ = fs::remove_dir_all(&base);
	assert_eq!(
		choices[0],
		[
			PathBuf::from("zzz-fonts/NerdIcons.ttf"),
			PathBuf::from("zzz-fonts/Weather-Symbols.ttf")
		]
	);
	assert_eq!(choices[1], choices[0]);
	assert_eq!(choices[2], choices[0]);
}

#[test]
fn counting_code_points_includes_both_ends() {
	assert_eq!(count(&[]), 0);
	assert_eq!(count(&[(0x41, 0x41)]), 1);
	assert_eq!(count(&[(0x41, 0x5a), (0xe000, 0xf8ff)]), 26 + 6400);
	assert_eq!(count(&[(0, 0x10_ffff)]), 0x11_0000);
}

#[test]
fn a_range_adds_characters_unless_one_covered_range_holds_it() {
	let covered = [(0x20, 0x7e), (0x2600, 0x26ff)];
	assert!(!adds(&covered, &[]));
	assert!(!adds(&covered, &[(0x20, 0x7e)]));
	assert!(!adds(&covered, &[(0x41, 0x41), (0x26ff, 0x26ff)]));
	// One past either end of a covered range.
	assert!(adds(&covered, &[(0x1f, 0x1f)]));
	assert!(adds(&covered, &[(0x7e, 0x7f)]));
	assert!(adds(&covered, &[(0x25ff, 0x2600)]));
	// Spanning the gap between two covered ranges.
	assert!(adds(&covered, &[(0x70, 0x2610)]));
	assert!(adds(&covered, &[(0x10_ffff, 0x10_ffff)]));
	assert!(adds(&[], &[(0, 0)]));
}

#[test]
fn unions_stay_sorted_and_merged() {
	assert_eq!(union(&[], &[]), []);
	assert_eq!(union(&[(0x41, 0x5a)], &[]), [(0x41, 0x5a)]);
	assert_eq!(union(&[(0x41, 0x5a)], &[(0x5b, 0x60)]), [(0x41, 0x60)]);
	assert_eq!(
		union(
			&[(0x41, 0x5a), (0x100, 0x1ff)],
			&[(0x30, 0x39), (0x150, 0x250)]
		),
		[(0x30, 0x39), (0x41, 0x5a), (0x100, 0x250)]
	);
	assert_eq!(union(&[(0x10, 0x10_ffff)], &[(0, 0x20)]), [(0, 0x10_ffff)]);
}

/// Without an upright regular face, the slant decides before the weight,
/// as `Cache::find` does: a bold upright face draws closer to the text.
#[test]
fn an_upright_face_beats_an_italic_one_of_regular_weight() {
	let nerd = &[PUA];
	let found = chosen(
		&cache(vec![
			file("/u/Nerd-Italic.ttf", 10, vec![face("N", nerd, 400, true)]),
			file("/u/Nerd-Bold.ttf", 10, vec![face("N", nerd, 700, false)]),
			file("/u/Nerd-Thin.ttf", 10, vec![face("N", nerd, 100, false)]),
		]),
		8,
	);
	// Bold and Thin are both 300 from regular and the same size: the path decides.
	assert_eq!(found, [("/u/Nerd-Bold.ttf".into(), 0)]);
}
