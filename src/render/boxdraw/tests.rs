//! Drawn box, block, Braille and Powerline glyphs: they fill their cells
//! exactly, so lines meet their neighbors and blocks tile without seams.
use super::*;

const SIZES: [(usize, usize); 5] = [(8, 16), (9, 19), (10, 21), (13, 27), (24, 50)];

fn glyph(ch: char, (w, h): (usize, usize)) -> Vec<u8> {
	draw(ch, (w, h)).unwrap_or_else(|| panic!("{ch:?} is not drawn"))
}

/// Coverage down column `x`, or along row `y`.
fn column(bitmap: &[u8], (w, _): (usize, usize), x: usize) -> Vec<u8> {
	bitmap.iter().skip(x).step_by(w).copied().collect()
}

fn row(bitmap: &[u8], (w, _): (usize, usize), y: usize) -> Vec<u8> {
	bitmap[y * w..(y + 1) * w].to_vec()
}

#[test]
fn every_covered_code_point_fills_exactly_its_cell() {
	let ranges = [
		0x2500..=0x259f,
		0x2800..=0x28ff,
		0x1fb00..=0x1fb3b,
		0xe0b0..=0xe0bf,
	];
	for size in SIZES {
		for ch in ranges.iter().cloned().flatten().filter_map(char::from_u32) {
			assert!(covers(ch), "{ch:?}");
			let bitmap = glyph(ch, size);
			assert_eq!(bitmap.len(), size.0 * size.1, "{ch:?}");
			if ch != '\u{2800}' {
				assert!(bitmap.iter().any(|&c| c > 0), "{ch:?} at {size:?} is blank");
			}
		}
	}
	assert!(!covers('a') && !covers('\u{25a0}') && draw('a', (8, 16)).is_none());
}

#[test]
fn lines_meet_their_neighbors_at_every_edge() {
	// (glyph, edge, partner whose opposite edge must match)
	let pairs = [
		('┌', 'R', '─'),
		('┌', 'D', '│'),
		('┏', 'R', '━'),
		('┗', 'U', '┃'),
		('╔', 'R', '═'),
		('╔', 'D', '║'),
		('╝', 'L', '═'),
		('╝', 'U', '║'),
		('╭', 'R', '─'),
		('╭', 'D', '│'),
		('╯', 'L', '─'),
		('╯', 'U', '│'),
		('┼', 'L', '─'),
		('╋', 'U', '┃'),
		('╬', 'R', '═'),
		('╪', 'U', '│'),
		('╫', 'R', '─'),
		('╼', 'R', '━'),
		('╽', 'D', '┃'),
	];
	for size @ (w, h) in SIZES {
		for (ch, edge, partner) in pairs {
			let (a, b) = (glyph(ch, size), glyph(partner, size));
			let (ours, theirs) = match edge {
				'L' => (column(&a, size, 0), column(&b, size, w - 1)),
				'R' => (column(&a, size, w - 1), column(&b, size, 0)),
				'U' => (row(&a, size, 0), row(&b, size, h - 1)),
				_ => (row(&a, size, h - 1), row(&b, size, 0)),
			};
			assert_eq!(
				ours, theirs,
				"{ch:?} {edge} against {partner:?} at {size:?}"
			);
			assert!(ours.contains(&255), "{ch:?} {edge} at {size:?}");
		}
	}
}

#[test]
fn heavy_lines_are_thicker_and_double_lines_have_a_gap() {
	for size in SIZES {
		let strokes = |ch| {
			let edge = column(&glyph(ch, size), size, 0);
			let ink = edge.iter().filter(|&&c| c == 255).count();
			// Runs of ink down the edge.
			let runs = edge.windows(2).filter(|w| w[0] == 0 && w[1] > 0).count();
			(ink, runs)
		};
		let (light, heavy, double) = (strokes('─'), strokes('━'), strokes('═'));
		assert!(heavy.0 > light.0, "{size:?}");
		assert_eq!((light.1, double.1), (1, 2), "{size:?}");
	}
}

