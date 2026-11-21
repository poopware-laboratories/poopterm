//! Tests for `COLR` version 0 color glyphs, on fonts assembled table by
//! table from rectangle outlines: layer lookup and palette colors, the text
//! color, malformed tables, source-over painting, and whole glyphs drawn.
use super::{
	Font,
	cbdt_tests::{font_tables, sfnt},
	colr::{Colr, Layer, MAX_LAYERS, paint},
	face::{Face, Table},
};

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const WHITE: u32 = 0xffff_ffff;
const TEXT: u16 = 0xffff;

/// A simple `glyf` glyph: the rectangle from `(x0, y0)` to `(x1, y1)`.
fn rectangle((x0, y0, x1, y1): (i16, i16, i16, i16)) -> Vec<u8> {
	let mut glyph = Vec::new();
	for value in [1, x0, y0, x1, y1, 3, 0] {
		glyph.extend(value.to_be_bytes());
	}
	// Four on-curve points with 16-bit deltas.
	glyph.extend([1; 4]);
	for delta in [x0, x1 - x0, 0, x0 - x1] {
		glyph.extend(delta.to_be_bytes());
	}
	for delta in [y0, 0, y1 - y0, 0] {
		glyph.extend(delta.to_be_bytes());
	}
	glyph
}

/// `glyf` and long `loca` for `glyphs` from 0 (`None` for an empty one).
fn glyf(glyphs: &[Option<(i16, i16, i16, i16)>]) -> (Vec<u8>, Vec<u8>) {
	let (mut glyf, mut loca) = (Vec::new(), Vec::new());
	for glyph in glyphs {
		loca.extend((glyf.len() as u32).to_be_bytes());
		if let Some(rect) = glyph {
			glyf.extend(rectangle(*rect));
			glyf.resize(glyf.len().div_ceil(4) * 4, 0);
		}
	}
	loca.extend((glyf.len() as u32).to_be_bytes());
	(glyf, loca)
}

/// A `COLR` table of `version` with base records `(glyph, first layer,
/// layer count)` and layers `(glyph, palette index)`.
fn colr_table(version: u16, bases: &[(u16, u16, u16)], layers: &[(u16, u16)]) -> Vec<u8> {
	let header = if version == 0 { 14 } else { 34 };
	let layers_at = header + 6 * bases.len();
	let mut table = version.to_be_bytes().to_vec();
	table.extend((bases.len() as u16).to_be_bytes());
	table.extend((header as u32).to_be_bytes());
	table.extend((layers_at as u32).to_be_bytes());
	table.extend((layers.len() as u16).to_be_bytes());
	// Version 1 adds five offsets, all zero here.
	table.resize(header, 0);
	for &(glyph, first, count) in bases {
		[glyph, first, count]
			.iter()
			.for_each(|v| table.extend(v.to_be_bytes()));
	}
	for &(glyph, index) in layers {
		[glyph, index]
			.iter()
			.for_each(|v| table.extend(v.to_be_bytes()));
	}
	table
}

/// A `CPAL` table of `entries` colors per palette, whose palettes start at
/// `firsts` in `colors` (`0xAARRGGBB`).
fn cpal_table(entries: u16, firsts: &[u16], colors: &[u32]) -> Vec<u8> {
	let mut table = Vec::new();
	let records_at = 12 + 2 * firsts.len() as u32;
	for value in [0, entries, firsts.len() as u16, colors.len() as u16] {
		table.extend(value.to_be_bytes());
	}
	table.extend(records_at.to_be_bytes());
	firsts
		.iter()
		.for_each(|first| table.extend(first.to_be_bytes()));
	for color in colors {
		let [alpha, red, green, blue] = color.to_be_bytes();
		table.extend([blue, green, red, alpha]);
	}
	table
}

