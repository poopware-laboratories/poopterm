//! Tests of the grid as quads: drawn the way the GPU draws them, from the
//! atlas pixels the uploads sent, they paint what the software renderer
//! paints; glyphs upload once; and a full atlas marks the frame incomplete.
use super::{
	atlas::Slot,
	quads::{Fill, QuadPen, Upload},
};
use crate::{
	fonts::Metrics,
	graphics::{Bitmap, Extent, Placement, Rect},
	render::{
		blend::blend,
		glyphs::{Glyph, Presentation, Shape},
		grid::{GridView, draw_grid},
		grid_case_tests::{BG, SAMPLE, SIZE, assert_paints_like_software, cases, fonts, frame, view},
		images::draw_placement,
		painter::Painter,
		pen::{Ink, Pen},
	},
};
use std::sync::Arc;

const SIDE: usize = 256;

/// A stand-in for the GPU: texture pages filled by uploads, and quads
/// drawn from nothing else.
#[derive(Default)]
pub(super) struct Gpu {
	pages: Vec<Vec<u32>>,
}

impl Gpu {
	pub(super) fn upload(&mut self, uploads: &[Upload]) {
		for upload in uploads {
			let slot = upload.slot;
			if self.pages.len() <= slot.page {
				self.pages.resize(slot.page + 1, vec![0; SIDE * SIDE]);
			}
			let page = &mut self.pages[slot.page];
			for (row, texels) in upload.pixels.chunks(slot.w.max(1)).enumerate() {
				let at = (slot.y + row) * SIDE + slot.x;
				page[at..at + texels.len()].copy_from_slice(texels);
			}
		}
	}

	/// The texels `slot` covers, row by row.
	fn texels(&self, slot: Slot) -> Vec<u32> {
		let page = &self.pages[slot.page];
		let rows = (0..slot.h).map(|row| &page[(slot.y + row) * SIDE + slot.x..][..slot.w]);
		rows.flatten().copied().collect()
	}

	pub(super) fn draw(&self, pen: &QuadPen) -> Vec<u32> {
		let (width, height) = pen.size;
		let mut pixels = vec![BG; width * height];
		let p = &mut Painter::new(&mut pixels, width, height);
		for quad in &pen.quads {
			let (x, y) = quad.at;
			match quad.fill {
				Fill::Solid => p.rect(x as usize, y as usize, quad.size.0, quad.size.1, quad.color),
				Fill::Coverage(slot) | Fill::Color(slot) => {
					let texels = self.texels(slot);
					let color = matches!(quad.fill, Fill::Color(_));
					let glyph = Glyph {
						metrics: Metrics {
							width: slot.w,
							height: slot.h,
							..Metrics::default()
						},
						coverage: texels.iter().map(|texel| (texel >> 24) as u8).collect(),
						color: color.then_some(texels),
					};
					p.glyph(&glyph, quad.at, quad.clip, quad.italic, quad.color);
				}
				Fill::Image(index, source) => {
					// As the fragment shader maps each pixel to a texel.
					let bitmap = &pen.images[index].placement.bitmap;
					let (left, top, right, bottom) = quad.clip;
					let (w, h) = (quad.size.0 as i32, quad.size.1 as i32);
					let columns = x.max(left).max(0)..(x + w).min(right).min(width as i32);
					for row in y.max(top).max(0)..(y + h).min(bottom).min(height as i32) {
						let texel_y = source.y + (row - y) as usize * source.height / h as usize;
						for column in columns.clone() {
							let texel_x = source.x + (column - x) as usize * source.width / w as usize;
							let texel = bitmap.pixels[texel_y * bitmap.width + texel_x];
							let out = &mut p.pixels[row as usize * width + column as usize];
							*out = blend(*out, texel & 0xff_ffff, texel >> 24);
						}
					}
				}
			}
		}
		pixels
	}
}

fn pen() -> QuadPen {
	QuadPen::new(SIZE, (SIDE, 4))
}

