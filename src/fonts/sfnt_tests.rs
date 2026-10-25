//! Tests for the OpenType metadata reader, on the bundled font and on small
//! fonts assembled table by table.
use super::sfnt::*;
use std::io::Cursor;

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

fn faces(bytes: &[u8]) -> std::io::Result<Vec<FaceInfo>> {
	read_faces(&mut Cursor::new(bytes))
}

/// A `name` table from `(platform, encoding, language, name id, text)`.
fn name_table(records: &[(u16, u16, u16, u16, &str)]) -> Vec<u8> {
	let mut strings = Vec::new();
	let mut table = Vec::new();
	table.extend(0u16.to_be_bytes());
	table.extend((records.len() as u16).to_be_bytes());
	table.extend((6 + 12 * records.len() as u16).to_be_bytes());
	for &(platform, encoding, language, id, text) in records {
		let bytes: Vec<u8> = if platform == 1 {
			text.bytes().collect()
		} else {
			text.encode_utf16().flat_map(u16::to_be_bytes).collect()
		};
		for value in [
			platform,
			encoding,
			language,
			id,
			bytes.len() as u16,
			strings.len() as u16,
		] {
			table.extend(value.to_be_bytes());
		}
		strings.extend(bytes);
	}
	table.extend(strings);
	table
}

/// An `OS/2` table with a weight class, `fsSelection` bits and PANOSE
/// proportion.
fn os2(weight: u16, fs_selection: u16, proportion: u8) -> Vec<u8> {
	let mut table = vec![0; 78];
	table[4..6].copy_from_slice(&weight.to_be_bytes());
	table[32] = 2; // PANOSE family type: Latin text
	table[35] = proportion;
	table[62..64].copy_from_slice(&fs_selection.to_be_bytes());
	table
}

fn post(fixed_pitch: bool) -> Vec<u8> {
	let mut table = vec![0; 32];
	table[12..16].copy_from_slice(&u32::from(fixed_pitch).to_be_bytes());
	table
}

fn head(mac_style: u16) -> Vec<u8> {
	let mut table = vec![0; 54];
	table[44..46].copy_from_slice(&mac_style.to_be_bytes());
	table
}

/// A `cmap` with one subtable under `(platform, encoding)`.
fn cmap(platform: u16, encoding: u16, subtable: Vec<u8>) -> Vec<u8> {
	let mut table = Vec::new();
	for value in [0, 1, platform, encoding] {
		table.extend(u16::to_be_bytes(value));
	}
	table.extend(12u32.to_be_bytes());
	table.extend(subtable);
	table
}

/// A format 4 subtable mapping each inclusive range to glyphs from 1.
fn format4(ranges: &[(u16, u16)]) -> Vec<u8> {
	let mut segments = ranges.to_vec();
	segments.push((0xffff, 0xffff));
	let count = segments.len() as u16;
	let mut table = Vec::new();
	for value in [4, 0, 0, 2 * count, 0, 0, 0] {
		table.extend(u16::to_be_bytes(value));
	}
	segments
		.iter()
		.for_each(|&(_, end)| table.extend(end.to_be_bytes()));
	table.extend([0, 0]);
	segments
		.iter()
		.for_each(|&(start, _)| table.extend(start.to_be_bytes()));
	// idDelta maps each segment's first character to glyph 1.
	segments
		.iter()
		.for_each(|&(start, _)| table.extend(1u16.wrapping_sub(start).to_be_bytes()));
	segments.iter().for_each(|_| table.extend([0, 0]));
	let length = table.len() as u16;
	table[2..4].copy_from_slice(&length.to_be_bytes());
	table
}

/// A format 12 subtable mapping each inclusive range to glyphs from 1.
fn format12(groups: &[(u32, u32)]) -> Vec<u8> {
	let mut table = Vec::new();
	table.extend(12u16.to_be_bytes());
	table.extend([0, 0]);
	table.extend((16 + 12 * groups.len() as u32).to_be_bytes());
	table.extend(0u32.to_be_bytes());
	table.extend((groups.len() as u32).to_be_bytes());
	for &(start, end) in groups {
		for value in [start, end, 1] {
			table.extend(value.to_be_bytes());
		}
	}
	table
}

/// Printable ASCII, as text fonts map it.
fn latin() -> (&'static [u8; 4], Vec<u8>) {
	(b"cmap", cmap(3, 1, format4(&[(0x20, 0x7e)])))
}

