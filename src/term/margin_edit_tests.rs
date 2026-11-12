//! Inserting and deleting inside the margins: ICH, DCH, IL and DL, the
//! column operations DECIC and DECDC, DECBI and DECFI, and SL and SR.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn feed(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The screen's rows, blanks as spaces.
fn screen(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|y| term.grid[y].iter().map(|cell| cell.ch).collect())
		.collect()
}

/// The cursor as `(column, row)`, counted from 1 like esctest's points.
fn cursor(term: &Term) -> (usize, usize) {
	(term.cx + 1, term.cy + 1)
}

/// A 5×5 screen of the letters `a` to `y`, as esctest prepares it.
fn letters() -> Term {
	let mut t = terminal(5, 5);
	feed(&mut t, "abcde\r\nfghij\r\nklmno\r\npqrst\r\nuvwxy\x1b[H");
	t
}

/// Two rows of letters in a 10×4 screen, with margins at columns 2 to 5.
fn two_rows() -> Term {
	let mut t = terminal(10, 4);
	feed(&mut t, "abcdefg\r\nABCDEFG\x1b[?69h\x1b[2;5s");
	t
}

#[test]
fn insert_characters_pushes_cells_off_the_right_margin() {
	// esctest ICH_ScrollOffRightMarginInScrollRegion.
	let mut t = two_rows();
	feed(&mut t, "\x1b[1;3H\x1b[@");
	assert_eq!(screen(&t)[0], "ab cdfg   ");
	feed(&mut t, "\x1b[1;3H\x1b[99@");
	assert_eq!(screen(&t)[0], "ab   fg   ");
	assert_eq!(screen(&t)[1], "ABCDEFG   ");
}

#[test]
fn delete_characters_pulls_cells_in_from_the_right_margin() {
	// esctest DCH_RespectsMargins and DCH_DeleteAllWithMargins.
	let mut t = terminal(5, 2);
	feed(&mut t, "abcde\x1b[?69h\x1b[2;4s\x1b[1;3H\x1b[P");
	assert_eq!(screen(&t)[0], "abd e");
	feed(&mut t, "\x1b[1;3H\x1b[99P");
	assert_eq!(screen(&t)[0], "ab  e");
}

#[test]
fn character_and_column_edits_outside_the_margins_do_nothing() {
	// esctest ICH_IsNoOpWhenCursorBeginsOutsideScrollRegion, DCH and DECIC
	// twins, left and right of the margins.
	for edit in ["\x1b[10@", "\x1b[10P", "\x1b[10'}", "\x1b[10'~"] {
		for column in [1, 7] {
			let mut t = two_rows();
			feed(&mut t, &format!("\x1b[1;{column}H{edit}"));
			assert_eq!(
				screen(&t)[..2],
				["abcdefg   ", "ABCDEFG   "],
				"{edit:?} at {column}"
			);
		}
	}
}

