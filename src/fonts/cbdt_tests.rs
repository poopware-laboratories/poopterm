//! Tests for color bitmap glyphs, on fonts assembled table by table: the
//! `CBLC` index and `CBDT` images, loading color-only faces, and drawing
//! their glyphs at a size.
use super::{Font, cbdt::ColorBitmap};

/// One strike: its size, index and image formats, and the image of each
/// glyph from `first` (`None` for a glyph without a bitmap).
pub(crate) struct Strike {
	pub ppem: u8,
	pub first: u16,
	pub index_format: u16,
	pub image_format: u16,
	pub images: Vec<Option<Image>>,
}

/// A glyph image: small metrics (height, width, left, top, advance) and PNG.
#[derive(Clone)]
pub(crate) struct Image {
	pub metrics: [u8; 5],
	pub png: Vec<u8>,
}

/// A `0xAARRGGBB` image as PNG.
pub(crate) fn png(width: u32, height: u32, pixels: &[u32]) -> Vec<u8> {
	use image::ImageEncoder;
	let rgba: Vec<u8> = pixels
		.iter()
		.flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8])
		.collect();
	let mut out = Vec::new();
	image::codecs::png::PngEncoder::new(&mut out)
		.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
		.unwrap();
	out
}

/// `CBLC` and `CBDT` tables for `strikes`.
pub(crate) fn tables(strikes: &[Strike]) -> (Vec<u8>, Vec<u8>) {
	let mut cbdt = vec![0, 3, 0, 0];
	let mut cblc = vec![0, 3, 0, 0];
	cblc.extend((strikes.len() as u32).to_be_bytes());
	let mut arrays = Vec::new();
	let arrays_start = 8 + 48 * strikes.len();
	for strike in strikes {
		let last = strike.first + strike.images.len() as u16 - 1;
		let mut record = vec![0; 48];
		record[..4].copy_from_slice(&((arrays_start + arrays.len()) as u32).to_be_bytes());
		record[8..12].copy_from_slice(&1u32.to_be_bytes());
		record[40..42].copy_from_slice(&strike.first.to_be_bytes());
		record[42..44].copy_from_slice(&last.to_be_bytes());
		record[44] = strike.ppem;
		record[45] = strike.ppem;
		record[46] = 32;
		cblc.extend(record);
		// One subtable record, then the subtable eight bytes on.
		for value in [strike.first, last] {
			arrays.extend(value.to_be_bytes());
		}
		arrays.extend(8u32.to_be_bytes());
		arrays.extend(strike.index_format.to_be_bytes());
		arrays.extend(strike.image_format.to_be_bytes());
		arrays.extend((cbdt.len() as u32).to_be_bytes());
		let base = cbdt.len();
		let mut offsets = vec![0];
		for image in &strike.images {
			if let Some(Image { metrics, png }) = image {
				cbdt.extend(metrics);
				if strike.image_format == 18 {
					cbdt.extend([0, 0, 0]);
				}
				cbdt.extend((png.len() as u32).to_be_bytes());
				cbdt.extend(png);
			}
			offsets.push(cbdt.len() - base);
		}
		for offset in offsets {
			match strike.index_format {
				3 => arrays.extend((offset as u16).to_be_bytes()),
				_ => arrays.extend((offset as u32).to_be_bytes()),
			}
		}
	}
	cblc.extend(arrays);
	(cblc, cbdt)
}

/// A font with only color bitmaps, mapping each of `chars` to the glyphs
/// from 1, with 2048 units per em.
pub(crate) fn color_font(chars: &[char], strikes: &[Strike]) -> Vec<u8> {
	sfnt(&font_tables(chars, strikes))
}

