//! Tests for `sbix` color glyphs, on fonts assembled table by table: strike
//! choice, placement from the origin offsets and the PNG's size, `dupe`
//! records, unsupported graphic types, and malformed tables.
use super::{
	Font,
	cbdt::ColorBitmap,
	cbdt_tests::{Image, Strike, font_tables, png, sfnt, tables},
};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;

/// One glyph record: origin offsets, graphic type and data.
#[derive(Clone)]
struct Graphic {
	x: i16,
	y: i16,
	kind: [u8; 4],
	data: Vec<u8>,
}

/// A strike: its size, and the record of each glyph from 0 (`None` for a
/// glyph without one).
struct SbixStrike {
	ppem: u16,
	glyphs: Vec<Option<Graphic>>,
}

fn graphic(x: i16, y: i16, (width, height): (u32, u32), color: u32) -> Option<Graphic> {
	let pixels = vec![color; (width * height) as usize];
	Some(Graphic {
		x,
		y,
		kind: *b"png ",
		data: png(width, height, &pixels),
	})
}

fn dupe(of: u16) -> Option<Graphic> {
	Some(Graphic {
		x: 0,
		y: 0,
		kind: *b"dupe",
		data: of.to_be_bytes().to_vec(),
	})
}

fn strike(ppem: u16, glyphs: Vec<Option<Graphic>>) -> SbixStrike {
	SbixStrike { ppem, glyphs }
}

/// An `sbix` table of `version` for `strikes`, each holding `glyphs`
/// records (padded with empty ones).
fn sbix_table(version: u16, glyphs: usize, strikes: &[SbixStrike]) -> Vec<u8> {
	let mut table = version.to_be_bytes().to_vec();
	table.extend(1u16.to_be_bytes());
	table.extend((strikes.len() as u32).to_be_bytes());
	let mut data = Vec::new();
	let start = 8 + 4 * strikes.len();
	for strike in strikes {
		table.extend(((start + data.len()) as u32).to_be_bytes());
		let mut body = strike.ppem.to_be_bytes().to_vec();
		body.extend(72u16.to_be_bytes());
		let mut records = Vec::new();
		let mut offsets = Vec::new();
		let header = 4 + 4 * (glyphs + 1);
		for glyph in 0..glyphs {
			offsets.push(header + records.len());
			if let Some(Some(g)) = strike.glyphs.get(glyph) {
				records.extend(g.x.to_be_bytes());
				records.extend(g.y.to_be_bytes());
				records.extend(g.kind);
				records.extend(&g.data);
			}
		}
		offsets.push(header + records.len());
		offsets
			.iter()
			.for_each(|&offset| body.extend((offset as u32).to_be_bytes()));
		body.extend(records);
		data.extend(body);
	}
	table.extend(data);
	table
}

/// A font mapping `chars` to glyphs from 1, 2048 units per em, whose glyphs
/// advance `advances` units (by glyph, from 0), with `sbix` as its only
/// glyph table.
fn sbix_font(chars: &[char], advances: &[u16], sbix: Vec<u8>) -> Vec<u8> {
	let mut tables = font_tables(chars, &[]);
	tables.retain(|(tag, _)| !matches!(*tag, b"CBDT" | b"CBLC"));
	let hhea = &mut tables
		.iter_mut()
		.find(|(tag, _)| *tag == b"hhea")
		.unwrap()
		.1;
	hhea[34..36].copy_from_slice(&(advances.len() as u16).to_be_bytes());
	let hmtx = advances
		.iter()
		.flat_map(|a| [a.to_be_bytes(), [0, 0]])
		.flatten();
	tables.push((b"hmtx", hmtx.collect()));
	tables.push((b"sbix", sbix));
	sfnt(&tables)
}

/// A font of 'a' and 'b' (glyphs 1 and 2), each advancing 1024 units, with
/// `strikes` of three glyph records.
fn font_of(strikes: &[SbixStrike]) -> Vec<u8> {
	sbix_font(&['a', 'b'], &[0, 1024, 1024], sbix_table(1, 3, strikes))
}

fn load(font: Vec<u8>) -> Font {
	Font::from_bytes(font, 0).unwrap()
}

/// `ch`'s bitmap as (ppem, width, height, left, top, advance) and its PNG.
type Found = ((u16, u16, u16, i16, i16, u16), Vec<u8>);