/// A font of 2048 units per em mapping `chars` to the glyphs from 1, with
/// `outlines` as its glyphs (from 0), each advancing 1024 units, and the
/// extra tables given.
fn font(
	chars: &[char],
	outlines: &[Option<(i16, i16, i16, i16)>],
	extra: Vec<(&'static [u8; 4], Vec<u8>)>,
) -> Vec<u8> {
	let mut tables = font_tables(chars, &[]);
	tables.retain(|(tag, _)| !matches!(*tag, b"CBDT" | b"CBLC"));
	for (tag, table) in &mut tables {
		match *tag {
			b"maxp" => table[4..6].copy_from_slice(&(outlines.len() as u16).to_be_bytes()),
			b"hhea" => table[34..36].copy_from_slice(&1u16.to_be_bytes()),
			b"head" => table[50..52].copy_from_slice(&1i16.to_be_bytes()),
			_ => {}
		}
	}
	let (glyf, loca) = glyf(outlines);
	tables.extend([
		(b"glyf", glyf),
		(b"loca", loca),
		(b"hmtx", vec![4, 0, 0, 0]),
	]);
	tables.extend(extra);
	tables.sort_by_key(|(tag, _)| **tag);
	sfnt(&tables)
}

const SQUARE: Option<(i16, i16, i16, i16)> = Some((0, 0, 1024, 1024));
const INNER: Option<(i16, i16, i16, i16)> = Some((256, 256, 768, 768));

/// 'a' (glyph 1) is a red square under a blue one inside it, 'b' (glyph 2)
/// a square in the text color, 'c' (glyph 3) has no layers; glyphs 4 and 5
/// are the layer outlines.
fn layered() -> Vec<u8> {
	let colr = colr_table(0, &[(1, 0, 2), (2, 2, 1)], &[(4, 0), (5, 1), (4, TEXT)]);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	let outlines = [None, SQUARE, SQUARE, SQUARE, SQUARE, INNER];
	font(
		&['a', 'b', 'c'],
		&outlines,
		vec![(b"COLR", colr), (b"CPAL", cpal)],
	)
}

fn face(font: Vec<u8>) -> Face {
	Face::parse(font.into(), 0).unwrap()
}

fn layer(glyph: u16, color: Option<u32>) -> Layer {
	Layer { glyph, color }
}

/// The layers of glyph 1 when `COLR` and `CPAL` are `colr` and `cpal`, in a
/// font of `glyphs` glyphs.
fn layers_of(colr: Vec<u8>, cpal: Option<Vec<u8>>, glyphs: u16) -> Option<Vec<Layer>> {
	let colr_len = colr.len();
	let mut data = colr;
	let cpal = cpal.map(|cpal| {
		let table = Table {
			offset: data.len(),
			len: cpal.len(),
		};
		data.extend(cpal);
		table
	});
	let colr = Table {
		offset: 0,
		len: colr_len,
	};
	Colr { colr, cpal }.layers(&data, 1, glyphs)
}

#[test]
fn base_glyphs_list_their_layers_bottom_first_with_palette_colors() {
	let face = face(layered());
	let reds_under_blue = vec![layer(4, Some(RED)), layer(5, Some(BLUE))];
	assert_eq!(face.color_layers(1), Some(reds_under_blue));
	assert_eq!(
		face.color_layers(2),
		Some(vec![layer(4, None)]),
		"text color"
	);
	assert_eq!(face.color_layers(3), None, "no base record");
	assert_eq!(face.color_layers(0), None);
	assert_eq!(face.color_layers(400), None, "past the glyphs");
}

#[test]
fn base_records_are_found_by_binary_search_among_many() {
	// Base glyphs 1, 3, 5 … 99, each with its own single layer.
	let bases: Vec<_> = (0..50).map(|i| (2 * i + 1, i, 1)).collect();
	let layers: Vec<_> = (0..50).map(|i| (i + 100, i % 2)).collect();
	let colr = colr_table(0, &bases, &layers);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	let table = |offset, len| Table { offset, len };
	let mut data = colr.clone();
	data.extend(&cpal);
	let found = Colr {
		colr: table(0, colr.len()),
		cpal: Some(table(colr.len(), cpal.len())),
	};
	for i in 0..50u16 {
		let color = [RED, BLUE][usize::from(i % 2)];
		let expected = Some(vec![layer(i + 100, Some(color))]);
		assert_eq!(
			found.layers(&data, 2 * i + 1, 200),
			expected,
			"base {}",
			2 * i + 1
		);
		assert_eq!(
			found.layers(&data, 2 * i, 200),
			None,
			"between bases at {}",
			2 * i
		);
	}
	assert_eq!(found.layers(&data, 101, 200), None, "after the last base");
}

#[test]
fn version_1_tables_still_color_glyphs_from_their_version_0_records() {
	let colr = colr_table(1, &[(1, 0, 1)], &[(2, 1)]);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	assert_eq!(
		layers_of(colr, Some(cpal), 3),
		Some(vec![layer(2, Some(BLUE))])
	);
	let unknown = colr_table(2, &[(1, 0, 1)], &[(2, 1)]);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	assert_eq!(layers_of(unknown, Some(cpal), 3), None, "a later version");
}

#[test]
fn colors_come_from_the_first_palette_wherever_it_starts() {
	// Palette 0 starts at record 2; palette 1 at record 0 is not used.
	let cpal = cpal_table(2, &[2, 0], &[0xff11_1111, 0xff22_2222, RED, 0x8000_ff00]);
	let colr = colr_table(0, &[(1, 0, 2)], &[(2, 0), (3, 1)]);
	let expected = vec![layer(2, Some(RED)), layer(3, Some(0x8000_ff00))];
	assert_eq!(layers_of(colr, Some(cpal), 4), Some(expected), "alpha kept");
}

#[test]
fn text_color_layers_need_no_palette() {
	let colr = colr_table(0, &[(1, 0, 1)], &[(2, TEXT)]);
	assert_eq!(layers_of(colr, None, 3), Some(vec![layer(2, None)]));
	let colored = colr_table(0, &[(1, 0, 1)], &[(2, 0)]);
	assert_eq!(
		layers_of(colored, None, 3),
		None,
		"a palette color without CPAL"
	);
}

#[test]
fn malformed_records_and_palettes_leave_the_glyph_uncolored() {
	let cpal = || Some(cpal_table(2, &[0], &[RED, BLUE]));
	let cases = [
		("no layers", colr_table(0, &[(1, 0, 0)], &[(2, 0)]), cpal()),
		(
			"layers past the list",
			colr_table(0, &[(1, 1, 2)], &[(2, 0), (2, 0)]),
			cpal(),
		),
		(
			"a layer glyph past the font",
			colr_table(0, &[(1, 0, 1)], &[(9, 0)]),
			cpal(),
		),
		(
			"an index past the palette",
			colr_table(0, &[(1, 0, 1)], &[(2, 2)]),
			cpal(),
		),
		(
			"an index past the color records",
			colr_table(0, &[(1, 0, 1)], &[(2, 1)]),
			Some(cpal_table(2, &[1], &[RED, BLUE])),
		),
		(
			"no palettes",
			colr_table(0, &[(1, 0, 1)], &[(2, 0)]),
			Some(cpal_table(2, &[], &[RED, BLUE])),
		),
	];
	for (why, colr, cpal) in cases {
		assert_eq!(layers_of(colr, cpal, 3), None, "{why}");
	}
}

#[test]
fn glyphs_with_more_layers_than_the_limit_are_not_drawn() {
	let at_limit = MAX_LAYERS as u16;
	let layers = vec![(2, 0); MAX_LAYERS + 1];
	let cpal = || Some(cpal_table(1, &[0], &[RED]));
	let colr = colr_table(0, &[(1, 0, at_limit)], &layers);
	assert_eq!(
		layers_of(colr, cpal(), 3).map(|l| l.len()),
		Some(MAX_LAYERS)
	);
	let colr = colr_table(0, &[(1, 0, at_limit + 1)], &layers);
	assert_eq!(layers_of(colr, cpal(), 3), None);
}

#[test]
fn truncated_tables_are_uncolored_not_a_panic() {
	let colr = colr_table(0, &[(1, 0, 2)], &[(2, 0), (2, 1)]);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	for len in 0..colr.len() {
		assert_eq!(
			layers_of(colr[..len].to_vec(), Some(cpal.clone()), 3),
			None,
			"COLR {len}"
		);
	}
	for len in 0..cpal.len() {
		let colr = colr.clone();
		assert_eq!(
			layers_of(colr, Some(cpal[..len].to_vec()), 3),
			None,
			"CPAL {len}"
		);
	}
}

#[test]
fn painting_full_coverage_of_an_opaque_color_replaces_the_pixel() {
	let mut pixels = [BLUE, 0, 0x8012_3456];
	paint(&mut pixels, &[255, 255, 255], RED);
	assert_eq!(pixels, [RED; 3]);
}

#[test]
fn painting_no_coverage_or_a_clear_color_changes_nothing() {
	let mut pixels = [BLUE, 0x8012_3456];
	paint(&mut pixels, &[0, 0], RED);
	paint(&mut pixels, &[255, 255], 0x00ff_ffff);
	assert_eq!(pixels, [BLUE, 0x8012_3456]);
}

#[test]
fn partial_coverage_over_nothing_keeps_the_color_with_less_alpha() {
	// Straight alpha: the color stays whole; only the alpha falls.
	let mut pixels = [0];
	paint(&mut pixels, &[128], RED);
	assert_eq!(pixels, [0x80ff_0000]);
}

#[test]
fn partial_coverage_over_an_opaque_pixel_mixes_the_colors() {
	let mut pixels = [BLUE];
	paint(&mut pixels, &[128], RED);
	assert_eq!(pixels, [0xff80_007f]);
}

#[test]
fn translucent_colors_over_translucent_pixels_add_their_alphas() {
	// 0.5 over 0.5 covers 0.75, two thirds of it the color on top.
	let mut pixels = [0x8000_00ff];
	paint(&mut pixels, &[255], 0x80ff_0000);
	assert_eq!(pixels, [0xc0aa_0055]);
}

/// The pixel at `(x, y)` from the top left of a drawn glyph.
fn pixel(glyph: &(super::Metrics, Vec<u32>), x: usize, y: usize) -> u32 {
	glyph.1[y * glyph.0.width + x]
}

#[test]
fn layers_are_drawn_bottom_first_in_one_box() {
	let font = Font::from_bytes(layered(), 0).unwrap();
	// 1024 of 2048 units at 32 pixels per em: a 16 pixel square, whose
	// inner layer covers pixels 4 to 11.
	let glyph = font.rasterize_color('a', 32.0).unwrap();
	let (m, _) = glyph;
	assert_eq!(
		(m.xmin, m.ymin, m.width, m.height, m.advance_width),
		(0, 0, 16, 16, 16.0)
	);
	assert_eq!(pixel(&glyph, 0, 0), RED, "outer corner");
	assert_eq!(pixel(&glyph, 3, 8), RED, "just outside the inner layer");
	assert_eq!(pixel(&glyph, 4, 8), BLUE, "inside it");
	assert_eq!(pixel(&glyph, 11, 11), BLUE);
	assert_eq!(pixel(&glyph, 12, 11), RED);
	assert_eq!(pixel(&glyph, 15, 15), RED);
}

#[test]
fn the_box_holds_layers_that_reach_past_each_other() {
	// A layer left and below the base square widens the box both ways.
	let colr = colr_table(0, &[(1, 0, 2)], &[(2, 0), (3, 1)]);
	let cpal = cpal_table(2, &[0], &[RED, BLUE]);
	let outlines = [None, SQUARE, SQUARE, Some((-512, -512, 0, 0))];
	let font = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let glyph = Font::from_bytes(font, 0)
		.unwrap()
		.rasterize_color('a', 32.0)
		.unwrap();
	let m = glyph.0;
	assert_eq!((m.xmin, m.ymin, m.width, m.height), (-8, -8, 24, 24));
	assert_eq!(pixel(&glyph, 0, 23), BLUE, "the corner layer, bottom left");
	assert_eq!(pixel(&glyph, 23, 0), RED, "the square, top right");
	assert_eq!(pixel(&glyph, 0, 0), 0, "covered by neither");
}

#[test]
fn text_color_layers_are_drawn_light() {
	let font = Font::from_bytes(layered(), 0).unwrap();
	let glyph = font.rasterize_color('b', 32.0).unwrap();
	assert_eq!(pixel(&glyph, 8, 8), WHITE);
}

#[test]
fn layered_glyphs_count_as_color_and_others_do_not() {
	let font = Font::from_bytes(layered(), 0).unwrap();
	assert!(font.has_color('a') && font.has_color('b'));
	assert!(!font.has_color('c'), "an outline without layers");
	assert!(font.rasterize_color('c', 32.0).is_none());
	assert!(!font.has_color('z'), "unmapped");
}

#[test]
fn layers_without_outlines_draw_an_empty_glyph_that_still_advances() {
	let colr = colr_table(0, &[(1, 0, 1)], &[(2, 0)]);
	let cpal = cpal_table(1, &[0], &[RED]);
	let outlines = [None, SQUARE, None];
	let font = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let (metrics, pixels) = Font::from_bytes(font, 0)
		.unwrap()
		.rasterize_color('a', 32.0)
		.unwrap();
	assert_eq!(
		(metrics.width, metrics.height, metrics.advance_width),
		(0, 0, 16.0)
	);
	assert!(pixels.is_empty());
}

#[test]
fn glyphs_without_a_character_are_drawn_by_index_as_sequences_are() {
	// Glyph 3 is a ZWJ sequence's ligature: in COLR but not in cmap.
	let colr = colr_table(0, &[(3, 0, 1)], &[(2, 0)]);
	let cpal = cpal_table(1, &[0], &[BLUE]);
	let outlines = [None, SQUARE, INNER, SQUARE];
	let font = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let font = Font::from_bytes(font, 0).unwrap();
	let glyph = font.rasterize_color_glyph(3, 32.0).unwrap();
	let m = glyph.0;
	// Placed where its one layer is, advancing as the base glyph does.
	assert_eq!(
		(m.xmin, m.ymin, m.width, m.height, m.advance_width),
		(4, 4, 8, 8, 16.0)
	);
	assert!(glyph.1.iter().all(|&p| p == BLUE));
	assert!(
		font.rasterize_color_glyph(1, 32.0).is_none(),
		"'a' has no layers"
	);
}

#[test]
fn layers_scale_with_the_font_size() {
	let font = Font::from_bytes(layered(), 0).unwrap();
	let glyph = font.rasterize_color('a', 64.0).unwrap();
	assert_eq!(
		(glyph.0.width, glyph.0.height, glyph.0.advance_width),
		(32, 32, 32.0)
	);
	assert_eq!(pixel(&glyph, 7, 16), RED);
	assert_eq!(pixel(&glyph, 8, 16), BLUE);
	assert_eq!(pixel(&glyph, 23, 23), BLUE);
	assert_eq!(pixel(&glyph, 24, 23), RED);
}

#[test]
fn translucent_palette_colors_let_lower_layers_show_through() {
	let colr = colr_table(0, &[(1, 0, 3)], &[(2, 0), (3, 1), (3, TEXT)]);
	let half_blue = 0x8000_00ff;
	let cpal = cpal_table(2, &[0], &[RED, half_blue]);
	let outlines = [None, SQUARE, SQUARE, INNER];
	let topped = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let glyph = Font::from_bytes(topped, 0)
		.unwrap()
		.rasterize_color('a', 32.0)
		.unwrap();
	assert_eq!(pixel(&glyph, 0, 0), RED, "only the bottom layer");
	// Half blue over red in the middle, then the text color over both.
	let colr = colr_table(0, &[(1, 0, 2)], &[(2, 0), (3, 1)]);
	let cpal = cpal_table(2, &[0], &[RED, half_blue]);
	let under = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let mixed = Font::from_bytes(under, 0)
		.unwrap()
		.rasterize_color('a', 32.0)
		.unwrap();
	assert_eq!(pixel(&mixed, 8, 8), 0xff7f_0080);
	assert_eq!(pixel(&glyph, 8, 8), WHITE, "an opaque layer on top");
}

#[test]
fn layers_far_apart_are_drawn_in_a_box_of_at_most_4096_pixels() {
	let colr = colr_table(0, &[(1, 0, 2)], &[(2, 0), (3, 0)]);
	let cpal = cpal_table(1, &[0], &[RED]);
	let outlines = [None, SQUARE, SQUARE, Some((30000, 0, 31024, 1024))];
	let font = font(&['a'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	// 31024 units at 300 pixels per em reach 4545 pixels to the right.
	let (metrics, pixels) = Font::from_bytes(font, 0)
		.unwrap()
		.rasterize_color('a', 300.0)
		.unwrap();
	assert_eq!((metrics.width, metrics.height), (4096, 150));
	assert_eq!(pixels.len(), 4096 * 150);
	assert_eq!(pixels[0], RED, "the square at the left edge is still drawn");
}

#[test]
fn the_renderer_takes_emoji_from_a_colr_font_and_text_from_outlines() {
	use crate::render::glyphs::{FontSet, Presentation};
	let colr = colr_table(0, &[(1, 0, 1)], &[(2, 0)]);
	let cpal = cpal_table(1, &[0], &[RED]);
	let outlines = [None, SQUARE, SQUARE];
	let emoji = font(&['😀'], &outlines, vec![(b"COLR", colr), (b"CPAL", cpal)]);
	let plain = font(&['😀'], &outlines, vec![]);
	let fonts = [plain, emoji].map(|bytes| Font::from_bytes(bytes, 0).unwrap());
	let bundled = Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	let mut set = FontSet::new(bundled, fonts.into());
	let glyph = set.presented('😀', 32.0, Presentation::Default);
	let color = glyph.color.as_ref().expect("drawn in color");
	assert!(color.iter().all(|&p| p == RED));
	let text = set.presented('😀', 32.0, Presentation::Text);
	assert!(text.color.is_none(), "VS15 takes the outline font");
	assert!(text.coverage.contains(&255));
}