pub(crate) fn font_tables(chars: &[char], strikes: &[Strike]) -> Vec<(&'static [u8; 4], Vec<u8>)> {
	let (cblc, cbdt) = tables(strikes);
	let mut head = vec![0; 54];
	head[18..20].copy_from_slice(&2048u16.to_be_bytes());
	let mut hhea = vec![0; 36];
	hhea[4..6].copy_from_slice(&1900i16.to_be_bytes());
	let mut maxp = vec![0, 0, 0x50, 0];
	maxp.extend((chars.len() as u16 + 1).to_be_bytes());
	let mut cmap = Vec::new();
	for value in [0u16, 1, 3, 10, 0, 12, 12, 0] {
		cmap.extend(value.to_be_bytes());
	}
	cmap.extend((16 + 12 * chars.len() as u32).to_be_bytes());
	cmap.extend([0; 4]);
	cmap.extend((chars.len() as u32).to_be_bytes());
	for (glyph, &ch) in chars.iter().enumerate() {
		for value in [u32::from(ch), u32::from(ch), glyph as u32 + 1] {
			cmap.extend(value.to_be_bytes());
		}
	}
	vec![
		(b"CBDT", cbdt),
		(b"CBLC", cblc),
		(b"cmap", cmap),
		(b"head", head),
		(b"hhea", hhea),
		(b"maxp", maxp),
	]
}

pub(crate) fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
	let mut out = 0x0001_0000u32.to_be_bytes().to_vec();
	out.extend((tables.len() as u16).to_be_bytes());
	out.extend([0; 6]);
	let mut offset = 12 + 16 * tables.len();
	let mut data = Vec::new();
	for (tag, table) in tables {
		out.extend(*tag);
		out.extend([0; 4]);
		out.extend((offset as u32).to_be_bytes());
		out.extend((table.len() as u32).to_be_bytes());
		data.extend(table);
		data.resize(data.len().div_ceil(4) * 4, 0);
		offset = 12 + 16 * tables.len() + data.len();
	}
	out.extend(data);
	out
}

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;

fn image(metrics: [u8; 5], color: u32) -> Option<Image> {
	let (height, width) = (u32::from(metrics[0]), u32::from(metrics[1]));
	let pixels = vec![color; (width * height) as usize];
	Some(Image {
		metrics,
		png: png(width, height, &pixels),
	})
}

fn strike(ppem: u8, first: u16, formats: (u16, u16), images: Vec<Option<Image>>) -> Strike {
	Strike {
		ppem,
		first,
		index_format: formats.0,
		image_format: formats.1,
		images,
	}
}

fn bitmap(font: &[u8], ch: char) -> Option<(u8, [u8; 5], usize)> {
	let font = Font::from_bytes(font.to_vec(), 0).ok()?;
	font.color_bitmap(ch).map(|b: ColorBitmap| {
		let metrics = [b.height, b.width, b.left as u8, b.top as u8, b.advance];
		(b.ppem, metrics, b.png.len())
	})
}

#[test]
fn index_format_1_with_small_metrics_finds_each_glyph() {
	let images = vec![
		image([2, 3, 1, 9, 4], RED),
		image([4, 4, 0xfe, 10, 5], BLUE),
	];
	let pngs: Vec<usize> = images
		.iter()
		.map(|i| i.as_ref().unwrap().png.len())
		.collect();
	let font = color_font(&['a', 'b'], &[strike(20, 1, (1, 17), images)]);
	assert_eq!(bitmap(&font, 'a'), Some((20, [2, 3, 1, 9, 4], pngs[0])));
	assert_eq!(bitmap(&font, 'b'), Some((20, [4, 4, 0xfe, 10, 5], pngs[1])));
	assert_eq!(bitmap(&font, 'c'), None, "unmapped");
}

#[test]
fn index_format_3_with_big_metrics_reads_the_horizontal_ones() {
	let red = image([5, 6, 2, 4, 7], RED);
	let length = red.as_ref().unwrap().png.len();
	let font = color_font(&['😀'], &[strike(109, 1, (3, 18), vec![red])]);
	assert_eq!(bitmap(&font, '😀'), Some((109, [5, 6, 2, 4, 7], length)));
	// The PNG after the vertical metrics decodes.
	let font = Font::from_bytes(font, 0).unwrap();
	assert_eq!(font.rasterize_color('😀', 109.0).unwrap().1, [RED; 30]);
}