fn found(font: &Font, ch: char) -> Option<Found> {
	font.color_bitmap(ch).map(|b: ColorBitmap| {
		let fields = (b.ppem, b.width, b.height, b.left, b.top, b.advance);
		(fields, b.png.to_vec())
	})
}

#[test]
fn a_png_glyph_sits_at_its_origin_with_the_size_of_its_image() {
	let red = graphic(1, -2, (4, 3), RED);
	let png = red.as_ref().unwrap().data.clone();
	let font = load(font_of(&[strike(40, vec![None, red])]));
	// The bottom left corner is one pixel right of the pen and two below
	// the baseline, so the top is one above it; 1024 of 2048 units at 40
	// pixels per em advance 20.
	assert_eq!(found(&font, 'a'), Some(((40, 4, 3, 1, 1, 20), png)));
	assert_eq!(found(&font, 'b'), None, "no record");
	assert_eq!(found(&font, 'c'), None, "unmapped");
	assert!(font.has_color('a') && !font.has_color('b'));
}

#[test]
fn sbix_glyphs_are_scaled_from_their_strike_to_the_font_size() {
	let font = load(font_of(&[strike(
		40,
		vec![None, graphic(2, -2, (4, 4), RED)],
	)]));
	let (metrics, pixels) = font.rasterize_color('a', 20.0).unwrap();
	assert_eq!((metrics.width, metrics.height), (2, 2));
	// Top 2 strike pixels above the baseline is 1 at half size.
	assert_eq!((metrics.xmin, metrics.ymin), (1, -1));
	assert_eq!(metrics.advance_width, 10.0);
	assert_eq!(pixels, [RED; 4]);
}

#[test]
fn the_advance_comes_from_hmtx_scaled_to_the_strike() {
	let records = || {
		vec![
			None,
			graphic(0, 0, (2, 2), RED),
			graphic(0, 0, (2, 2), BLUE),
		]
	};
	let advance = |advances: &[u16]| {
		let sbix = sbix_table(1, 3, &[strike(64, records())]);
		let font = load(sbix_font(&['a', 'b'], advances, sbix));
		(
			found(&font, 'a').unwrap().0.5,
			found(&font, 'b').unwrap().0.5,
		)
	};
	assert_eq!(advance(&[0, 2048, 512]), (64, 16));
	// A monospace font lists one metric; later glyphs share it.
	assert_eq!(advance(&[0, 1536]), (48, 48));
	// Rounded to the nearest strike pixel: 100 units are 3.125 pixels,
	// 1500 are 46.875.
	assert_eq!(advance(&[0, 100, 1500]), (3, 47));
}

#[test]
fn the_largest_strike_with_the_glyph_wins() {
	let font = load(font_of(&[
		strike(
			20,
			vec![None, graphic(0, 0, (2, 2), RED), graphic(0, 0, (2, 2), RED)],
		),
		strike(80, vec![None, graphic(0, 0, (8, 8), RED)]),
		strike(
			40,
			vec![
				None,
				graphic(0, 0, (4, 4), RED),
				graphic(0, 0, (4, 4), BLUE),
			],
		),
	]));
	assert_eq!(found(&font, 'a').map(|b| b.0.0), Some(80));
	assert_eq!(found(&font, 'b').map(|b| b.0.0), Some(40), "80 lacks it");
}

#[test]
fn a_dupe_uses_the_record_of_the_glyph_it_names() {
	let red = graphic(3, -1, (2, 4), RED);
	let png = red.as_ref().unwrap().data.clone();
	let font = load(font_of(&[strike(32, vec![None, red, dupe(1)])]));
	// The origin and image are glyph 1's; the advance is still b's own.
	assert_eq!(found(&font, 'b'), Some(((32, 2, 4, 3, 3, 16), png)));
}

#[test]
fn dupes_are_followed_once_so_loops_cannot_hang() {
	let blue = graphic(0, 0, (2, 2), BLUE);
	// 'a' names itself; 'b' names 'a', itself a dupe.
	let font = load(font_of(&[strike(32, vec![blue, dupe(1), dupe(1)])]));
	assert_eq!(found(&font, 'a'), None);
	assert_eq!(found(&font, 'b'), None);
	// Glyph 0 is a png, so a dupe of it is found.
	let font = load(font_of(&[strike(
		32,
		vec![graphic(0, 0, (2, 2), BLUE), dupe(0)],
	)]));
	assert!(found(&font, 'a').is_some());
}

