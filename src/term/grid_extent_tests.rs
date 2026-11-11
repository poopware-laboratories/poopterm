//! Tests for each grid row's used extent: the cells after it are padding.
use super::{Cell, Grid, Term};
use crate::{
	bench::corpus::{Size, WORKLOADS},
	parser::Parser,
	theme::{Theme, ThemeKind},
};

const PAD: Cell = Cell {
	ch: ' ',
	fg: 7,
	bg: 0,
	attrs: 0,
	width: 1,
	extra: super::NO_EXTRA,
};
/// A blank in another background: not padding.
const COLORED: Cell = Cell { bg: 4, ..PAD };

fn letter(ch: char) -> Cell {
	Cell { ch, ..PAD }
}

fn used(grid: &Grid, rows: usize) -> Vec<usize> {
	(0..rows).map(|y| grid.used(y)).collect()
}

fn text(grid: &Grid, y: usize) -> String {
	grid[y].iter().map(|cell| cell.ch).collect()
}

/// A 6 × 3 grid with `row` written into every row.
fn written(row: &str) -> Grid {
	let mut grid = Grid::new(6, 3, PAD);
	for y in 0..3 {
		for (x, ch) in row.chars().enumerate() {
			grid.row_upto(y, x + 1)[x] = letter(ch);
		}
	}
	grid
}

#[test]
fn new_rows_are_unused_until_written() {
	let mut grid = Grid::new(6, 2, PAD);
	assert_eq!(used(&grid, 2), [0, 0]);
	grid.row_upto(1, 3)[2] = letter('x');
	assert_eq!(used(&grid, 2), [0, 3]);
	grid.assert_extents();
}

#[test]
fn marking_only_raises_and_stops_at_the_last_column() {
	let mut grid = Grid::new(6, 1, PAD);
	grid.mark_used(0, 4);
	grid.mark_used(0, 2);
	assert_eq!(grid.used(0), 4);
	grid.mark_used(0, 99);
	assert_eq!(grid.used(0), 6);
}

#[test]
fn writes_through_index_or_iteration_count_whole_rows() {
	let mut grid = Grid::new(6, 3, PAD);
	grid[1][0] = letter('a');
	assert_eq!(used(&grid, 3), [0, 6, 0]);
	grid.iter_mut().next();
	assert_eq!(used(&grid, 3), [6, 6, 6]);
}

#[test]
fn erasing_padding_to_the_end_shortens_the_row() {
	let mut grid = written("abcd");
	grid.fill(0, 2, 6, PAD);
	assert_eq!((text(&grid, 0).as_str(), grid.used(0)), ("ab    ", 2));
	// Ending exactly at the used end also counts as erasing to the end.
	grid.fill(1, 1, 4, PAD);
	assert_eq!((text(&grid, 1).as_str(), grid.used(1)), ("a     ", 1));
	grid.assert_extents();
}

#[test]
fn erasing_padding_inside_the_row_keeps_its_end() {
	let mut grid = written("abcd");
	grid.fill(0, 1, 3, PAD);
	assert_eq!((text(&grid, 0).as_str(), grid.used(0)), ("a  d  ", 4));
}

#[test]
fn erasing_unused_cells_writes_nothing() {
	let mut grid = written("ab");
	// Breaks the invariant on purpose: a fill of padding past the used end
	// must not touch the cells there.
	grid.row_upto(0, 0)[4] = letter('z');
	grid.fill(0, 3, 6, PAD);
	assert_eq!((text(&grid, 0).as_str(), grid.used(0)), ("ab  z ", 2));
}

#[test]
fn colored_erases_extend_the_row() {
	let mut grid = written("ab");
	grid.fill(0, 4, 5, COLORED);
	assert_eq!(grid.used(0), 5);
	grid.fill(1, 0, 1, COLORED);
	assert_eq!(grid.used(1), 2);
	grid.assert_extents();
}

#[test]
fn a_new_padding_cell_counts_every_row_as_used() {
	let mut grid = written("ab");
	grid.sync_pad(PAD);
	assert_eq!(used(&grid, 3), [2, 2, 2]);
	grid.sync_pad(COLORED);
	assert_eq!(used(&grid, 3), [6, 6, 6]);
	grid.assert_extents();
	// Measured against the new padding, erasing to the end shortens again.
	grid.fill(0, 1, 6, COLORED);
	assert_eq!(grid.used(0), 1);
}