#[test]
fn delete_lines_moves_only_the_columns_between_the_margins() {
	// esctest DL_InLeftRightScrollRegion and its top and bottom twins.
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;3H\x1b[M");
	assert_eq!(screen(&t), ["abcde", "flmnj", "kqrso", "pvwxt", "u   y"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[2;3H\x1b[M");
	assert_eq!(screen(&t), ["abcde", "flmnj", "kqrso", "p   t", "uvwxy"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[2;3H\x1b[99M");
	assert_eq!(screen(&t), ["abcde", "f   j", "k   o", "p   t", "uvwxy"]);
	assert!(t.scrollback.is_empty());
}

#[test]
fn insert_lines_moves_only_the_columns_between_the_margins() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;3H\x1b[L");
	assert_eq!(screen(&t), ["abcde", "f   j", "kghio", "plmnt", "uqrsy"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[3;2H\x1b[2L");
	assert_eq!(screen(&t), ["abcde", "fghij", "k   o", "p   t", "uvwxy"]);
}

#[test]
fn line_edits_outside_the_margins_do_nothing() {
	// esctest DL_OutsideLeftRightScrollRegion and
	// DL_OutsideLeftRightAndTopBottomScrollRegion.
	for edit in ["\x1b[M", "\x1b[L"] {
		for (setup, at) in [
			("", "\x1b[2;1H"),
			("", "\x1b[2;5H"),
			("\x1b[2;4r", "\x1b[1;3H"),
		] {
			let mut t = letters();
			feed(&mut t, &format!("\x1b[?69h\x1b[2;4s{setup}{at}{edit}"));
			assert_eq!(
				screen(&t),
				["abcde", "fghij", "klmno", "pqrst", "uvwxy"],
				"{edit:?} {at:?}"
			);
		}
	}
}

#[test]
fn insert_column_shifts_every_row_of_the_region() {
	// esctest DECIC_DefaultParam, ExplicitParam and
	// ScrollOffRightMarginInScrollRegion.
	let mut t = terminal(10, 3);
	feed(&mut t, "abcdefg\r\nABCDEFG\r\nzyxwvut\x1b[2;2H\x1b['}");
	assert_eq!(screen(&t), ["a bcdefg  ", "A BCDEFG  ", "z yxwvut  "]);
	feed(&mut t, "\x1b[3;2H\x1b[2'}");
	assert_eq!(screen(&t), ["a   bcdefg", "A   BCDEFG", "z   yxwvut"]);
	let mut t = two_rows();
	feed(&mut t, "\x1b[1;3H\x1b['}");
	assert_eq!(screen(&t)[..2], ["ab cdfg   ", "AB CDFG   "]);
}

#[test]
fn delete_column_shifts_every_row_of_the_region() {
	// esctest DECDC_DeleteWithLeftRightMargins.
	let mut t = two_rows();
	feed(&mut t, "\x1b[1;3H\x1b['~");
	assert_eq!(screen(&t)[..2], ["abde fg   ", "ABDE FG   "]);
	let mut t = terminal(8, 2);
	feed(&mut t, "abcdefgh\r\nABCDEFGH\x1b[1;2H\x1b[3'~");
	assert_eq!(screen(&t), ["aefgh   ", "AEFGH   "]);
}

#[test]
fn column_edits_touch_only_rows_inside_the_top_and_bottom_margins() {
	// esctest DECIC_CursorWithinTopBottom, and nothing from outside them.
	let mut t = terminal(10, 4);
	feed(
		&mut t,
		"abcdefg\r\nABCDEFG\r\nzyxwvut\r\nZYXWVUT\x1b[2;3r\x1b[2;2H\x1b[2'}",
	);
	assert_eq!(
		screen(&t),
		["abcdefg   ", "A  BCDEFG ", "z  yxwvut ", "ZYXWVUT   "]
	);
	feed(&mut t, "\x1b[4;1H\x1b[2'~\x1b[1;1H\x1b['}");
	assert_eq!(screen(&t)[0], "abcdefg   ");
	assert_eq!(screen(&t)[3], "ZYXWVUT   ");
}

#[test]
fn back_index_moves_left_or_scrolls_the_region_right() {
	// esctest DECBI_Basic, NoWrapOnLeftEdge, LeftOfMargin and
	// WholeScreenScrolls.
	let mut t = terminal(10, 8);
	feed(&mut t, "\x1b[6;5H\x1b6");
	assert_eq!(cursor(&t), (4, 6));
	feed(&mut t, "\x1b[1;1Hx\x1b[1;1H\x1b6");
	assert_eq!(cursor(&t), (1, 1));
	assert_eq!(&screen(&t)[0][..2], " x");
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[1;2H\x1b6");
	assert_eq!(cursor(&t), (1, 1));
	// Left of the margins at the screen's edge, nothing happens.
	feed(&mut t, "\x1b6");
	assert_eq!((cursor(&t), &screen(&t)[0][..2]), ((1, 1), " x"));
}

#[test]
fn back_index_at_the_left_margin_scrolls_the_rectangle() {
	// esctest DECBI_Scrolls.
	let mut t = terminal(8, 8);
	for (y, row) in ["abcde", "fghij", "klmno", "pqrst", "uvwxy"]
		.iter()
		.enumerate()
	{
		feed(&mut t, &format!("\x1b[{};2H{row}", y + 3));
	}
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[4;6r\x1b[5;3H\x1b6");
	let rows: Vec<String> = screen(&t)[2..7]
		.iter()
		.map(|row| row[1..6].to_owned())
		.collect();
	assert_eq!(rows, ["abcde", "f ghj", "k lmo", "p qrt", "uvwxy"]);
	assert_eq!(cursor(&t), (3, 5));
}

#[test]
fn forward_index_moves_right_or_scrolls_the_region_left() {
	// esctest DECFI_RightOfMargin and WholeScreenScrolls.
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[2;5H\x1b9");
	assert_eq!(cursor(&t), (6, 2));
	feed(&mut t, "\x1b[1;10Hx\x1b9");
	assert_eq!(&screen(&t)[0][8..], "x ");
	assert_eq!(cursor(&t), (10, 1));
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[1;6H\x1b9");
	assert_eq!(cursor(&t), (7, 1));
}

#[test]
fn forward_index_at_the_right_margin_scrolls_the_rectangle() {
	let mut t = terminal(8, 8);
	for (y, row) in ["abcde", "fghij", "klmno", "pqrst", "uvwxy"]
		.iter()
		.enumerate()
	{
		feed(&mut t, &format!("\x1b[{};2H{row}", y + 3));
	}
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[4;6r\x1b[5;5H\x1b9");
	let rows: Vec<String> = screen(&t)[2..7]
		.iter()
		.map(|row| row[1..6].to_owned())
		.collect();
	assert_eq!(rows, ["abcde", "fhi j", "kmn o", "prs t", "uvwxy"]);
	assert_eq!(cursor(&t), (5, 5));
}

#[test]
fn scroll_left_and_right_shift_the_rectangle_wherever_the_cursor_is() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[5;5H\x1b[ @");
	assert_eq!(screen(&t), ["acd e", "fhi j", "kmn o", "prs t", "uwx y"]);
	assert_eq!(cursor(&t), (5, 5));
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[2 A");
	assert_eq!(screen(&t), ["abcde", "f  gj", "k  lo", "p  qt", "uvwxy"]);
	// Without margins they shift the whole screen; past its width, all of it.
	let mut t = letters();
	feed(&mut t, "\x1b[ @");
	assert_eq!(screen(&t)[0], "bcde ");
	feed(&mut t, "\x1b[99 A");
	assert_eq!(screen(&t), ["     "; 5]);
}

#[test]
fn shifting_columns_repairs_wide_characters_the_margins_cut() {
	// A wide character across the left margin loses its right half to DCH;
	// the left half, alone, becomes a blank.
	let mut t = terminal(8, 2);
	feed(&mut t, "a世bcd\x1b[?69h\x1b[3;6s\x1b[1;3H\x1b[P");
	assert_eq!((t.grid[0][1].ch, t.grid[0][1].width), (' ', 1));
	assert_eq!((t.grid[0][2].ch, t.grid[0][2].width), ('b', 1));
	assert_eq!(screen(&t)[0], "a bcd   ");
	// One across the right margin loses its left half to ICH, and its
	// right half, left behind outside the margins, becomes a blank.
	let mut t = terminal(8, 2);
	feed(&mut t, "abcde世\x1b[?69h\x1b[3;6s\x1b[1;3H\x1b[@");
	assert_eq!(screen(&t)[0], "ab cde  ");
	assert_eq!((t.grid[0][6].ch, t.grid[0][6].width), (' ', 1));
}

#[test]
fn shifting_columns_erases_sixel_images_inside_them() {
	let mut t = terminal(10, 4);
	(t.cell_width, t.cell_height) = (10, 20);
	let sixel = "\x1bPq#0;2;0;0;0~\x1b\\";
	feed(&mut t, &format!("\x1b[1;1H{sixel}\x1b[1;4H{sixel}"));
	feed(&mut t, "\x1b[?69h\x1b[3;6s\x1b[ @");
	let columns: Vec<usize> = t.images.iter().map(|image| image.col).collect();
	assert_eq!(columns, [0]);
}

#[test]
fn shifting_columns_into_short_rows_marks_them_used() {
	for background in ["", "\x1b[44m"] {
		let mut t = terminal(6, 3);
		feed(
			&mut t,
			&format!("ab\x1b[?69h\x1b[1;6s{background}\x1b[1;1H\x1b[3'}}"),
		);
		assert_eq!(screen(&t)[0], "   ab ", "{background:?}");
		t.grid.assert_extents();
	}
}