#[test]
fn a_dupe_of_a_glyph_past_the_strike_or_a_short_one_is_no_bitmap() {
	let font = load(font_of(&[strike(32, vec![None, dupe(3), dupe(u16::MAX)])]));
	assert_eq!(found(&font, 'a'), None, "glyph 3 of 3");
	assert_eq!(found(&font, 'b'), None);
	let short = Some(Graphic {
		x: 0,
		y: 0,
		kind: *b"dupe",
		data: vec![0],
	});
	let font = load(font_of(&[strike(
		32,
		vec![graphic(0, 0, (2, 2), RED), short],
	)]));
	assert_eq!(found(&font, 'a'), None, "one byte of the two");
}

#[test]
fn other_graphic_types_are_skipped_for_a_strike_that_has_png() {
	for kind in [b"jpg ", b"tiff", b"mask", b"emjc", b"PNG "] {
		// Skipped by its type alone: the data is a valid PNG.
		let other = Some(Graphic {
			kind: *kind,
			..graphic(0, 0, (2, 2), BLUE).unwrap()
		});
		let font = load(font_of(&[strike(64, vec![None, other.clone()])]));
		assert_eq!(found(&font, 'a'), None, "{kind:?}");
		// A smaller strike with a PNG is used instead.
		let font = load(font_of(&[
			strike(64, vec![None, other]),
			strike(16, vec![None, graphic(0, 0, (2, 2), RED)]),
		]));
		assert_eq!(found(&font, 'a').map(|b| b.0.0), Some(16), "{kind:?}");
	}
}

#[test]
fn a_png_without_a_readable_header_is_no_bitmap() {
	let record = |data: Vec<u8>| {
		Some(Graphic {
			x: 0,
			y: 0,
			kind: *b"png ",
			data,
		})
	};
	let whole = png(2, 2, &[RED; 4]);
	// The IHDR chunk's type, eight bytes after the signature's start.
	let mut no_ihdr = whole.clone();
	no_ihdr[12..16].copy_from_slice(b"IDAT");
	// An intact IHDR behind a broken signature.
	let mut unsigned = whole.clone();
	unsigned[1] = b'X';
	let font = load(font_of(&[strike(32, vec![None, record(whole)])]));
	assert!(found(&font, 'a').is_some());
	for data in [no_ihdr, unsigned, b"\x89PNG\r\n\x1a\n".to_vec(), Vec::new()] {
		let font = load(font_of(&[strike(32, vec![None, record(data)])]));
		assert_eq!(found(&font, 'a'), None);
	}
}

#[test]
fn sizes_and_origins_that_do_not_fit_are_no_bitmap() {
	let sized = |width: u32, height: u32| {
		let mut png = png(1, 1, &[RED]);
		png[16..20].copy_from_slice(&width.to_be_bytes());
		png[20..24].copy_from_slice(&height.to_be_bytes());
		Some(Graphic {
			x: 0,
			y: i16::MAX - 10,
			kind: *b"png ",
			data: png,
		})
	};
	let ppem = |record| found(&load(font_of(&[strike(32, vec![None, record])])), 'a');
	assert_eq!(ppem(sized(1, 10)).map(|b| b.0.4), Some(i16::MAX));
	assert_eq!(ppem(sized(1, 11)), None, "the top overflows");
	assert_eq!(ppem(sized(70_000, 1)), None, "too wide");
}

#[test]
fn only_version_1_tables_are_read() {
	let strikes = [strike(32, vec![None, graphic(0, 0, (2, 2), RED)])];
	for (version, drawn) in [(1, true), (0, false), (2, false)] {
		let sbix = sbix_table(version, 3, &strikes);
		let font = Font::from_bytes(sbix_font(&['a'], &[0, 1024], sbix), 0).unwrap();
		assert_eq!(font.has_color('a'), drawn, "{version}");
	}
}