#[test]
fn double_joints_leave_their_gaps_open_and_crossings_cross() {
	let size @ (w, h) = (24, 50);
	let at = |ch, x: usize, y: usize| glyph(ch, size)[y * w + x];
	// The gap of ═ runs through the middle of ╬ untouched.
	let gap_row = (1..h)
		.find(|&y| at('═', 0, y) == 0 && at('═', 0, y - 1) == 255)
		.unwrap();
	assert_eq!(at('╬', w / 2, gap_row), 0);
	// ╫ carries its single horizontal line through the gap of ║.
	let line_row = (0..h).find(|&y| at('─', 0, y) == 255).unwrap();
	let gap_column = (1..w)
		.find(|&x| at('║', x, 0) == 0 && at('║', x - 1, 0) == 255)
		.unwrap();
	assert_eq!(at('╫', gap_column, line_row), 255);
	// ╟ starts its line at the right stroke of ║, leaving the gap open.
	assert_eq!(at('╟', gap_column, line_row), 0);
}

#[test]
fn halves_quadrants_and_eighths_tile_without_seams_or_overlap() {
	let complements = [
		('▀', '▄'),
		('▌', '▐'),
		('▚', '▞'),
		('▘', '▟'),
		('▝', '▙'),
		('▖', '▜'),
	];
	for size in SIZES {
		assert!(glyph('█', size).iter().all(|&c| c == 255));
		for (a, b) in complements {
			let (a_ink, b_ink) = (glyph(a, size), glyph(b, size));
			let mut sum = a_ink
				.iter()
				.zip(b_ink)
				.map(|(&a, b)| u16::from(a) + u16::from(b));
			assert!(sum.all(|c| c == 255), "{a:?} + {b:?} at {size:?}");
		}
		let ink = |ch| {
			glyph(ch, size)
				.iter()
				.map(|&c| usize::from(c))
				.sum::<usize>()
		};
		let lower: Vec<usize> = ('▁'..='█').map(ink).collect();
		let left: Vec<usize> = ('▉'..='▏').map(ink).collect();
		assert!(
			lower.is_sorted() && left.iter().rev().is_sorted(),
			"{size:?}"
		);
		assert!(ink('░') < ink('▒') && ink('▒') < ink('▓') && ink('▓') < ink('█'));
		// Upper and right eighths sit flush with their edges.
		assert_eq!(row(&glyph('▔', size), size, 0), vec![255; size.0]);
		assert!(
			column(&glyph('▕', size), size, size.0 - 1)
				.iter()
				.all(|&c| c == 255)
		);
	}
}

#[test]
fn braille_and_sextants_place_each_dot_in_its_own_slot() {
	let size @ (w, h) = (10, 20);
	let ink_in = |bitmap: &[u8], (x0, x1): (usize, usize), (y0, y1): (usize, usize)| {
		(y0..y1)
			.flat_map(|y| (x0..x1).map(move |x| (x, y)))
			.any(|(x, y)| bitmap[y * w + x] > 0)
	};
	// Dot 1 is the top left of a 2×4 grid, dot 8 the bottom right.
	let one = glyph('\u{2801}', size);
	assert!(ink_in(&one, (0, w / 2), (0, h / 4)));
	assert!(!ink_in(&one, (w / 2, w), (0, h)) && !ink_in(&one, (0, w), (h / 4, h)));
	let eight = glyph('\u{2880}', size);
	assert!(ink_in(&eight, (w / 2, w), (3 * h / 4, h)));
	assert!(!ink_in(&eight, (0, w), (0, 3 * h / 4)));
	let all = glyph('\u{28ff}', size);
	let ink: usize = all.iter().map(|&c| usize::from(c)).sum();
	assert!(
		ink > 8 * 255 && ink < w * h * 255 / 3,
		"dots stay apart: {ink}"
	);
	// Sextants number their 2×3 blocks as bits; U+1FB00 is pattern 1, the
	// top-left block alone.
	let first = glyph('\u{1fb00}', size);
	assert!(ink_in(&first, (0, w / 2), (0, h / 3)));
	assert!(!ink_in(&first, (w / 2, w), (0, h)) && !ink_in(&first, (0, w), (h / 3 + 1, h)));
	// Pattern 21 (the left column) is skipped; 0x1fb14 is pattern 22.
	let skipped = glyph('\u{1fb14}', size);
	assert!(
		ink_in(&skipped, (w / 2, w), (0, h / 3))
			&& ink_in(&skipped, (0, w / 2), (h / 3 + 1, 2 * h / 3))
	);
}

