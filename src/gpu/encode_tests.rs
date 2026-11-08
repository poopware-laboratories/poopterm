//! Tests of quads encoded as the shaders read them.
use super::encode;
use crate::{
	gpu::{
		atlas::Slot,
		quads::{Fill, Quad},
	},
	graphics::Rect,
};

fn slot(page: usize, x: usize, y: usize, w: usize, h: usize) -> Slot {
	Slot { page, x, y, w, h }
}

fn quad(fill: Fill, italic: bool) -> Quad {
	Quad {
		at: (-3, 7),
		size: (5, 9),
		clip: (0, 2, 40, 30),
		fill,
		italic,
		color: 0x00ab_cdef,
	}
}

#[test]
fn quads_encode_as_the_shaders_read_them() {
	// Rectangle, clip, then atlas x | y << 16, page, kind | italic << 8 and
	// the color.
	let place = [-3i32 as u32, 7, 5, 9, 0, 2, 40, 30];
	let glyph = slot(3, 0x12, 0x345, 5, 9);
	let solid = encode(&quad(Fill::Solid, false)).unwrap();
	assert_eq!(solid[..8], place);
	assert_eq!(solid[8..], [0, 0, 0, 0x00ab_cdef]);
	let coverage = encode(&quad(Fill::Coverage(glyph), true)).unwrap();
	assert_eq!(coverage[8..], [0x0345_0012, 3, 0x101, 0x00ab_cdef]);
	let color = encode(&quad(Fill::Color(glyph), false)).unwrap();
	assert_eq!(color[8..], [0x0345_0012, 3, 2, 0x00ab_cdef]);
	// Only coverage leans: an italic emoji keeps its shape.
	assert_eq!(encode(&quad(Fill::Color(glyph), true)), Some(color));
	assert_eq!(encode(&quad(Fill::Solid, true)), Some(solid));
	// Images: source x | y << 16, width | height << 16, kind 3; no color.
	let source = Rect {
		x: 0x12,
		y: 0x345,
		width: 0x6789,
		height: 0xffff,
	};
	let image = encode(&quad(Fill::Image(7, source), true)).unwrap();
	assert_eq!(image[..8], place);
	assert_eq!(image[8..], [0x0345_0012, 0xffff_6789, 3, 0x00ab_cdef]);
}

#[test]
fn an_image_source_past_sixteen_bits_does_not_encode() {
	let fits = Rect {
		x: 0xffff,
		y: 0xffff,
		width: 0xffff,
		height: 0xffff,
	};
	assert!(encode(&quad(Fill::Image(0, fits), false)).is_some());
	for past in [
		Rect {
			x: 0x1_0000,
			..fits
		},
		Rect {
			y: 0x1_0000,
			..fits
		},
		Rect {
			width: 0x1_0000,
			..fits
		},
		Rect {
			height: 0x1_0000,
			..fits
		},
	] {
		assert_eq!(encode(&quad(Fill::Image(0, past), false)), None, "{past:?}");
	}
}