/// Draws `view` with `pen` as a new frame, then on `gpu`.
fn draw(pen: &mut QuadPen, gpu: &mut Gpu, view: &GridView) -> Vec<u32> {
	pen.begin_frame(SIZE);
	draw_grid(pen, &mut fonts(), view, None);
	gpu.upload(&pen.uploads);
	gpu.draw(pen)
}

#[test]
fn every_case_drawn_as_quads_paints_what_software_paints() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	for case in cases() {
		let view = case.view();
		let drawn = draw(&mut pen, &mut gpu, &view);
		assert!(pen.complete, "{}", case.name);
		assert_paints_like_software(&view, &drawn, &case.name);
	}
}

#[test]
fn glyphs_upload_once_and_keep_their_slots_across_frames() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	let frame = frame("abcabc ❤\u{fe0f}❤\u{fe0f} ╭╭");
	draw(&mut pen, &mut gpu, &view(&frame));
	// a, b, c, the heart and the arc: one upload each.
	assert_eq!(pen.uploads.len(), 5);
	let quads = pen.quads.clone();
	let drawn = draw(&mut pen, &mut gpu, &view(&frame));
	assert_eq!(pen.uploads.len(), 0);
	assert_eq!(pen.quads, quads);
	assert_paints_like_software(&view(&frame), &drawn, "second frame");
}

#[test]
fn coverage_uploads_as_white_with_alpha_and_color_as_it_is() {
	let mut pen = pen();
	let mut fonts = fonts();
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts, &view(&frame("a❤\u{fe0f}")), None);
	let (a, heart) = (&pen.uploads[1], &pen.uploads[0]);
	let coverage = &fonts.glyph('a', 13.0).coverage;
	let expected: Vec<u32> = coverage
		.iter()
		.map(|&c| u32::from(c) << 24 | 0xff_ffff)
		.collect();
	assert_eq!(a.pixels, expected);
	let emoji = crate::render::glyphs::Presentation::Emoji;
	let color = fonts.presented('❤', 13.0, emoji).color.clone().unwrap();
	assert_eq!(heart.pixels, color);
	let fills: Vec<_> = (pen.quads.iter())
		.filter(|quad| !matches!(quad.fill, Fill::Solid))
		.map(|quad| matches!(quad.fill, Fill::Color(_)))
		.collect();
	assert_eq!(fills, [true, false], "the heart, then the a");
}

#[test]
fn one_character_at_two_sizes_or_two_cells_is_two_glyphs() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	let frame = frame("a╭");
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&frame), None);
	let wider = GridView {
		px: 20.0,
		cell: (12, 24),
		..view(&frame)
	};
	draw_grid(&mut pen, &mut fonts(), &wider, None);
	assert_eq!(pen.uploads.len(), 4);
	gpu.upload(&pen.uploads);
	let slots: Vec<_> = pen.uploads.iter().map(|upload| upload.slot).collect();
	assert!(
		slots
			.iter()
			.enumerate()
			.all(|(i, a)| slots[i + 1..].iter().all(|b| a != b))
	);
}

#[test]
fn a_glyph_the_atlas_cannot_hold_is_left_out_of_an_incomplete_frame() {
	// Pages of 10 pixels hold no glyph at 40 pixels.
	let mut pen = QuadPen::new(SIZE, (10, 1));
	let frame = frame("\x1b[41mW");
	let view = GridView {
		px: 40.0,
		cell: (30, 50),
		..view(&frame)
	};
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view, None);
	assert!(!pen.complete);
	assert!(pen.uploads.is_empty());
	assert!(pen.quads.iter().all(|quad| quad.fill == Fill::Solid));
	assert!(!pen.quads.is_empty(), "the background is still drawn");
	// The next frame starts complete again.
	pen.begin_frame(SIZE);
	assert!(pen.complete && pen.quads.is_empty());
}