#[test]
fn powerline_separators_reach_the_cell_edges() {
	for size @ (w, h) in SIZES {
		let solid = glyph('\u{e0b0}', size);
		let base = column(&solid, size, 0);
		assert!(
			base.iter().all(|&c| c > 0) && base[1..h - 1].iter().all(|&c| c == 255),
			"{size:?}"
		);
		let tip = column(&solid, size, w - 1);
		assert!(
			tip[0] == 0 && tip[h - 1] == 0 && tip[h / 2] > 0,
			"{size:?}: {tip:?}"
		);
		let left = glyph('\u{e0b2}', size);
		assert!(
			column(&left, size, w - 1)[1..h - 1]
				.iter()
				.all(|&c| c == 255)
		);
		let round = glyph('\u{e0b4}', size);
		assert!(column(&round, size, 0).iter().all(|&c| c > 0));
	}
}

#[test]
fn diagonals_and_arcs_run_corner_to_corner() {
	let size @ (w, h) = (10, 20);
	let rising = glyph('╱', size);
	assert!(rising[w - 1] > 0 && rising[(h - 1) * w] > 0 && rising[0] == 0);
	let arc = glyph('╭', size);
	assert_eq!(arc[0], 0, "the outer corner stays empty");
}

/// Draws `input` into a 3×2 grid of 10×20 cells with a font too small to
/// fill them, returning the pixels.
fn grid_pixels(input: &str) -> Vec<u32> {
	use crate::{
		parser::Parser,
		render::{glyphs::FontSet, grid::GridView, painter::Painter},
		term::{Frame, Term},
		theme::{Theme, ThemeKind},
	};
	let mut term = Term::new(3, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(
		&mut term,
		format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m\x1b[2J\x1b[H{input}").as_bytes(),
	);
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.cursor = None;
	let (width, height) = (30, 40);
	let mut pixels = vec![0; width * height];
	let mut painter = Painter::new(&mut pixels, width, height);
	let view = GridView {
		frame: &frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 12.0,
		ascent: 14,
		scale: 1,
		default_bg: 0,
		link_color: 0,
		selection_colors: (0, 0),
		match_colors: (0, 0),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
		dim: None,
	};
	let font = crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	let mut fonts = FontSet::new(font, Vec::new());
	crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, None);
	pixels
}

#[test]
fn the_grid_draws_lines_that_join_across_cells_and_rows() {
	let pixels = grid_pixels("─┬─\r\n │");
	let (w, h) = (30, 40);
	let line_row = (0..20)
		.find(|&y| pixels[y * w] != 0)
		.expect("a horizontal line");
	assert!(
		(0..w).all(|x| pixels[line_row * w + x] == 0xffffff),
		"unbroken across cells"
	);
	let stem = (10..20)
		.find(|&x| pixels[(h - 1) * w + x] != 0)
		.expect("a vertical line");
	assert!(
		(line_row..h).all(|y| pixels[y * w + stem] == 0xffffff),
		"unbroken down rows"
	);
}

/// Paints a compact glyph back into a cell-sized bitmap.
fn expand(drawn: &Drawn, (w, h): (usize, usize)) -> Vec<u8> {
	let mut out = vec![0; w * h];
	match drawn {
		Drawn::Rects(rects) => {
			for &(x, y, rw, rh) in rects {
				for row in y..y + rh {
					out[row * w + x..row * w + x + rw].fill(255);
				}
			}
		}
		Drawn::Bitmap { left, top, glyph } => {
			let gw = glyph.metrics.width;
			for (i, &c) in glyph.coverage.iter().enumerate() {
				out[(top + i / gw) * w + left + i % gw] = c;
			}
		}
	}
	out
}

#[test]
fn compact_glyphs_draw_the_same_pixels_in_less_work() {
	for size in SIZES {
		for ch in (0x2500..=0x28ff)
			.chain(0xe0b0..=0xe0bf)
			.filter_map(char::from_u32)
			.filter(|&c| covers(c))
		{
			let full = glyph(ch, size);
			let drawn = compact(&full, size);
			assert_eq!(expand(&drawn, size), full, "{ch:?} at {size:?}");
		}
		// Solid lines and blocks become a few rectangles.
		for (ch, most) in [('─', 1), ('═', 2), ('┼', 3), ('╬', 8), ('█', 1), ('▚', 2)] {
			match compact(&glyph(ch, size), size) {
				Drawn::Rects(rects) => assert!(rects.len() <= most, "{ch:?}: {rects:?}"),
				Drawn::Bitmap { .. } => panic!("{ch:?} is solid"),
			}
		}
		// Antialiased ones keep only the rows and columns with ink.
		match compact(&glyph('\u{2801}', size), size) {
			Drawn::Bitmap { glyph, .. } => assert!(glyph.coverage.len() * 4 < size.0 * size.1),
			Drawn::Rects(_) => panic!("Braille dots are round"),
		}
	}
}