#[test]
fn glyphs_past_the_glyph_count_have_no_bitmap() {
	// Strikes hold maxp.numGlyphs records; glyph 2 is past maxp's 2, though
	// the strike has a record for it.
	let past_the_count = |a: Option<Graphic>| {
		let records = vec![None, a, graphic(0, 0, (2, 2), BLUE)];
		let mut font = sbix_font(
			&['a', 'b'],
			&[0, 1024],
			sbix_table(1, 3, &[strike(32, records)]),
		);
		let maxp = table_start(&font, b"maxp");
		font[maxp + 4..maxp + 6].copy_from_slice(&2u16.to_be_bytes());
		load(font)
	};
	let font = past_the_count(graphic(0, 0, (2, 2), RED));
	assert!(found(&font, 'a').is_some());
	// The cmap gives 'b' glyph 0 now; by index, glyph 2 has no bitmap.
	assert_eq!(found(&font, 'b'), None);
	assert!(font.rasterize_color_glyph(1, 20.0).is_some());
	assert!(font.rasterize_color_glyph(2, 20.0).is_none());
	// Nor does a dupe of it.
	assert_eq!(found(&past_the_count(dupe(2)), 'a'), None);
}

/// Where the table `tag` starts in `font`, from the table directory.
fn table_start(font: &[u8], tag: &[u8; 4]) -> usize {
	let entry = font.windows(4).position(|w| w == tag).unwrap();
	u32::from_be_bytes(font[entry + 8..entry + 12].try_into().unwrap()) as usize
}

#[test]
fn truncated_tables_are_no_bitmap_not_a_panic() {
	let records = vec![None, graphic(1, 1, (2, 2), RED), dupe(1)];
	let sbix = sbix_table(1, 3, &[strike(32, records)]);
	let whole = font_of(&[strike(32, vec![None, graphic(1, 1, (2, 2), RED), dupe(1)])]);
	let entry = whole.windows(4).position(|w| w == b"sbix").unwrap() + 12;
	for cut in 0..sbix.len() {
		let mut font = whole.clone();
		font[entry..entry + 4].copy_from_slice(&(cut as u32).to_be_bytes());
		let font = load(font);
		let _ = (
			found(&font, 'a'),
			found(&font, 'b'),
			font.rasterize_color('b', 9.0),
		);
	}
	assert!(found(&load(whole), 'b').is_some());
}