#[test]
fn a_full_atlas_keeps_the_glyphs_already_drawn_this_frame() {
	// One 16-pixel page holds a few small glyphs, then is full.
	let mut pen = QuadPen::new(SIZE, (16, 1));
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&frame("abcdefgh")), None);
	assert!(!pen.complete);
	let glyphs = pen.quads.iter().filter(|quad| quad.fill != Fill::Solid);
	assert_eq!(glyphs.count(), pen.uploads.len());
	assert!(!pen.uploads.is_empty());
}

#[test]
fn a_new_frame_takes_the_new_size_and_forgets_the_last() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let mut frame = frame("image here");
	frame.images.push(crate::graphics::Placement::natural(
		std::sync::Arc::new(crate::graphics::Bitmap {
			width: 4,
			height: 4,
			opaque: true,
			pixels: vec![0xff_ffffff; 16],
		}),
		1,
		0,
	));
	draw_grid(&mut pen, &mut fonts(), &view(&frame), None);
	assert!(!pen.images.is_empty() && !pen.quads.is_empty());
	pen.begin_frame((100, 50));
	assert_eq!(pen.size, (100, 50));
	assert!(pen.quads.is_empty() && pen.images.is_empty() && pen.uploads.is_empty());
}

#[test]
fn presentations_clusters_and_drawn_cells_never_share_a_slot() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	// The heart as text and as emoji; the arc in cells of two heights.
	let hearts = frame("\u{2764}\u{fe0e} \u{2764}\u{fe0f}");
	draw(&mut pen, &mut gpu, &view(&hearts));
	assert_eq!(pen.uploads.len(), 2);
	let arc = frame("\u{256d}");
	let taller = GridView {
		cell: (8, 20),
		..view(&arc)
	};
	pen.begin_frame(SIZE);
	let mut fonts = fonts();
	draw_grid(&mut pen, &mut fonts, &view(&arc), None);
	draw_grid(&mut pen, &mut fonts, &taller, None);
	assert_eq!(pen.uploads.len(), 2, "one arc per cell height");
	// A cluster's characters never read as a character's key, nor a drawn
	// character's.
	let x = fonts.glyph('x', 13.0);
	let glyph = Glyph {
		metrics: x.metrics,
		coverage: x.coverage.clone(),
		color: None,
	};
	let inks = [
		Ink::Text(Shape::Char('x', Presentation::Emoji), 13.0),
		Ink::Text(Shape::Cluster(&['\u{1}', 'x']), 13.0),
		Ink::Drawn('x', (13.0f32.to_bits() as usize, 1)),
	];
	pen.begin_frame(SIZE);
	for ink in inks {
		pen.glyph(&glyph, ink, (0, 0), (0, 0, 9, 9), false, 0);
	}
	assert_eq!(pen.uploads.len(), 3);
}

#[test]
fn empty_glyphs_draw_nothing_and_upload_nothing() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let ink = Ink::Text(Shape::Char(' ', Presentation::Default), 13.0);
	for (width, height) in [(0, 0), (0, 5), (5, 0)] {
		let glyph = Glyph {
			metrics: Metrics {
				width,
				height,
				..Metrics::default()
			},
			coverage: Vec::new(),
			color: None,
		};
		pen.glyph(&glyph, ink, (0, 0), (0, 0, 9, 9), false, 0);
	}
	assert!(pen.quads.is_empty() && pen.uploads.is_empty() && pen.complete);
}

#[test]
fn a_page_full_last_frame_is_emptied_for_the_next() {
	// One 16-pixel page cannot hold all eight letters in one frame, but
	// once that frame is over its page makes room for the next.
	let (mut pen, mut gpu) = (QuadPen::new(SIZE, (16, 1)), Gpu::default());
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&frame("abcdefgh")), None);
	gpu.upload(&pen.uploads);
	assert!(!pen.complete);
	let frame = frame("a");
	let drawn = draw(&mut pen, &mut gpu, &view(&frame));
	assert!(pen.complete);
	assert_paints_like_software(&view(&frame), &drawn, "after eviction");
}