/// An sfnt with `tables`, as if it started at `base` in its file.
fn sfnt(tables: &[(&[u8; 4], Vec<u8>)], base: usize) -> Vec<u8> {
	let mut out = Vec::new();
	out.extend(0x0001_0000u32.to_be_bytes());
	out.extend((tables.len() as u16).to_be_bytes());
	out.extend([0; 6]);
	let mut offset = base + 12 + 16 * tables.len();
	let mut data = Vec::new();
	for (tag, table) in tables {
		out.extend(*tag);
		out.extend(0u32.to_be_bytes());
		out.extend((offset as u32).to_be_bytes());
		out.extend((table.len() as u32).to_be_bytes());
		let padded = table.len().div_ceil(4) * 4;
		data.extend(table);
		data.resize(data.len() + padded - table.len(), 0);
		offset += padded;
	}
	out.extend(data);
	out
}

/// A TrueType collection of fonts built by `build(base)`.
fn collection(fonts: &[&dyn Fn(usize) -> Vec<u8>]) -> Vec<u8> {
	let header = 12 + 4 * fonts.len();
	let mut out = Vec::new();
	out.extend(*b"ttcf");
	out.extend(0x0001_0000u32.to_be_bytes());
	out.extend((fonts.len() as u32).to_be_bytes());
	let mut body = Vec::new();
	for build in fonts {
		let start = header + body.len();
		out.extend((start as u32).to_be_bytes());
		body.extend(build(start));
	}
	out.extend(body);
	out
}

fn basic(family: &str, style: &str, os2_table: Vec<u8>, base: usize) -> Vec<u8> {
	sfnt(
		&[
			(b"OS/2", os2_table),
			(
				b"name",
				name_table(&[(3, 1, 0x409, 1, family), (3, 1, 0x409, 2, style)]),
			),
			(b"post", post(false)),
		],
		base,
	)
}

#[test]
fn reads_the_bundled_font() {
	let faces = faces(BUNDLED).unwrap();
	assert_eq!(faces.len(), 1);
	let face = &faces[0];
	assert_eq!(face.index, 0);
	assert_eq!(face.family, "DejaVu Sans Mono for Powerline");
	assert_eq!(face.style, "Book");
	assert_eq!(face.full_name, "DejaVu Sans Mono for Powerline");
	assert_eq!(face.postscript, "DejaVuSansMonoPowerline");
	assert_eq!(face.weight, 400);
	assert!(!face.italic);
	assert!(face.monospace);
}

#[test]
fn typographic_names_group_weights_into_one_family() {
	let font = sfnt(
		&[
			(b"OS/2", os2(300, 0, 0)),
			(
				b"name",
				name_table(&[
					(3, 1, 0x409, 1, "Example Light"),
					(3, 1, 0x409, 2, "Regular"),
					(3, 1, 0x409, 16, "Example"),
					(3, 1, 0x409, 17, "Light"),
				]),
			),
		],
		0,
	);
	let face = &faces(&font).unwrap()[0];
	assert_eq!(
		(face.family.as_str(), face.style.as_str()),
		("Example", "Light")
	);
	assert_eq!(face.weight, 300);
}

#[test]
fn english_windows_names_win_over_other_languages_and_platforms() {
	let font = sfnt(
		&[(
			b"name",
			name_table(&[
				(1, 0, 0, 1, "Mac Name"),
				(3, 1, 0x407, 1, "Deutscher Name"),
				(3, 1, 0x409, 1, "English Name"),
			]),
		)],
		0,
	);
	assert_eq!(faces(&font).unwrap()[0].family, "English Name");
	let mac_only = sfnt(&[(b"name", name_table(&[(1, 0, 0, 1, "Mac Name")]))], 0);
	assert_eq!(faces(&mac_only).unwrap()[0].family, "Mac Name");
}

#[test]
fn style_comes_from_os2_or_else_from_head() {
	let bold_italic = basic("A", "Bold Italic", os2(700, 1, 0), 0);
	let face = &faces(&bold_italic).unwrap()[0];
	assert_eq!((face.weight, face.italic), (700, true));
	// fsSelection bit 9: oblique counts as italic.
	let oblique = basic("A", "Oblique", os2(400, 1 << 9, 0), 0);
	assert!(faces(&oblique).unwrap()[0].italic);
	// Without OS/2, head's macStyle bold and italic bits decide.
	let font = sfnt(
		&[
			(b"head", head(0b11)),
			(b"name", name_table(&[(3, 1, 0x409, 1, "Old")])),
		],
		0,
	);
	let face = &faces(&font).unwrap()[0];
	assert_eq!((face.weight, face.italic), (700, true));
}

