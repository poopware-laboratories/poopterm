//! Tests of the grid's pen: a pen that records what the grid draws, then
//! replays it, paints the same pixels as drawing directly, so the calls a
//! GPU renderer gets say everything; and every glyph's key finds its bitmap
//! again.
use super::{
	boxdraw::Drawn,
	glyphs::{FontSet, Glyph, Presentation, Shape},
	grid::{GridView, draw_grid},
	grid_case_tests::{
		BG, CELL, ORIGIN, SIZE, assert_paints_like_software, cases, fonts, frame, view,
	},
	images::draw_placement,
	painter::Painter,
	pen::{Ink, Pen},
};
use crate::{
	fonts::Metrics,
	graphics::{Placement, Rect},
};
use std::ops::Range;

/// Which bitmap a glyph stroke shows.
#[derive(Clone, Debug, PartialEq)]
enum InkKey {
	Char(char, Presentation, f32),
	/// A joined cluster: its characters in [`GridRecord::chars`], and size.
	Cluster(Range<usize>, f32),
	/// A character made to fill a cell of this size.
	Drawn(char, (usize, usize)),
}

/// One drawing operation, in pixels of the surface, in drawing order.
#[derive(Clone, Debug)]
enum Stroke {
	Rect {
		x: usize,
		y: usize,
		w: usize,
		h: usize,
		color: u32,
	},
	/// A glyph bitmap with its top-left corner at `origin`, clipped to
	/// `clip` (left, top, right, bottom).
	Glyph {
		ink: InkKey,
		metrics: Metrics,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	},
	/// The part of an image inside `clip`, its top-left corner at `at`.
	Image {
		clip: Rect,
		placement: Placement,
		at: (isize, isize),
		cell: (usize, usize),
	},
}

/// What the grid drew on a surface `width` by `height` pixels.
#[derive(Debug, Default)]
struct GridRecord {
	pub width: usize,
	pub height: usize,
	pub strokes: Vec<Stroke>,
	/// The characters of joined clusters, which their keys index.
	pub chars: Vec<char>,
}

impl GridRecord {
	fn new(width: usize, height: usize) -> Self {
		Self {
			width,
			height,
			..Self::default()
		}
	}
}

impl Pen for GridRecord {
	fn size(&self) -> (usize, usize) {
		(self.width, self.height)
	}

	fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		self.strokes.push(Stroke::Rect { x, y, w, h, color });
	}

	fn glyph(
		&mut self,
		glyph: &Glyph,
		ink: Ink,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		let ink = match ink {
			Ink::Text(Shape::Char(ch, presentation), px) => InkKey::Char(ch, presentation, px),
			Ink::Text(Shape::Cluster(chars), px) => {
				let start = self.chars.len();
				self.chars.extend_from_slice(chars);
				InkKey::Cluster(start..self.chars.len(), px)
			}
			Ink::Drawn(ch, cell) => InkKey::Drawn(ch, cell),
		};
		self.strokes.push(Stroke::Glyph {
			ink,
			metrics: glyph.metrics,
			origin,
			clip,
			italic,
			color,
		});
	}

	fn placement(
		&mut self,
		clip: Rect,
		placement: &Placement,
		at: (isize, isize),
		cell: (usize, usize),
	) {
		self.strokes.push(Stroke::Image {
			clip,
			placement: placement.clone(),
			at,
			cell,
		});
	}
}

/// Paints `record` the way the grid painted it, checking each glyph's key
/// finds a bitmap with the recorded metrics.
fn replay(record: &GridRecord, p: &mut Painter, fonts: &mut FontSet) {
	for stroke in &record.strokes {
		match stroke {
			&Stroke::Rect { x, y, w, h, color } => p.rect(x, y, w, h, color),
			Stroke::Glyph {
				ink,
				metrics,
				origin,
				clip,
				italic,
				color,
			} => {
				let glyph = match *ink {
					InkKey::Char(ch, presentation, px) => fonts.presented(ch, px, presentation),
					InkKey::Cluster(ref chars, px) => {
						fonts.shaped(Shape::Cluster(&record.chars[chars.clone()]), px)
					}
					InkKey::Drawn(ch, cell) => match fonts.drawn(ch, cell) {
						Some(Drawn::Bitmap { glyph, .. }) => glyph,
						_ => panic!("{ch:?} drew no bitmap"),
					},
				};
				assert_eq!(glyph.metrics, *metrics, "{ink:?}");
				p.glyph(glyph, *origin, *clip, *italic, *color);
			}
			Stroke::Image {
				clip,
				placement,
				at,
				cell,
			} => draw_placement(p.pixels, p.width, *clip, placement, *at, *cell),
		}
	}
}