#[test]
fn scaled_shrunk_and_italic_text_draws_as_software_draws_it() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	let scaled = frame("\x1b]66;s=2;\u{2764}\u{fe0f}a\x07 \x1b]66;s=3;b\x07");
	let italic = frame("\x1b[3mitalic \u{2764}\u{fe0f} \u{256d}\x1b[1;4mbold\x1b[m");
	let wide = frame("WMW");
	let views = [
		("scaled", view(&scaled)),
		("italic", view(&italic)),
		(
			"shrunk",
			GridView {
				px: 40.0,
				..view(&wide)
			},
		),
	];
	for (what, view) in views {
		let drawn = draw(&mut pen, &mut gpu, &view);
		assert!(pen.complete, "{what}");
		assert_paints_like_software(&view, &drawn, what);
	}
}

#[test]
fn quads_cover_the_rectangles_and_images_they_were_given() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	pen.rect(3, 5, 7, 2, 0xff_0000);
	let image = crate::graphics::Placement::natural(
		std::sync::Arc::new(crate::graphics::Bitmap::new(2, 2, vec![0xff_ffffff; 4])),
		0,
		0,
	);
	let clip = Rect {
		x: 4,
		y: 6,
		width: 9,
		height: 3,
	};
	pen.placement(clip, &image, (-1, 2), (8, 16));
	let [rect, picture] = &pen.quads[..] else {
		panic!("{:?}", pen.quads)
	};
	assert_eq!(
		(rect.at, rect.size, rect.clip),
		((3, 5), (7, 2), (3, 5, 10, 7))
	);
	assert_eq!((rect.fill, rect.color), (Fill::Solid, 0xff_0000));
	// The whole picture, which the clip cuts down.
	assert_eq!((picture.at, picture.size), ((-1, 2), (2, 2)));
	let whole = image.source;
	assert_eq!(
		(picture.clip, picture.fill),
		((4, 6, 13, 9), Fill::Image(0, whole))
	);
	let drawn = &pen.images[0];
	assert_eq!((drawn.at, drawn.cell), ((-1, 2), (8, 16)));
}

/// A placement of a `width` × `height` bitmap showing `source` as `extent`,
/// `offset` into its first cell.
fn placement(
	(width, height): (usize, usize),
	source: Rect,
	extent: Extent,
	offset: (usize, usize),
) -> Placement {
	let pixels = (0..width * height)
		.map(|n| 0x8000_0000 | (n as u32 * 0x01_0305))
		.collect();
	let mut placement = Placement::natural(Arc::new(Bitmap::new(width, height, pixels)), 0, 0);
	placement.source = source;
	placement.extent = extent;
	placement.offset = offset;
	placement
}

const fn rect(x: usize, y: usize, width: usize, height: usize) -> Rect {
	Rect {
		x,
		y,
		width,
		height,
	}
}

#[test]
fn an_image_quad_covers_the_scaled_picture_from_its_offset_and_keeps_its_source() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let source = rect(1, 2, 3, 4);
	let scaled = placement((5, 7), source, Extent::Cells { cols: 2, rows: 1 }, (3, 1));
	pen.placement(rect(0, 0, 100, 100), &scaled, (8, 16), (8, 16));
	let [picture] = &pen.quads[..] else {
		panic!("{:?}", pen.quads)
	};
	// Two cells wide and one high, from 3 pixels right and 1 down.
	assert_eq!((picture.at, picture.size), ((11, 17), (16, 16)));
	assert_eq!(picture.fill, Fill::Image(0, source));
	assert_eq!(picture.clip, (0, 0, 100, 100));
}

#[test]
fn an_image_scrolled_partly_above_the_view_starts_above_it_and_is_clipped() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let natural = placement((4, 40), rect(0, 0, 4, 40), Extent::Natural, (0, 2));
	// A row and a half above the clip's top, cells 16 high.
	pen.placement(rect(5, 6, 30, 20), &natural, (5, 6 - 24), (8, 16));
	let [picture] = &pen.quads[..] else {
		panic!("{:?}", pen.quads)
	};
	assert_eq!((picture.at, picture.size), ((5, -16), (4, 40)));
	assert_eq!(picture.clip, (5, 6, 35, 26));
	assert_eq!(pen.images[0].at, (5, -18));
}