#[test]
fn monospace_comes_from_post_not_panose() {
	let proportional = basic("P", "Regular", os2(400, 0, 3), 0);
	assert!(!faces(&proportional).unwrap()[0].monospace);
	// Patched proportional fonts (Iosevka Nerd Font Propo) keep the PANOSE
	// "monospaced" byte of the font they came from; fontconfig ignores it.
	let [hhea, hmtx] = metrics(&[500, 600, 700]);
	let stale = sfnt(
		&[
			(b"OS/2", os2(400, 0, 9)),
			(b"name", name_table(&[(3, 1, 0x409, 1, "Propo")])),
			hhea,
			hmtx,
		],
		0,
	);
	assert!(!faces(&stale).unwrap()[0].monospace);
	let fixed = sfnt(
		&[
			(b"name", name_table(&[(3, 1, 0x409, 1, "F")])),
			(b"post", post(true)),
			latin(),
		],
		0,
	);
	assert!(faces(&fixed).unwrap()[0].monospace);
}

#[test]
fn collections_list_every_face_with_its_index() {
	let ttc = collection(&[
		&|base| basic("Regular Family", "Regular", os2(400, 0, 9), base),
		&|base| basic("Regular Family", "Bold", os2(700, 0, 9), base),
	]);
	let faces = faces(&ttc).unwrap();
	assert_eq!(faces.len(), 2);
	assert_eq!((faces[0].index, faces[0].weight), (0, 400));
	assert_eq!((faces[1].index, faces[1].weight), (1, 700));
	assert_eq!(faces[1].style, "Bold");
}

#[test]
fn a_face_without_a_family_name_is_skipped() {
	let font = sfnt(&[(b"OS/2", os2(400, 0, 9))], 0);
	assert!(faces(&font).unwrap().is_empty());
}

#[test]
fn malformed_fonts_are_errors_not_panics() {
	assert!(faces(b"").is_err());
	assert!(faces(b"not a font at all").is_err());
	// Every truncation of a real font, and of a collection header claiming
	// far more faces than the file holds.
	for len in (0..BUNDLED.len()).step_by(97).chain(0..64) {
		let _ = faces(&BUNDLED[..len]);
	}
	let mut ttc = b"ttcf".to_vec();
	ttc.extend(0x0001_0000u32.to_be_bytes());
	ttc.extend(u32::MAX.to_be_bytes());
	assert!(faces(&ttc).is_err());
	// A name record pointing far past the table is ignored.
	let mut name = name_table(&[(3, 1, 0x409, 1, "Fine")]);
	name[16..18].copy_from_slice(&u16::MAX.to_be_bytes());
	let font = sfnt(&[(b"name", name)], 0);
	assert!(faces(&font).unwrap().is_empty());
}