#[test]
fn a_glyph_without_an_image_has_no_bitmap() {
	let images = vec![
		image([2, 2, 0, 2, 2], RED),
		None,
		image([2, 2, 0, 2, 2], BLUE),
	];
	let font = color_font(&['a', 'b', 'c'], &[strike(20, 1, (1, 17), images)]);
	assert!(bitmap(&font, 'a').is_some());
	assert_eq!(bitmap(&font, 'b'), None);
	assert!(bitmap(&font, 'c').is_some(), "the next one is still found");
}

#[test]
fn glyphs_outside_every_strike_have_no_bitmap() {
	// The strike starts at glyph 2, so 'a' (glyph 1) is not in it.
	let font = color_font(
		&['a', 'b'],
		&[strike(20, 2, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])],
	);
	assert_eq!(bitmap(&font, 'a'), None);
	assert!(bitmap(&font, 'b').is_some());
}

#[test]
fn the_largest_strike_with_the_glyph_wins() {
	let font = color_font(
		&['a', 'b'],
		&[
			strike(
				20,
				1,
				(1, 17),
				vec![image([2, 2, 0, 2, 2], RED), image([2, 2, 0, 2, 2], RED)],
			),
			strike(80, 1, (1, 17), vec![image([8, 8, 0, 8, 8], RED)]),
			strike(
				40,
				1,
				(3, 17),
				vec![image([4, 4, 0, 4, 4], RED), image([4, 4, 0, 4, 4], RED)],
			),
		],
	);
	assert_eq!(bitmap(&font, 'a').map(|b| b.0), Some(80));
	assert_eq!(bitmap(&font, 'b').map(|b| b.0), Some(40), "80 lacks it");
}

#[test]
fn unsupported_index_and_image_formats_have_no_bitmap() {
	for formats in [(2, 17), (4, 17), (5, 17), (1, 19), (1, 1), (1, 6)] {
		let font = color_font(
			&['a'],
			&[strike(20, 1, formats, vec![image([2, 2, 0, 2, 2], RED)])],
		);
		assert_eq!(bitmap(&font, 'a'), None, "{formats:?}");
	}
}

#[test]
fn truncated_tables_are_no_bitmap_not_a_panic() {
	let strikes = [strike(20, 1, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])];
	let (cblc, cbdt) = tables(&strikes);
	let whole = color_font(&['a'], &strikes);
	let at = |tag: &[u8; 4]| whole.windows(4).position(|w| w == tag).unwrap() + 12;
	let (cbdt_length, cblc_length) = (at(b"CBDT"), at(b"CBLC"));
	for (length_at, full) in [(cbdt_length, cbdt.len()), (cblc_length, cblc.len())] {
		for cut in 0..full {
			let mut font = whole.clone();
			font[length_at..length_at + 4].copy_from_slice(&(cut as u32).to_be_bytes());
			let _ = bitmap(&font, 'a');
		}
	}
	assert!(bitmap(&whole, 'a').is_some());
}

#[test]
fn a_length_beyond_the_image_data_is_no_bitmap() {
	let strikes = [strike(20, 1, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])];
	let mut font = color_font(&['a'], &strikes);
	let png = image([2, 2, 0, 2, 2], RED).unwrap().png;
	// The length field sits just before the PNG.
	let start = font.windows(8).position(|w| w == &png[..8]).unwrap();
	font[start - 4..start].copy_from_slice(&(png.len() as u32 + 1).to_be_bytes());
	assert_eq!(bitmap(&font, 'a'), None);
}

#[test]
fn faces_with_only_color_bitmaps_load_and_others_without_glyphs_do_not() {
	let strikes = [strike(20, 1, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])];
	assert!(Font::from_bytes(color_font(&['a'], &strikes), 0).is_ok());
	// The index alone, or the images alone, are not enough.
	for missing in [b"CBDT", b"CBLC"] {
		let mut tables = font_tables(&['a'], &strikes);
		tables.retain(|(tag, _)| *tag != missing);
		assert!(Font::from_bytes(sfnt(&tables), 0).is_err(), "{missing:?}");
	}
}