#[test]
fn image_quads_name_their_image_even_past_one_that_shows_nothing() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let shown = placement((2, 2), rect(0, 0, 2, 2), Extent::Natural, (0, 0));
	// Sized in cells, yet showing no row of its bitmap.
	let cell = Extent::Cells { cols: 1, rows: 1 };
	let nothing = placement((2, 2), rect(0, 0, 2, 0), cell, (0, 0));
	for image in [&shown, &nothing, &shown] {
		pen.placement(rect(0, 0, 50, 50), image, (0, 0), (8, 16));
	}
	let fills: Vec<_> = pen.quads.iter().map(|quad| quad.fill).collect();
	let whole = rect(0, 0, 2, 2);
	assert_eq!(fills, [Fill::Image(0, whole), Fill::Image(1, whole)]);
	assert_eq!(pen.images.len(), 2);
}

#[test]
fn an_image_showing_nothing_draws_no_quad() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	let empty = [
		placement((4, 4), rect(0, 0, 0, 4), Extent::Natural, (0, 0)),
		placement(
			(4, 4),
			rect(0, 0, 4, 4),
			Extent::Pixels {
				width: 0,
				height: 9,
			},
			(0, 0),
		),
		placement(
			(4, 4),
			rect(0, 0, 4, 4),
			Extent::Pixels {
				width: 9,
				height: 0,
			},
			(0, 0),
		),
	];
	for image in &empty {
		pen.placement(rect(0, 0, 50, 50), image, (0, 0), (8, 16));
	}
	assert!(
		pen.quads.is_empty() && pen.images.is_empty(),
		"{:?}",
		pen.quads
	);
}

#[test]
fn image_quads_drawn_as_the_gpu_does_paint_what_software_paints() {
	let cases = [
		("natural", rect(0, 0, 5, 7), Extent::Natural, (0, 0)),
		("cropped", rect(1, 2, 3, 4), Extent::Natural, (2, 3)),
		(
			"scaled up",
			rect(0, 0, 5, 7),
			Extent::Pixels {
				width: 13,
				height: 17,
			},
			(1, 0),
		),
		(
			"scaled down",
			rect(1, 1, 4, 6),
			Extent::Pixels {
				width: 3,
				height: 2,
			},
			(0, 5),
		),
		(
			"in cells",
			rect(0, 0, 5, 7),
			Extent::Cells { cols: 3, rows: 2 },
			(7, 15),
		),
	];
	let clips = [
		rect(0, 0, SIZE.0, SIZE.1),
		rect(10, 20, 9, 11),
		rect(17, 25, 3, 40),
	];
	for (name, source, extent, offset) in cases {
		let image = placement((5, 7), source, extent, offset);
		for clip in clips {
			for at in [(8, 16), (-6, -9), (14, 30)] {
				let (mut pen, gpu) = (pen(), Gpu::default());
				pen.begin_frame(SIZE);
				pen.placement(clip, &image, at, (8, 16));
				let mut want = vec![BG; SIZE.0 * SIZE.1];
				draw_placement(&mut want, SIZE.0, clip, &image, at, (8, 16));
				assert!(
					gpu.draw(&pen) == want,
					"{name} clipped to {clip:?} at {at:?}"
				);
			}
		}
	}
}

#[test]
fn every_upload_fills_its_slot_and_glyph_quads_are_their_slots_size() {
	let mut pen = pen();
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&frame(SAMPLE)), None);
	assert!(pen.uploads.len() > 10);
	for upload in &pen.uploads {
		let slot = upload.slot;
		assert_eq!(upload.pixels.len(), slot.w * slot.h, "{slot:?}");
		assert!(slot.w > 0 && slot.h > 0 && slot.x + slot.w <= SIDE && slot.y + slot.h <= SIDE);
	}
	for quad in &pen.quads {
		if let Fill::Coverage(slot) | Fill::Color(slot) = quad.fill {
			assert_eq!(quad.size, (slot.w, slot.h));
		}
	}
}