/// Draws `view` through a record and replays it.
fn replayed(view: &GridView) -> Vec<u32> {
	let mut record = GridRecord::new(SIZE.0, SIZE.1);
	draw_grid(&mut record, &mut fonts(), view, None);
	let mut pixels = vec![BG; SIZE.0 * SIZE.1];
	replay(
		&record,
		&mut Painter::new(&mut pixels, SIZE.0, SIZE.1),
		&mut fonts(),
	);
	pixels
}

fn assert_same(view: &GridView, what: &str) {
	assert_paints_like_software(view, &replayed(view), what);
}

#[test]
fn every_case_replays_what_drawing_directly_paints() {
	for case in cases() {
		assert_same(&case.view(), &case.name);
	}
}

/// The glyph strokes of `input` drawn alone.
fn inks(input: &str) -> (GridRecord, Vec<InkKey>) {
	let mut frame = frame(input);
	frame.cursor = None;
	let mut record = GridRecord::new(SIZE.0, SIZE.1);
	draw_grid(&mut record, &mut fonts(), &view(&frame), None);
	let inks = record.strokes.iter().filter_map(|stroke| match stroke {
		Stroke::Glyph { ink, .. } => Some(ink.clone()),
		_ => None,
	});
	let inks = inks.collect();
	(record, inks)
}

#[test]
fn glyphs_are_keyed_by_character_presentation_and_size() {
	let (record, inks) = inks("a\u{301}❤\u{fe0f}╭─");
	// Right to left: the arc (lines are rectangles, not glyphs), the heart,
	// the a, then its mark.
	assert_eq!(
		inks,
		[
			InkKey::Drawn('╭', CELL),
			InkKey::Char('❤', Presentation::Emoji, 13.0),
			InkKey::Char('a', Presentation::Default, 13.0),
			InkKey::Char('\u{301}', Presentation::Default, 13.0),
		]
	);
	let line = (record.strokes.iter()).filter(
		|stroke| matches!(stroke, Stroke::Rect { x, w, .. } if *x == ORIGIN.0 + 4 * CELL.0 && *w > 0),
	);
	assert!(line.count() > 0, "the line is drawn as rectangles");
}

#[test]
fn joined_clusters_keep_their_characters_beside_the_strokes() {
	let (record, inks) = inks("1\u{fe0f}\u{20e3} ❤\u{fe0f}\u{200d}🔥 ok");
	let clusters: Vec<_> = (inks.iter())
		.filter_map(|ink| match ink {
			InkKey::Cluster(chars, px) => Some((&record.chars[chars.clone()], *px)),
			_ => None,
		})
		.collect();
	// Right to left, each cluster's characters after the last one's.
	let heart_on_fire = &['❤', '\u{fe0f}', '\u{200d}', '🔥'][..];
	let keycap = &['1', '\u{fe0f}', '\u{20e3}'][..];
	assert_eq!(clusters, [(heart_on_fire, 13.0), (keycap, 13.0)]);
	assert_eq!(record.chars.len(), 7);
}

#[test]
fn glyphs_too_wide_for_their_cell_are_keyed_at_the_size_drawn() {
	// "W" at 40 px is far wider than an 8-pixel cell, so it shrinks.
	let frame = frame("W");
	let mut record = GridRecord::new(SIZE.0, SIZE.1);
	let view = GridView {
		px: 40.0,
		..view(&frame)
	};
	draw_grid(&mut record, &mut fonts(), &view, None);
	let sizes: Vec<f32> = (record.strokes.iter())
		.filter_map(|stroke| match stroke {
			Stroke::Glyph {
				ink: InkKey::Char('W', _, px),
				..
			} => Some(*px),
			_ => None,
		})
		.collect();
	assert_eq!(sizes.len(), 1);
	assert!(sizes[0] < 40.0, "drawn at {}", sizes[0]);
	assert_same(&view, "shrunk W");
}

#[test]
fn scaled_text_is_keyed_at_its_scaled_size_and_presentation() {
	let input = "\x1b]66;s=2;❤\u{fe0f}a\x07";
	let (_, inks) = inks(input);
	// Each character is a block two rows tall; each row draws its slice of
	// the blocks, right to left like every cell.
	let heart = InkKey::Char('❤', Presentation::Emoji, 26.0);
	let a = InkKey::Char('a', Presentation::Default, 26.0);
	assert_eq!(inks, [a.clone(), heart.clone(), a, heart]);
	assert_same(&view(&frame(input)), "scaled emoji");
}