#[test]
fn offsets_out_of_order_or_out_of_the_table_are_no_bitmap() {
	let records = || {
		vec![
			None,
			graphic(0, 0, (2, 2), RED),
			graphic(0, 0, (2, 2), BLUE),
		]
	};
	let patched = |offset_index: usize, value: u32| {
		let mut font = font_of(&[strike(32, records())]);
		// The strike follows the header and its one offset; its glyph
		// offsets follow ppem and ppi.
		let at = table_start(&font, b"sbix") + 12 + 4 + 4 * offset_index;
		font[at..at + 4].copy_from_slice(&value.to_be_bytes());
		load(font)
	};
	// Glyph 2's start before glyph 1's: 'a' runs backwards.
	let font = patched(2, 4);
	assert_eq!(found(&font, 'a'), None);
	// 'a' shorter than the eight bytes of origin and type, which start
	// after ppem, ppi and four offsets.
	for length in 0..8 {
		assert_eq!(found(&patched(2, 20 + length), 'a'), None, "{length}");
	}
	// The last end past the table.
	let font = patched(3, u32::MAX);
	assert!(found(&font, 'a').is_some());
	assert_eq!(found(&font, 'b'), None);
	// A strike offset past the table.
	let mut font = font_of(&[strike(32, records())]);
	let at = table_start(&font, b"sbix") + 8;
	font[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
	assert_eq!(found(&load(font), 'a'), None);
}

/// A font whose 'a' is only in the last of `strikes` strikes.
fn behind_empty_strikes(strikes: usize) -> Vec<u8> {
	let mut all: Vec<SbixStrike> = (0..strikes).map(|_| strike(200, Vec::new())).collect();
	all.push(strike(20, vec![None, graphic(0, 0, (2, 2), RED)]));
	sbix_font(&['a'], &[0, 1024], sbix_table(1, 2, &all))
}

#[test]
fn strikes_past_the_bound_are_not_read() {
	assert!(found(&load(behind_empty_strikes(0)), 'a').is_some());
	assert!(
		found(&load(behind_empty_strikes(63)), 'a').is_some(),
		"the 64th"
	);
	assert_eq!(
		found(&load(behind_empty_strikes(64)), 'a'),
		None,
		"the 65th"
	);
}

#[test]
fn faces_with_only_sbix_load() {
	let font = font_of(&[strike(32, vec![None, graphic(0, 0, (2, 2), RED)])]);
	assert!(Font::from_bytes(font, 0).is_ok());
	// An empty sbix table still makes the face drawable, as an empty CBDT
	// does; it draws nothing.
	let empty = sbix_font(&['a'], &[0, 1024], Vec::new());
	assert!(!load(empty).has_color('a'));
}

#[test]
fn a_face_with_both_tables_prefers_cbdt() {
	let cbdt_strike = Strike {
		ppem: 20,
		first: 1,
		index_format: 1,
		image_format: 17,
		images: vec![Some(Image {
			metrics: [2, 2, 0, 2, 2],
			png: png(2, 2, &[BLUE; 4]),
		})],
	};
	let (cblc, cbdt) = tables(&[cbdt_strike]);
	let sbix = sbix_table(1, 2, &[strike(64, vec![None, graphic(0, 0, (8, 8), RED)])]);
	let mut both = font_tables(&['a'], &[]);
	both.retain(|(tag, _)| !matches!(*tag, b"CBDT" | b"CBLC"));
	both.extend([(b"CBDT", cbdt), (b"CBLC", cblc), (b"sbix", sbix)]);
	let font = load(sfnt(&both));
	assert_eq!(found(&font, 'a').map(|b| b.0.0), Some(20));
	assert_eq!(font.rasterize_color('a', 20.0).unwrap().1, [BLUE; 4]);
}

/// An sbix copy of Noto Color Emoji written by another tool (fontTools:
/// the same PNGs, origins from the CBDT metrics) draws every emoji exactly
/// as the CBDT original does. Set `POOPTERM_SBIX_FONT` to its path.
#[test]
fn an_sbix_copy_of_noto_color_emoji_draws_what_its_cbdt_draws() {
	let (Ok(path), Ok(original)) = (
		std::env::var("POOPTERM_SBIX_FONT"),
		std::fs::read("/usr/share/fonts/noto/NotoColorEmoji.ttf"),
	) else {
		return;
	};
	let copy = Font::from_bytes(std::fs::read(path).unwrap(), 0).unwrap();
	let original = Font::from_bytes(original, 0).unwrap();
	let emoji = (0x1f300..0x1fb00).filter_map(char::from_u32);
	let mut compared = 0;
	for ch in emoji.filter(|&ch| original.has_color(ch)).step_by(5) {
		assert!(copy.has_color(ch), "{ch}");
		for px in [18.0, 37.5] {
			let (want, got) = (
				original.rasterize_color(ch, px),
				copy.rasterize_color(ch, px),
			);
			let (want, got) = (want.unwrap(), got.unwrap());
			assert_eq!(got.0.xmin, want.0.xmin, "{ch} at {px}");
			assert_eq!(got.0.ymin, want.0.ymin, "{ch} at {px}");
			assert_eq!(
				(got.0.width, got.0.height),
				(want.0.width, want.0.height),
				"{ch}"
			);
			assert!(
				(got.0.advance_width - want.0.advance_width).abs() < 0.5,
				"{ch}"
			);
			assert!(got.1 == want.1, "{ch} at {px}: pixels differ");
		}
		compared += 1;
	}
	assert!(compared > 200, "{compared}");
}

#[test]
fn installed_apple_color_emoji_draws_in_color_when_present() {
	let Ok(bytes) = std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc") else {
		return;
	};
	// Color only: it loads now that sbix counts as glyphs.
	let font = Font::from_bytes(bytes, 0).unwrap();
	assert!(!font.has_color('a'));
	// Recent releases store some strikes in a compressed type we skip;
	// when the PNG strikes have it, it draws at about the font size.
	if font.has_color('😀') {
		let (metrics, pixels) = font.rasterize_color('😀', 18.0).unwrap();
		assert!((14..=26).contains(&metrics.width), "{metrics:?}");
		let center = pixels[metrics.height / 2 * metrics.width + metrics.width / 2];
		assert_eq!(center >> 24, 0xff, "opaque in the middle");
	}
}