/// `hhea` and `hmtx` tables for glyphs with these advance widths.
fn metrics(advances: &[u16]) -> [(&'static [u8; 4], Vec<u8>); 2] {
	let mut hhea = vec![0; 36];
	hhea[34..36].copy_from_slice(&(advances.len() as u16).to_be_bytes());
	let hmtx = advances
		.iter()
		.flat_map(|advance| [advance.to_be_bytes(), 7i16.to_be_bytes()].concat())
		.collect();
	[(b"hhea", hhea), (b"hmtx", hmtx)]
}

fn unflagged(advances: &[u16]) -> Vec<u8> {
	let [hhea, hmtx] = metrics(advances);
	sfnt(
		&[
			(b"name", name_table(&[(3, 1, 0x409, 1, "Unflagged")])),
			hhea,
			hmtx,
			latin(),
		],
		0,
	)
}

#[test]
fn equal_advance_widths_make_an_unflagged_font_monospace() {
	// Roboto Mono sets neither post.isFixedPitch nor PANOSE; fontconfig and
	// we measure the glyphs instead. Zero widths (combining marks) are
	// ignored, and long tables are checked past the first vector and chunk.
	let mut widths = vec![1229; 5000];
	widths[0] = 0;
	widths[4321] = 0;
	assert!(faces(&unflagged(&widths)).unwrap()[0].monospace);
	for at in [1, 15, 16, 17, 255, 4999] {
		let mut widths = widths.clone();
		widths[at] = 600;
		assert!(!faces(&unflagged(&widths)).unwrap()[0].monospace, "{at}");
	}
	assert!(!faces(&unflagged(&[0, 0, 0])).unwrap()[0].monospace);
	assert!(!faces(&unflagged(&[])).unwrap()[0].monospace);
}

/// A fixed-pitch font with this `cmap` table, if any.
fn fixed_with(cmap: Option<Vec<u8>>) -> Vec<u8> {
	let mut tables = vec![
		(b"name", name_table(&[(3, 1, 0x409, 1, "Glyphs")])),
		(b"post", post(true)),
	];
	tables.extend(cmap.map(|table| (b"cmap", table)));
	sfnt(&tables, 0)
}

fn monospace(font: &[u8]) -> bool {
	faces(font).unwrap()[0].monospace
}

#[test]
fn monospace_fonts_must_map_latin_letters_and_digits() {
	// Emoji and symbol fonts are fixed pitch too, but cannot be the
	// terminal font: Noto Color Emoji maps digits and '#' but no letters.
	let letters = [(0x30, 0x39), (0x41, 0x5a), (0x61, 0x7a)];
	assert!(monospace(&fixed_with(Some(cmap(3, 1, format4(&letters))))));
	assert!(monospace(&fixed_with(Some(cmap(
		0,
		3,
		format4(&[(0x20, 0x7e)])
	)))));
	let emoji = format4(&[(0x23, 0x23), (0x30, 0x39), (0x2600, 0x27bf)]);
	assert!(!monospace(&fixed_with(Some(cmap(3, 1, emoji)))));
	assert!(!monospace(&fixed_with(Some(cmap(
		3,
		1,
		format4(&[(0x41, 0x5a)])
	)))));
	assert!(!monospace(&fixed_with(None)));
	// Full-repertoire fonts use format 12.
	let wide = format12(&[(0x20, 0x7e), (0x1f600, 0x1f64f)]);
	assert!(monospace(&fixed_with(Some(cmap(3, 10, wide)))));
	let symbols = format12(&[(0xe000, 0xf8ff)]);
	assert!(!monospace(&fixed_with(Some(cmap(3, 10, symbols)))));
	// A character mapped to glyph 0 (.notdef) is not covered: shift the
	// digits' segment so that '0' lands on it.
	let mut notdef = format4(&letters);
	let deltas = 14 + 2 * 4 + 2 + 2 * 4; // header, endCode, pad, startCode
	notdef[deltas..deltas + 2].copy_from_slice(&0u16.wrapping_sub(0x30).to_be_bytes());
	assert!(!monospace(&fixed_with(Some(cmap(3, 1, notdef)))));
	// Malformed subtables are not coverage, and not panics.
	let mut cut = cmap(3, 1, format4(&[(0x20, 0x7e)]));
	cut.truncate(30);
	assert!(!monospace(&fixed_with(Some(cut))));
}

/// A font named "Glyphs" with `cmap` and, if given, an outline table.
fn covering(cmap: Option<Vec<u8>>, outline: Option<&'static [u8; 4]>) -> FaceInfo {
	let mut tables = vec![(b"name", name_table(&[(3, 1, 0x409, 1, "Glyphs")]))];
	tables.extend(cmap.map(|table| (b"cmap", table)));
	tables.extend(outline.map(|tag| (tag, vec![0; 4])));
	faces(&sfnt(&tables, 0)).unwrap().remove(0)
}

#[test]
fn faces_record_the_pages_their_cmap_maps() {
	let bmp = cmap(
		3,
		1,
		format4(&[(0x20, 0x7e), (0x2500, 0x257f), (0x4e00, 0x9fff)]),
	);
	assert_eq!(
		covering(Some(bmp), None).pages,
		[(0, 0), (0x25, 0x25), (0x4e, 0x9f)]
	);
	let full = cmap(3, 10, format12(&[(0x20, 0x7e), (0x1f600, 0x1f64f)]));
	assert_eq!(covering(Some(full), None).pages, [(0, 0), (0x1f6, 0x1f6)]);
	assert_eq!(covering(None, None).pages, []);
}

#[test]
fn the_format4_end_marker_is_not_coverage() {
	// Every format 4 table ends with a 0xFFFF segment; it maps nothing.
	let face = covering(Some(cmap(3, 1, format4(&[(0x41, 0x5a)]))), None);
	assert_eq!(face.pages, [(0, 0)]);
	// A real segment reaching 0xFFFF still counts.
	let face = covering(Some(cmap(3, 1, format4(&[(0xff00, 0xffef)]))), None);
	assert_eq!(face.pages, [(0xff, 0xff)]);
}

#[test]
fn the_full_repertoire_subtable_wins_over_the_bmp_one() {
	let mut table = Vec::new();
	for value in [0, 2, 3, 1, 0, 20, 3, 10] {
		table.extend(u16::to_be_bytes(value));
	}
	let bmp = format4(&[(0x20, 0x7e)]);
	table.extend((20 + bmp.len() as u32).to_be_bytes());
	table.extend(bmp);
	table.extend(format12(&[(0x20, 0x7e), (0x1f300, 0x1f3ff)]));
	assert_eq!(covering(Some(table), None).pages, [(0, 0), (0x1f3, 0x1f3)]);
}

#[test]
fn unreadable_cmaps_leave_the_face_without_pages() {
	let mut cut = cmap(3, 1, format4(&[(0x20, 0x7e), (0x400, 0x4ff)]));
	cut.truncate(24);
	let face = covering(Some(cut), None);
	assert_eq!((face.family.as_str(), face.pages.len()), ("Glyphs", 0));
	// A segment count larger than the table is not read past its end.
	let mut lying = cmap(3, 1, format4(&[(0x20, 0x7e)]));
	lying[12 + 6..12 + 8].copy_from_slice(&400u16.to_be_bytes());
	assert_eq!(covering(Some(lying), None).pages, []);
	// So is a format 12 group count.
	let mut groups = cmap(3, 10, format12(&[(0x20, 0x7e)]));
	groups[12 + 12..12 + 16].copy_from_slice(&u32::MAX.to_be_bytes());
	assert_eq!(covering(Some(groups), None).pages, [(0, 0)]);
	// Formats without ranges give none.
	let mut format6 = vec![0; 12];
	format6[1] = 6;
	assert_eq!(covering(Some(cmap(3, 1, format6)), None).pages, []);
}

#[test]
fn outline_tables_mark_fonts_that_can_be_drawn() {
	for tag in [b"glyf", b"CFF ", b"CFF2"] {
		assert!(covering(None, Some(tag)).outlines, "{tag:?}");
	}
	// Color bitmap fonts like Noto Color Emoji have only CBDT.
	assert!(!covering(None, Some(b"CBDT")).outlines);
	assert!(!covering(None, None).outlines);
}

#[test]
fn the_bundled_font_covers_latin_box_drawing_and_powerline() {
	let face = &faces(BUNDLED).unwrap()[0];
	assert!(face.outlines);
	let covers = |ch| super::coverage::covers(&face.pages, ch);
	assert!(['a', 'é', '─', '\u{e0b0}'].into_iter().all(covers));
	assert!(!covers('漢') && !covers('😀'));
}

#[test]
fn each_face_of_a_collection_has_its_own_pages() {
	let face = |ranges: &'static [(u16, u16)]| {
		move |base| {
			let tables = [
				(b"name", name_table(&[(3, 1, 0x409, 1, "Glyphs")])),
				(b"cmap", cmap(3, 1, format4(ranges))),
			];
			sfnt(&tables, base)
		}
	};
	let (latin, han) = (face(&[(0x20, 0x7e)]), face(&[(0x4e00, 0x9fff)]));
	let found = faces(&collection(&[&latin, &han])).unwrap();
	assert_eq!(found[0].pages, [(0, 0)]);
	assert_eq!(found[1].pages, [(0x4e, 0x9f)]);
}

#[test]
fn reversed_format4_segments_add_no_pages() {
	// A malformed segment whose start is after its end maps nothing.
	let face = covering(
		Some(cmap(3, 1, format4(&[(0x500, 0x400), (0x20, 0x7e)]))),
		None,
	);
	assert_eq!(face.pages, [(0, 0)]);
}

#[test]
fn monospace_detection_still_reads_latin_from_a_long_format12_table() {
	// Hundreds of groups, on every other page; ASCII stays in the first.
	let mut groups = vec![(0x20, 0x7e)];
	groups.extend((0..300).map(|i| (0x1_0000 + 0x200 * i, 0x1_0010 + 0x200 * i)));
	let face = faces(&fixed_with(Some(cmap(3, 10, format12(&groups)))))
		.unwrap()
		.remove(0);
	assert!(face.monospace);
	assert_eq!(face.pages.len(), 301);
	assert_eq!(face.pages.last(), Some(&(0x356, 0x356)));
}