#[test]
fn color_glyphs_are_scaled_from_their_strike_to_the_font_size() {
	// A 4×4 strike at 20 pixels per em, drawn at 10: half size.
	let font = color_font(
		&['a'],
		&[strike(20, 1, (1, 17), vec![image([4, 4, 2, 8, 6], RED)])],
	);
	let font = Font::from_bytes(font, 0).unwrap();
	let (metrics, pixels) = font.rasterize_color('a', 10.0).unwrap();
	assert_eq!((metrics.width, metrics.height), (2, 2));
	assert_eq!((metrics.xmin, metrics.ymin), (1, 2), "top 4 less height 2");
	assert_eq!(metrics.advance_width, 3.0);
	assert_eq!(pixels, [RED; 4]);
	let (large, pixels) = font.rasterize_color('a', 40.0).unwrap();
	assert_eq!((large.width, large.height, large.ymin), (8, 8, 8));
	assert_eq!(pixels.len(), 64);
}

#[test]
fn has_color_tells_bitmap_glyphs_from_missing_ones() {
	let images = vec![image([2, 2, 0, 2, 2], RED), None];
	let font = Font::from_bytes(
		color_font(&['a', 'b'], &[strike(20, 1, (1, 17), images)]),
		0,
	)
	.unwrap();
	assert!(font.has_color('a'));
	assert!(!font.has_color('b'), "no image");
	assert!(!font.has_color('c'), "unmapped");
	assert_ne!(font.lookup_glyph_index('b'), 0);
	let bundled = Font::from_bytes(super::BUNDLED, 0).unwrap();
	assert!(!bundled.has_color('a'), "an outline font");
	assert_eq!(bundled.rasterize_color('a', 16.0), None);
}

#[test]
fn an_image_that_is_not_png_draws_nothing_in_color() {
	let broken = Image {
		metrics: [2, 2, 0, 2, 2],
		png: b"\x89PNG but not really".to_vec(),
	};
	let font = color_font(&['a'], &[strike(20, 1, (1, 17), vec![Some(broken)])]);
	let font = Font::from_bytes(font, 0).unwrap();
	assert!(font.has_color('a'));
	assert_eq!(font.rasterize_color('a', 20.0), None);
}

#[test]
fn installed_noto_color_emoji_draws_in_color_when_present() {
	let Ok(bytes) = std::fs::read("/usr/share/fonts/noto/NotoColorEmoji.ttf") else {
		return;
	};
	let font = Font::from_bytes(bytes, 0).unwrap();
	assert!(font.has_color('😀') && !font.has_color('a'));
	let (metrics, pixels) = font.rasterize_color('😀', 18.0).unwrap();
	assert!((18..=26).contains(&metrics.width), "{metrics:?}");
	let center = pixels[metrics.height / 2 * metrics.width + metrics.width / 2];
	assert_eq!(center >> 24, 0xff, "opaque in the middle");
}

/// Where the table `tag` starts in `font`, from the table directory.
fn table_start(font: &[u8], tag: &[u8; 4]) -> usize {
	let entry = font.windows(4).position(|w| w == tag).unwrap();
	u32::from_be_bytes(font[entry + 8..entry + 12].try_into().unwrap()) as usize
}

fn two_glyphs() -> Vec<u8> {
	let images = vec![image([2, 2, 0, 2, 2], RED), image([2, 2, 0, 2, 2], BLUE)];
	color_font(&['a', 'b'], &[strike(20, 1, (1, 17), images)])
}

#[test]
fn a_huge_strike_count_is_bounded() {
	let mut font = two_glyphs();
	let cblc = table_start(&font, b"CBLC");
	font[cblc + 4..cblc + 8].copy_from_slice(&u32::MAX.to_be_bytes());
	// The records past the real one read the index arrays as strikes; the
	// real one is still found and the scan stops.
	let found = bitmap(&font, 'a');
	assert_eq!(found.map(|b| b.0), Some(20));
	assert_eq!(bitmap(&font, 'b').map(|b| b.1), Some([2, 2, 0, 2, 2]));
}

#[test]
fn a_huge_subtable_count_still_finds_the_first() {
	let mut font = two_glyphs();
	let cblc = table_start(&font, b"CBLC");
	// The strike record's subtable count, eight bytes into the record.
	font[cblc + 16..cblc + 20].copy_from_slice(&u32::MAX.to_be_bytes());
	assert!(bitmap(&font, 'a').is_some());
}