#[test]
fn scrolling_up_hands_out_and_moves_the_extents() {
	let mut grid = Grid::new(6, 4, PAD);
	for (y, end) in [1, 2, 3, 4].into_iter().enumerate() {
		grid.mark_used(y, end);
	}
	let mut seen = Vec::new();
	grid.scroll_up(0, 3, 2, PAD, |row, used| {
		seen.push(used);
		row.len()
	});
	assert_eq!(seen, [1, 2]);
	assert_eq!(used(&grid, 4), [3, 4, 0, 0]);
	// Rows blanked with anything but padding are fully used.
	grid.scroll_up(1, 3, 1, COLORED, |row, _| row.len());
	assert_eq!(used(&grid, 4), [3, 0, 0, 6]);
	grid.assert_extents();
}

#[test]
fn scrolling_down_moves_the_extents() {
	let mut grid = Grid::new(6, 4, PAD);
	for (y, end) in [1, 2, 3, 4].into_iter().enumerate() {
		grid.mark_used(y, end);
	}
	grid.scroll_down(0, 3, 1, PAD);
	assert_eq!(used(&grid, 4), [0, 1, 2, 3]);
	grid.scroll_down(1, 2, 1, COLORED);
	assert_eq!(used(&grid, 4), [0, 6, 1, 3]);
	grid.assert_extents();
}

#[test]
fn resizing_counts_every_row_as_used() {
	let mut grid = written("ab");
	grid.resize(4, 5, PAD);
	assert_eq!(used(&grid, 5), [4; 5]);
}

/// Random fills, writes and scrolls keep the same cells as plain rows, and
/// the extents stay true.
#[test]
fn random_edits_match_plain_rows() {
	let (cols, rows) = (7, 5);
	let cells = [PAD, COLORED, letter('a'), letter('b')];
	for seed in 1..=200u64 {
		let mut state = seed;
		let mut next = |n: usize| {
			state ^= state << 13;
			state ^= state >> 7;
			state ^= state << 17;
			(state % n as u64) as usize
		};
		let mut grid = Grid::new(cols, rows, PAD);
		let mut model = vec![vec![PAD; cols]; rows];
		for step in 0..60 {
			let (y, cell) = (next(rows), cells[next(cells.len())]);
			let (a, b) = (next(cols + 1), next(cols + 1));
			let (start, stop) = (a.min(b), a.max(b));
			match next(4) {
				0 => {
					grid.fill(y, start, stop, cell);
					model[y][start..stop].fill(cell);
				}
				1 if start < cols => {
					grid.row_upto(y, start + 1)[start] = cell;
					model[y][start] = cell;
				}
				2 => {
					let (top, bottom) = (y.min(start % rows), y.max(start % rows));
					let count = 1 + next(bottom + 1 - top);
					let blank = cells[next(2)];
					grid.scroll_up(top, bottom, count, blank, |row, _| row.len());
					model[top..=bottom].rotate_left(count);
					model[bottom + 1 - count..=bottom]
						.iter_mut()
						.for_each(|row| row.fill(blank));
				}
				_ => {
					let pad = cells[next(2)];
					grid.sync_pad(pad);
				}
			}
			grid.assert_extents();
			for (y, row) in model.iter().enumerate() {
				assert_eq!(&grid[y], row, "seed {seed}, step {step}, row {y}");
			}
		}
	}
}

/// The benchmark corpora exercise every printing and erasing path; after
/// each burst, every row must still be padding past its used end.
#[test]
fn bench_corpora_keep_extents_true() {
	let size = Size { cols: 80, rows: 24 };
	for workload in WORKLOADS
		.iter()
		.filter(|workload| !workload.name.starts_with("kitty"))
	{
		let mut term = Term::new(
			size.cols,
			size.rows,
			Theme::from_kind(ThemeKind::Poop),
			"/tmp".into(),
		);
		let mut parser = Parser::new();
		for chunk in workload.build(64 << 10, size).chunks(4096) {
			parser.advance(&mut term, chunk);
			term.grid.assert_extents();
		}
	}
}

/// Lines moved to the history keep what the screen showed.
#[test]
fn history_lines_match_the_screen_after_erases() {
	let mut term = Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
	let mut parser = Parser::new();
	parser.advance(
		&mut term,
		b"abcdef\x1b[4G\x1b[K\r\n\x1b[41mxy\x1b[K\x1b[m\r\n\r\n",
	);
	let lines: Vec<String> = term
		.scrollback
		.iter()
		.map(|row| row.iter().map(|c| c.ch).collect())
		.collect();
	assert_eq!(lines, ["abc", "xy      "]);
}

