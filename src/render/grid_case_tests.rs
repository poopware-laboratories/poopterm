//! Grid scenes shared by the tests of renderers that take the grid's pen:
//! every style, decoration, kind of glyph, cursor, highlight, padding and
//! image layer, with the pixels the software renderer paints for them.
use super::{
	glyphs::{FontSet, clusters::tests::emoji_font},
	grid::{GridView, draw_grid},
	painter::Painter,
};
use crate::{
	fonts::{BUNDLED, Font},
	graphics::{Bitmap, Extent, Placement, Rect},
	parser::Parser,
	term::{CursorShape, Frame, Term},
	theme::{Theme, ThemeKind},
	ui::selection::{CellPoint, Selection},
};
use std::sync::Arc;

pub(crate) const BG: u32 = 0x10_1010;
pub(crate) const CELL: (usize, usize) = (8, 16);
pub(crate) const ORIGIN: (usize, usize) = (6, 4);
pub(crate) const SIZE: (usize, usize) = (6 + 30 * 8 + 6, 4 + 5 * 16 + 4);

/// Styles, decorations, wide and joined text, marks, box drawing, links,
/// scaled text and glyphs too wide for their cells, on a 30×5 grid.
pub(crate) const SAMPLE: &str = concat!(
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
pub(crate) fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), vec![emoji_font()])
}

pub(crate) fn frame(input: &str) -> Frame {
	let mut term = Term::new(30, 5, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame
}

pub(crate) fn view(frame: &Frame) -> GridView<'_> {
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

fn select(start: (usize, usize), end: (usize, usize), block: bool) -> Option<Selection> {
	let point = |(row, col)| CellPoint { row, col };
	Some(Selection {
		start: point(start),
		end: point(end),
		block,
	})
}

/// One scene: a frame and how the window shows it.
pub(crate) struct Case {
	pub name: String,
	pub frame: Frame,
	pub focused: bool,
	pub selection: Option<Selection>,
	pub search_match: Option<Selection>,
	pub padding: bool,
}

impl Case {
	fn new(name: String, frame: Frame) -> Self {
		Self {
			name,
			frame,
			focused: true,
			selection: None,
			search_match: None,
			padding: false,
		}
	}

	pub(crate) fn view(&self) -> GridView<'_> {
		let whole = Rect {
			x: 0,
			y: 0,
			width: SIZE.0,
			height: SIZE.1,
		};
		GridView {
			focused: self.focused,
			selection: self.selection,
			search_match: self.search_match,
			padding: self.padding.then_some(whole),
			..view(&self.frame)
		}
	}
}

/// [`SAMPLE`] plain; with every cursor shape, focused or not, at four
/// places; under stream and block selections, search matches and the
/// cursor guide; and with padding and images at every layer.
pub(crate) fn cases() -> Vec<Case> {
	let mut cases = vec![Case::new("plain".into(), frame(SAMPLE))];
	for (col, row) in [(0, 0), (2, 1), (12, 2), (29, 4)] {
		for shape in [CursorShape::Block, CursorShape::Underline, CursorShape::Bar] {
			for focused in [true, false] {
				let mut frame = frame(SAMPLE);
				frame.cursor = Some((col, row));
				frame.cursor_shape = shape;
				let name = format!("{shape:?} at {col},{row} focused {focused}");
				cases.push(Case {
					focused,
					..Case::new(name, frame)
				});
			}
		}
	}
	let first = frame(SAMPLE).first_row;
	for (selection, search_match) in [
		(select((first, 2), (first + 2, 9), false), None),
		(select((first + 1, 1), (first + 3, 6), true), None),
		(None, select((first + 1, 0), (first + 1, 12), false)),
		(
			select((first, 0), (first + 4, 29), false),
			select((first + 2, 3), (first + 2, 5), false),
		),
	] {
		let mut frame = frame(SAMPLE);
		frame.guide = Some(2);
		cases.push(Case {
			selection,
			search_match,
			..Case::new(format!("{selection:?} {search_match:?}"), frame)
		});
	}
	cases.push(Case {
		padding: true,
		..Case::new("images and padding".into(), with_images(frame(SAMPLE)))
	});
	cases
}

/// `frame` with translucent images under the backgrounds, between them and
/// the text, and over the text, an opaque one over the last rows, and one
/// stretched over cells.
fn with_images(mut frame: Frame) -> Frame {
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
	let mut stretched = Placement::natural(bitmap(0xc0_20_e0_40, false), 17, 0);
	stretched.extent = Extent::Cells { cols: 3, rows: 2 };
	frame.images.push(stretched);
	frame
}

/// The pixels the software renderer paints for `view`.
pub(crate) fn direct(view: &GridView) -> Vec<u32> {
	let mut pixels = vec![BG; SIZE.0 * SIZE.1];
	draw_grid(
		&mut Painter::new(&mut pixels, SIZE.0, SIZE.1),
		&mut fonts(),
		view,
		None,
	);
	pixels
}

/// Checks `drawn` equals what the software renderer paints for `view`.
pub(crate) fn assert_paints_like_software(view: &GridView, drawn: &[u32], what: &str) {
	let expected = direct(view);
	assert!(expected.iter().any(|&pixel| pixel != BG), "{what}: blank");
	let differ = expected.iter().zip(drawn).filter(|(a, b)| a != b).count();
	assert_eq!(differ, 0, "{what}: {differ} pixels differ");
}