/// A font whose 'a' sits in a strike after `strikes` others that lack it,
/// and in a subtable after `subtables` others that lack it.
fn behind_decoys(strikes: usize, subtables: usize) -> Vec<u8> {
	let real = [strike(20, 1, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])];
	let (original, _) = tables(&real);
	let mut decoy = [0u8; 48];
	decoy[40..44].copy_from_slice(&[0xff; 4]);
	let mut record = original[8..56].to_vec();
	let array = 8 + 48 * (strikes + 1);
	record[..4].copy_from_slice(&(array as u32).to_be_bytes());
	record[8..12].copy_from_slice(&(subtables as u32 + 1).to_be_bytes());
	let mut cblc = vec![0, 3, 0, 0];
	cblc.extend((strikes as u32 + 1).to_be_bytes());
	cblc.extend(decoy.repeat(strikes));
	cblc.extend(record);
	cblc.extend([0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0].repeat(subtables));
	cblc.extend([0, 1, 0, 1]);
	cblc.extend((8 * (subtables as u32 + 1)).to_be_bytes());
	// The subtable itself, after the one record of the original.
	cblc.extend(&original[64..]);
	let mut tables = font_tables(&['a'], &real);
	tables
		.iter_mut()
		.find(|(tag, _)| *tag == b"CBLC")
		.unwrap()
		.1 = cblc;
	sfnt(&tables)
}

#[test]
fn strikes_past_the_bound_are_not_read() {
	assert!(bitmap(&behind_decoys(0, 0), 'a').is_some());
	assert!(bitmap(&behind_decoys(63, 0), 'a').is_some(), "the 64th");
	assert_eq!(bitmap(&behind_decoys(64, 0), 'a'), None, "the 65th");
}

#[test]
fn subtables_past_the_bound_are_not_read() {
	assert!(bitmap(&behind_decoys(0, 4095), 'a').is_some(), "the 4096th");
	assert_eq!(bitmap(&behind_decoys(0, 4096), 'a'), None, "the 4097th");
}

#[test]
fn offsets_that_run_backwards_are_no_bitmap() {
	let mut font = two_glyphs();
	let cblc = table_start(&font, b"CBLC");
	// The subtable follows the strike record and its one subtable record;
	// its offsets array starts eight bytes in. Make the second offset
	// smaller than the first.
	let offsets = cblc + 8 + 48 + 8 + 8;
	font[offsets + 4..offsets + 8].copy_from_slice(&0u32.to_be_bytes());
	font[offsets..offsets + 4].copy_from_slice(&4u32.to_be_bytes());
	assert_eq!(bitmap(&font, 'a'), None);
	// 'b' now spans both images and reads the first.
	assert_eq!(bitmap(&font, 'b').map(|b| b.1), Some([2, 2, 0, 2, 2]));
}

#[test]
fn a_strike_of_zero_pixels_per_em_still_draws() {
	let font = color_font(
		&['a'],
		&[strike(0, 1, (1, 17), vec![image([2, 2, 0, 2, 2], RED)])],
	);
	let font = Font::from_bytes(font, 0).unwrap();
	// Read as one pixel per em rather than dividing by zero, then clamped.
	let (metrics, pixels) = font.rasterize_color('a', 3.0).unwrap();
	assert_eq!((metrics.width, metrics.height), (6, 6));
	assert_eq!(pixels, [RED; 36]);
}

#[test]
fn glyphs_left_of_the_pen_or_below_the_baseline_keep_their_place() {
	// Two pixels left of the pen, top one pixel below the baseline.
	let metrics = [4, 4, (-2i8) as u8, (-1i8) as u8, 4];
	let font = color_font(&['a'], &[strike(20, 1, (1, 17), vec![image(metrics, RED)])]);
	let font = Font::from_bytes(font, 0).unwrap();
	let (at_20, _) = font.rasterize_color('a', 20.0).unwrap();
	assert_eq!((at_20.xmin, at_20.ymin), (-2, -5));
	let (at_40, _) = font.rasterize_color('a', 40.0).unwrap();
	assert_eq!((at_40.xmin, at_40.ymin), (-4, -10));
}
