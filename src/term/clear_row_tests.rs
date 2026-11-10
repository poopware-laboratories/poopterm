//! `Term::clear_row` against `Term::clear_range` over the same cells, and
//! the row-by-row multicell cleanup of multi-row erasures.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

const COLS: usize = 12;
/// One row below the image, so the cursor moving under it does not scroll.
const ROWS: usize = 5;

/// A screen holding everything an erasure must handle: colored cells, wide
/// characters, OSC 66 blocks over two rows, and a Sixel image over columns
/// 6..8 of row 3.
fn busy() -> Term {
	let mut term = Term::new(COLS, ROWS, Theme::from_kind(ThemeKind::Poop), "/".into());
	(term.cell_width, term.cell_height) = (10, 6);
	let setup = concat!(
		"\x1b[41mab界cd\x1b[44m世ef\x1b[m",
		"\x1b[2;3H\x1b]66;s=2;xy\x07\x1b[2;9H漢",
		"\x1b[4;1H\x1b[42m0123\x1b[4;7H\x1bP7;1q!20~\x1b\\",
		// Erasures blank with this background.
		"\x1b[45m",
	);
	Parser::new().advance(&mut term, setup.as_bytes());
	term
}

/// Everything an erasure may change: cells, and where images are.
fn state(term: &Term) -> (Vec<Vec<crate::term::Cell>>, Vec<(isize, usize)>) {
	let cells = (0..ROWS).map(|y| term.grid[y].to_vec()).collect();
	let images = term
		.images
		.iter()
		.map(|image| (image.row, image.col))
		.collect();
	(cells, images)
}

#[test]
fn the_setup_has_blocks_wide_cells_and_an_image() {
	let term = busy();
	assert!(term.has_multicells);
	assert_eq!(state(&term).1, [(3, 6)]);
	assert_eq!(term.grid[0][0].ch, 'a', "nothing scrolled");
	assert_eq!(term.images.len(), 1);
	assert_eq!(term.grid[0][2].width, 2);
	assert_eq!(term.grid[1][9].width, 0);
}

#[test]
fn clearing_part_of_a_row_matches_clearing_the_same_flat_range() {
	for y in 0..ROWS {
		for from in 0..=COLS {
			for to in from..=COLS + 2 {
				let (mut by_row, mut by_range) = (busy(), busy());
				by_row.clear_row(y, from, to);
				let end = y * COLS + to.min(COLS);
				by_range.clear_range(y * COLS + from, end);
				assert_eq!(state(&by_row), state(&by_range), "row {y}, {from}..{to}");
			}
		}
	}
}

#[test]
fn an_empty_or_reversed_span_changes_nothing() {
	let before = state(&busy());
	for (from, to) in [(0, 0), (5, 5), (7, 3), (COLS, COLS + 4)] {
		let mut term = busy();
		term.clear_row(1, from, to);
		assert_eq!(state(&term), before, "{from}..{to}");
	}
}

#[test]
fn a_row_below_the_screen_changes_nothing() {
	let mut term = busy();
	let before = state(&term);
	term.clear_row(ROWS, 0, COLS);
	term.clear_row(usize::MAX / 2, 0, COLS);
	assert_eq!(state(&term), before);
}

#[test]
fn only_columns_under_the_image_remove_it() {
	// The image covers columns 6 and 7 of row 3.
	for (from, to, kept) in [(0, 6, true), (8, COLS, true), (5, 7, false), (7, 8, false)] {
		let mut term = busy();
		term.clear_row(3, from, to);
		assert_eq!(term.images.len() == 1, kept, "{from}..{to}");
	}
	let mut term = busy();
	term.clear_row(2, 0, COLS);
	assert_eq!(term.images.len(), 1, "another row leaves it");
}

#[test]
fn cleared_cells_take_the_current_background_and_others_stay() {
	let mut term = busy();
	let kept = term.grid[3][0];
	term.clear_row(3, 1, 4);
	let blank = term.blank();
	assert!(term.grid[3][1..4].iter().all(|cell| *cell == blank));
	assert_eq!(term.grid[3][0], kept);
	assert_eq!(term.grid[3][4].ch, ' ');
}

#[test]
fn erasing_below_removes_blocks_with_any_cell_in_the_erased_rows() {
	// The block covers columns 2..6 of rows 1 and 2. ED 0 from row 3 misses
	// it; from row 1 column 6 it misses its top row but clears its bottom.
	let mut term = busy();
	Parser::new().advance(&mut term, b"\x1b[4;1H\x1b[J");
	assert!(term.extras.multicell(term.grid[1][2].extra).is_some());
	assert!(term.images.is_empty(), "the image on row 3 goes");
	let mut term = busy();
	Parser::new().advance(&mut term, b"\x1b[2;7H\x1b[J");
	assert!(term.extras.multicell(term.grid[1][2].extra).is_none());
	assert!(term.extras.multicell(term.grid[2][3].extra).is_none());
	assert_eq!(term.grid[0][0].ch, 'a', "rows above the cursor stay");
}

#[test]
fn erasing_above_removes_a_block_reaching_into_the_erased_rows() {
	// ED 1 at row 2 column 1 covers the block's second row.
	let mut term = busy();
	Parser::new().advance(&mut term, b"\x1b[3;1H\x1b[1J");
	assert!(term.extras.multicell(term.grid[1][2].extra).is_none());
	assert_eq!(term.grid[0][0].ch, ' ');
	assert_eq!(term.grid[3][0].ch, '0', "rows below the cursor stay");
}

#[test]
fn line_erasures_keep_repairing_wide_characters_they_cut() {
	// EL 0 from the right half of 界 and EL 1 to its left half.
	let mut term = busy();
	Parser::new().advance(&mut term, b"\x1b[1;4H\x1b[K");
	assert_eq!((term.grid[0][2].ch, term.grid[0][2].width), (' ', 1));
	let mut term = busy();
	Parser::new().advance(&mut term, b"\x1b[1;3H\x1b[1K");
	assert_eq!((term.grid[0][3].ch, term.grid[0][3].width), (' ', 1));
}
