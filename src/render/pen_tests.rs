//! Tests of the grid's pen: a pen that records what the grid draws, then
//! replays it, paints the same pixels as drawing directly, so the calls a
//! GPU renderer gets say everything; and every glyph's key finds its bitmap
//! again.
use super::{
	boxdraw::Drawn,
	glyphs::clusters::tests::emoji_font,
	glyphs::{FontSet, Glyph, Presentation, Shape},
	grid::{GridView, draw_grid},
	images::draw_placement,
	painter::Painter,
	pen::{Ink, Pen},
};
use crate::{
	fonts::{BUNDLED, Font, Metrics},
	graphics::{Bitmap, Placement, Rect},
	parser::Parser,
	term::{CursorShape, Frame, Term},
	theme::{Theme, ThemeKind},
	ui::selection::{CellPoint, Selection},
};
use std::{ops::Range, sync::Arc};

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

const BG: u32 = 0x10_1010;
const CELL: (usize, usize) = (8, 16);
const ORIGIN: (usize, usize) = (6, 4);
const SIZE: (usize, usize) = (6 + 30 * 8 + 6, 4 + 5 * 16 + 4);

/// Styles, decorations, wide and joined text, marks, box drawing, links,
/// scaled text and glyphs too wide for their cells, on a 30×5 grid.
const SAMPLE: &str = concat!(
	"\x1b[1;31mred\x1b[0m \x1b[4;58:2::255:0:0mul\x1b[24m \x1b[21mdbl\x1b[24m ",
	"\x1b[53mov\x1b[55m \x1b[9mst\x1b[29m \x1b[2mdim\x1b[22m\r\n",
	"猫 e\u{301} 🐱 👍🏽 👨\u{200d}👩\u{200d}👧 ❤\u{fe0f} ─│┼█▀⠿ ",
	"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\\r\n",
	"\x1b[3mitalic\x1b[0m \x1b[7mrev\x1b[27m W@MW \x1b[48;5;196m bg \x1b[0m ",
	"1\u{fe0f}\u{20e3} ❤\u{fe0f}\u{200d}🔥 \x1b]66;s=2;hi\x07\r\n\r\n",
	"\x1b[44mblue to the edge\x1b[K\x1b[0m",
);

/// The bundled font, with a color font joining a keycap 1 and a heart on
/// fire as its fallback.
fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), vec![emoji_font()])
}

fn frame(input: &str) -> Frame {
	let mut term = Term::new(30, 5, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame
}

fn view(frame: &Frame) -> GridView<'_> {
	GridView {
		frame,
		origin: ORIGIN,
		cell: CELL,
		px: 13.0,
		ascent: 12,
		scale: 1,
		default_bg: BG,
		link_color: 0x00_ff00,
		selection_colors: (0xff_ffff, 0x33_66cc),
		match_colors: (0x00_0000, 0xff_cc00),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
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

/// Draws `view` directly and through a record, returning both.
fn both(view: &GridView) -> (Vec<u32>, Vec<u32>) {
	let mut direct = vec![BG; SIZE.0 * SIZE.1];
	draw_grid(
		&mut Painter::new(&mut direct, SIZE.0, SIZE.1),
		&mut fonts(),
		view,
		None,
	);
	let mut record = GridRecord::new(SIZE.0, SIZE.1);
	draw_grid(&mut record, &mut fonts(), view, None);
	let mut replayed = vec![BG; SIZE.0 * SIZE.1];
	replay(
		&record,
		&mut Painter::new(&mut replayed, SIZE.0, SIZE.1),
		&mut fonts(),
	);
	(direct, replayed)
}

fn assert_same(view: &GridView, what: &str) {
	let (direct, replayed) = both(view);
	assert!(direct.iter().any(|&pixel| pixel != BG), "{what}: blank");
	let differ = direct.iter().zip(&replayed).filter(|(a, b)| a != b).count();
	assert_eq!(differ, 0, "{what}: {differ} pixels differ");
}

fn select(start: (usize, usize), end: (usize, usize), block: bool) -> Option<Selection> {
	let point = |(row, col)| CellPoint { row, col };
	Some(Selection {
		start: point(start),
		end: point(end),
		block,
	})
}

#[test]
fn replaying_the_record_paints_what_drawing_directly_paints() {
	let frame = frame(SAMPLE);
	assert_same(&view(&frame), "plain");
}

#[test]
fn every_cursor_shape_focused_or_not_replays_the_same() {
	let mut frame = frame(SAMPLE);
	for (col, row) in [(0, 0), (2, 1), (12, 2), (29, 4)] {
		for shape in [CursorShape::Block, CursorShape::Underline, CursorShape::Bar] {
			for focused in [true, false] {
				frame.cursor = Some((col, row));
				frame.cursor_shape = shape;
				let view = GridView {
					focused,
					..view(&frame)
				};
				assert_same(
					&view,
					&format!("{shape:?} at {col},{row} focused {focused}"),
				);
			}
		}
	}
}

#[test]
fn selections_matches_and_the_guide_replay_the_same() {
	let mut frame = frame(SAMPLE);
	frame.guide = Some(2);
	let first = frame.first_row;
	for (selection, search_match) in [
		(select((first, 2), (first + 2, 9), false), None),
		(select((first + 1, 1), (first + 3, 6), true), None),
		(None, select((first + 1, 0), (first + 1, 12), false)),
		(
			select((first, 0), (first + 4, 29), false),
			select((first + 2, 3), (first + 2, 5), false),
		),
	] {
		let view = GridView {
			selection,
			search_match,
			..view(&frame)
		};
		assert_same(&view, &format!("{selection:?} {search_match:?}"));
	}
}

#[test]
fn padding_and_images_at_every_layer_replay_the_same() {
	let mut frame = frame(SAMPLE);
	let bitmap = |color: u32, opaque: bool| {
		Arc::new(Bitmap {
			width: 20,
			height: 30,
			opaque,
			pixels: vec![color; 20 * 30],
		})
	};
	for (z, col) in [(i32::MIN, 1), (-5, 6), (3, 11)] {
		let mut image = Placement::natural(bitmap(0x80_ff_00_80, false), col, 1);
		image.z = z;
		frame.images.push(image);
	}
	frame
		.images
		.push(Placement::natural(bitmap(0xff_00_80_ff, true), 25, 3));
	let view = GridView {
		padding: Some(Rect {
			x: 0,
			y: 0,
			width: SIZE.0,
			height: SIZE.1,
		}),
		..view(&frame)
	};
	assert_same(&view, "images and padding");
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