fn fed(cols: usize, bytes: &[u8]) -> Term {
	let mut term = Term::new(cols, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
	Parser::new().advance(&mut term, bytes);
	term.grid.assert_extents();
	term
}

#[test]
fn erasing_to_the_end_of_line_shortens_the_row() {
	assert_eq!(fed(8, b"abcdef\x1b[3G\x1b[K").grid.used(0), 2);
	// Erased in another background, the cells stay in the row.
	assert_eq!(fed(8, b"abcdef\x1b[3G\x1b[44m\x1b[K").grid.used(0), 8);
}

#[test]
fn rows_follow_new_default_colors() {
	let osc = b"\x1b]11;#102030\x1b\\";
	assert_eq!(
		fed(8, &[osc, &b"abcdef\x1b[3G\x1b[K"[..]].concat())
			.grid
			.used(0),
		2
	);
	// The saved primary screen follows too.
	let back = [&b"\x1b[?1049h"[..], osc, b"\x1b[?1049labcdef\x1b[3G\x1b[K"].concat();
	assert_eq!(fed(8, &back).grid.used(0), 2);
}

#[test]
fn the_alternate_screen_pads_with_default_colors() {
	let term = fed(8, b"\x1b[44m\x1b[?1049h\x1b[mabcdef\x1b[3G\x1b[K");
	assert_eq!(term.grid.used(0), 2);
	assert_eq!(term.grid.used(1), 8, "blanked in blue");
}

#[test]
fn a_broken_wide_half_at_the_used_end_is_repaired() {
	let mut term = fed(8, b"ab");
	// A left half without its right half, in the last used cell.
	term.grid.row_upto(0, 0)[1].width = 2;
	term.repair_wide_rows(0, 1);
	assert_eq!((term.grid[0][1].ch, term.grid[0][1].width), (' ', 1));
}

/// History lines of a two-row terminal fed `bytes`, then scrolled once.
fn history(cols: usize, bytes: &[u8]) -> Vec<String> {
	let term = fed(cols, &[bytes, b"\r\n\r\n"].concat());
	term
		.scrollback
		.iter()
		.map(|row| row.iter().map(|c| c.ch).collect())
		.collect()
}

#[test]
fn shifted_rows_keep_their_text_in_history() {
	assert_eq!(history(8, b"abcdef\x1b[1G\x1b[2P"), ["cdef"]);
	assert_eq!(history(8, b"abcdef\x1b[3G\x1b[2@"), ["ab  cdef"]);
	assert_eq!(history(8, b"abcdef\x1b[2G\x1b[3X"), ["a   ef"]);
}

#[test]
fn the_primary_screen_keeps_its_extents_across_the_alternate_screen() {
	let lines = history(8, b"abcdef\x1b[?1049hxyz\x1b[K\x1b[?1049l\x1b[4G\x1b[K");
	assert_eq!(lines, ["abc"]);
}

#[test]
fn a_mark_joined_to_padding_uses_that_cell() {
	let term = fed(8, "ab\x1b[5G\u{301}".as_bytes());
	assert_eq!(term.grid.used(0), 4);
	assert_eq!(term.marks(&term.grid[0][3]), ['\u{301}']);
}

#[test]
fn a_wrapped_wide_character_uses_both_cells_of_the_next_row() {
	let term = fed(8, "\x1b[8G猫".as_bytes());
	assert_eq!((term.grid.used(0), term.grid.used(1)), (0, 2));
	assert_eq!(term.grid[1][1].width, 0);
}

#[test]
fn single_line_scrolls_inside_a_region_keep_its_rows_in_order() {
	// A region narrower than the screen rotates its rows instead of moving
	// the ring's first row.
	let mut t = Term::new(4, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut t, b"top\x1b[4;1Hbot\x1b[2;3r\x1b[3;1H");
	for n in 0..200 {
		Parser::new().advance(&mut t, format!("\r\n{n:03}").as_bytes());
	}
	let screen: Vec<String> = (0..4).map(|y| text(&t.grid, y)).collect();
	assert_eq!(screen, ["top ", "198 ", "199 ", "bot "]);
	assert_eq!(used(&t.grid, 4), [3, 3, 3, 3]);
}
