//! Tests of what a frame's images ask of their textures: the size each
//! bitmap is shown at whole, the largest of its placements', and image
//! quads carried into a texture smaller than their bitmap.
use super::{
	quads::{Fill, Quad, QuadPen},
	shrink::{in_texture, shown_sizes},
};
use crate::{
	graphics::{Bitmap, Extent, Placement, Rect},
	render::pen::Pen,
};
use std::sync::Arc;

const SIZE: (usize, usize) = (400, 300);

fn bitmap(width: usize, height: usize) -> Arc<Bitmap> {
	Arc::new(Bitmap::new(
		width,
		height,
		vec![0xff00_0000; width * height],
	))
}

/// Places `bitmap`'s `source` at the top left, `extent` large.
fn place(pen: &mut QuadPen, bitmap: &Arc<Bitmap>, source: Rect, extent: Extent) {
	let mut placement = Placement::natural(bitmap.clone(), 0, 0);
	(placement.source, placement.extent) = (source, extent);
	let clip = Rect {
		x: 0,
		y: 0,
		width: SIZE.0,
		height: SIZE.1,
	};
	pen.placement(clip, &placement, (0, 0), (8, 16));
}

const fn whole(width: usize, height: usize) -> Rect {
	Rect {
		x: 0,
		y: 0,
		width,
		height,
	}
}

fn pixels(width: usize, height: usize) -> Extent {
	Extent::Pixels { width, height }
}

#[test]
fn an_image_is_shown_at_the_size_it_is_placed_at() {
	let mut pen = QuadPen::new(SIZE, (64, 1));
	pen.begin_frame(SIZE);
	let (photo, icon) = (bitmap(4000, 3000), bitmap(10, 10));
	place(&mut pen, &photo, whole(4000, 3000), pixels(400, 300));
	place(&mut pen, &icon, whole(10, 10), Extent::Natural);
	assert_eq!(shown_sizes(&pen), [(400, 300), (10, 10)]);
}

#[test]
fn a_cropped_image_is_shown_whole_at_the_scale_of_its_crop() {
	// The left half shown 20 wide: the whole would be 40 wide.
	let mut pen = QuadPen::new(SIZE, (64, 1));
	pen.begin_frame(SIZE);
	let left = Rect {
		width: 50,
		..whole(100, 100)
	};
	place(&mut pen, &bitmap(100, 100), left, pixels(20, 30));
	assert_eq!(shown_sizes(&pen), [(40, 30)]);
}

#[test]
fn a_bitmap_placed_twice_is_shown_at_the_largest_of_its_placements() {
	let mut pen = QuadPen::new(SIZE, (64, 1));
	pen.begin_frame(SIZE);
	let (shared, other) = (bitmap(1000, 1000), bitmap(1000, 1000));
	place(&mut pen, &shared, whole(1000, 1000), pixels(10, 300));
	place(&mut pen, &other, whole(1000, 1000), pixels(200, 200));
	place(&mut pen, &shared, whole(1000, 1000), pixels(50, 20));
	assert_eq!(shown_sizes(&pen), [(50, 300), (200, 200), (50, 300)]);
}

fn quad(fill: Fill) -> Quad {
	Quad {
		at: (3, 4),
		size: (20, 10),
		clip: (0, 0, 40, 30),
		fill,
		italic: false,
		color: 0,
	}
}

#[test]
fn an_image_quad_shows_the_same_part_of_a_smaller_texture() {
	let source = Rect {
		x: 100,
		y: 50,
		width: 200,
		height: 100,
	};
	let shrunk = in_texture(quad(Fill::Image(2, source)), (400, 200), (40, 20));
	let carried = Rect {
		x: 10,
		y: 5,
		width: 20,
		height: 10,
	};
	assert_eq!(shrunk, quad(Fill::Image(2, carried)));
	// In a texture its bitmap's size, and for any other fill, as it was.
	let same = quad(Fill::Image(2, source));
	assert_eq!(in_texture(same, (400, 200), (400, 200)), same);
	assert_eq!(
		in_texture(quad(Fill::Solid), (400, 200), (40, 20)),
		quad(Fill::Solid)
	);
}

#[test]
fn an_image_mostly_off_screen_is_shown_whole_at_its_full_scale() {
	let mut pen = QuadPen::new(SIZE, (64, 1));
	pen.begin_frame(SIZE);
	// One column of a very wide bitmap, 300 wide, its left 290 off screen.
	let mut placement = Placement::natural(bitmap(100_000, 2), 0, 0);
	(placement.source, placement.extent) = (whole(1, 2), pixels(300, 4));
	pen.placement(whole(SIZE.0, SIZE.1), &placement, (-290, 0), (8, 16));
	assert_eq!(pen.images.len(), 1);
	assert_eq!(shown_sizes(&pen), [(30_000_000, 4)]);
}
