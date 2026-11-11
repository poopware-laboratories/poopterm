//! kitty's text sizing protocol (OSC 66): text drawn across blocks of
//! cells, and how other controls treat those blocks.
use super::multicell::Multicell;
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The block cell at column `x`, row `y`, if any.
fn part(term: &Term, x: usize, y: usize) -> Option<Multicell> {
	term.extras.multicell(term.grid[y][x].extra).cloned()
}

/// Text, (scale, width), offset in the block.
type Summary = (String, (u8, u8), (u8, u8));

fn summary(term: &Term, x: usize, y: usize) -> Option<Summary> {
	part(term, x, y).map(|m| {
		(
			m.text.to_string(),
			(m.size.scale, m.size.width),
			(m.dx, m.dy),
		)
	})
}

fn row_text(term: &Term, y: usize) -> String {
	term.grid[y].iter().map(|cell| cell.ch).collect()
}

fn assert_plain(term: &Term) {
	for y in 0..term.rows {
		for x in 0..term.cols {
			assert_eq!(part(term, x, y), None, "({x}, {y})");
		}
	}
}

#[test]
fn scaled_text_takes_blocks_of_cells() {
	let mut t = terminal(10, 4);
	send(&mut t, "\x1b]66;s=2;ab\x07");
	let a = |dx, dy| Some(("a".to_owned(), (2, 1), (dx, dy)));
	assert_eq!(summary(&t, 0, 0), a(0, 0));
	assert_eq!(summary(&t, 1, 0), a(1, 0));
	assert_eq!(summary(&t, 0, 1), a(0, 1));
	assert_eq!(summary(&t, 1, 1), a(1, 1));
	assert_eq!(summary(&t, 2, 0), Some(("b".to_owned(), (2, 1), (0, 0))));
	assert_eq!(summary(&t, 3, 1), Some(("b".to_owned(), (2, 1), (1, 1))));
	assert_eq!(part(&t, 4, 0), None);
	// The first cell holds the character, so copies and searches see it.
	assert!(row_text(&t, 0).starts_with("a b "));
	// The cursor moves right on the block's first row.
	assert_eq!((t.cx, t.cy), (4, 0));
	// Wide characters take blocks twice as wide.
	send(&mut t, "\x1b]66;s=2;🐈\x07");
	assert_eq!(summary(&t, 7, 1), Some(("🐈".to_owned(), (2, 2), (3, 1))));
	assert_eq!((t.cx, t.cy), (8, 0));
}

#[test]
fn an_explicit_width_holds_all_the_text() {
	let mut t = terminal(10, 3);
	send(&mut t, "\x1b]66;n=1:d=2:w=1;ab\x1b\\");
	let m = part(&t, 0, 0).unwrap();
	assert_eq!(&*m.text, "ab");
	let size = m.size;
	assert_eq!(
		(size.scale, size.width, size.numerator, size.denominator),
		(1, 1, 1, 2)
	);
	assert_eq!(part(&t, 1, 0), None);
	assert_eq!((t.cx, t.cy), (1, 0));
	send(&mut t, "\x1b]66;w=2:v=1:h=2;🐈\x07");
	let m = part(&t, 2, 0).unwrap();
	assert_eq!((m.size.vertical, m.size.horizontal, m.dx), (1, 2, 1));
}

#[test]
fn support_is_detected_with_cursor_reports() {
	let mut t = terminal(20, 5);
	send(
		&mut t,
		"\r\x1b[6n\x1b]66;w=2; \x07\x1b[6n\x1b]66;s=2; \x07\x1b[6n",
	);
	assert_eq!(
		t.responses,
		[
			b"\x1b[1;1R".to_vec(),
			b"\x1b[1;3R".to_vec(),
			b"\x1b[1;5R".to_vec()
		]
	);
}

#[test]
fn plain_text_stays_plain() {
	let mut t = terminal(10, 2);
	send(&mut t, "\x1b]66;;hi\x07\x1b]66;s=1;yo\x07");
	assert!(row_text(&t, 0).starts_with("hiyo"));
	assert_plain(&t);
	assert!(!t.has_multicells);
}

#[test]
fn bad_metadata_and_oversized_blocks_are_dropped() {
	let mut t = terminal(10, 4);
	for bad in [
		"s=0", "s=8", "w=8", "n=16", "d=16", "n=2:d=2", "n=3:d=2", "v=3", "h=3", "s=x", "s=5",
		"w=6:s=2",
	] {
		send(&mut t, &format!("\x1b]66;{bad};x\x07"));
		assert_eq!((t.cx, t.cy), (0, 0), "{bad}");
	}
	send(&mut t, &format!("\x1b]66;s=2;{}\x07", "x".repeat(4097)));
	assert_plain(&t);
	assert_eq!((t.cx, t.cy), (0, 0));
	// Controls are not text.
	send(&mut t, "\x1b]66;w=1;a\u{9b}b\x07");
	assert_eq!(&*part(&t, 0, 0).unwrap().text, "ab");
}

#[test]
fn blocks_wrap_or_stop_at_the_margin() {
	let mut t = terminal(10, 4);
	send(&mut t, "123456789\x1b]66;s=2;x\x07");
	assert_eq!(summary(&t, 0, 1).unwrap().0, "x");
	assert_eq!((t.cx, t.cy), (2, 1));
	let mut t = terminal(10, 4);
	send(&mut t, "\x1b[?7l123456789\x1b]66;s=2;x\x07");
	assert_eq!(summary(&t, 8, 0).unwrap().2, (0, 0));
	assert_eq!((t.cx, t.cy, t.wrap_pending), (9, 0, true));
}

#[test]
fn blocks_at_the_bottom_scroll_the_screen() {
	let mut t = terminal(10, 4);
	send(&mut t, "top\r\n\r\n\r\n\x1b]66;s=3;x\x07");
	assert_eq!(summary(&t, 0, 1).unwrap().2, (0, 0));
	assert_eq!(summary(&t, 2, 3).unwrap().2, (2, 2));
	assert_eq!((t.cx, t.cy), (3, 1));
	assert_eq!(t.scrollback.len(), 2);
}

#[test]
fn writing_over_the_top_row_erases_the_block() {
	let mut t = terminal(10, 3);
	send(&mut t, "\x1b]66;s=2;ab\x07\x1b[1;2Hx");
	assert!(row_text(&t, 0).starts_with(" xb "));
	assert_eq!(part(&t, 0, 0), None);
	assert_eq!(part(&t, 1, 1), None);
	assert_eq!(summary(&t, 2, 1).unwrap().0, "b");
}

#[test]
fn writing_below_the_top_row_skips_the_block() {
	let mut t = terminal(10, 3);
	send(&mut t, "\x1b]66;s=2;ab\x07\x1b[2;2Hx");
	// Blocks next to each other are all skipped.
	assert_eq!(summary(&t, 1, 1).unwrap().0, "a");
	assert_eq!(summary(&t, 3, 1).unwrap().0, "b");
	assert!(row_text(&t, 1).starts_with("    x"));
	assert_eq!((t.cx, t.cy), (5, 1));
	// At the margin the text wraps after skipping.
	send(&mut t, "\x1b[1;9H\x1b]66;s=2;c\x07\x1b[2;9Hy");
	assert_eq!(summary(&t, 9, 1).unwrap().0, "c");
	assert!(row_text(&t, 2).starts_with('y'));
}

#[test]
fn erasing_any_cell_erases_the_whole_block() {
	for erase in [
		"\x1b[2;2H\x1b[X",
		"\x1b[2;2H\x1b[K",
		"\x1b[2;3H\x1b[1K",
		"\x1b[2;1H\x1b[J",
	] {
		let mut t = terminal(10, 3);
		send(&mut t, "\x1b]66;s=2;a\x07z");
		send(&mut t, erase);
		assert_eq!(part(&t, 0, 0), None, "{erase:?}");
		assert_eq!(part(&t, 1, 1), None, "{erase:?}");
		assert_eq!(t.grid[0][0].ch, ' ', "{erase:?}");
	}
}

#[test]
fn inserting_and_deleting_split_blocks_are_erased() {
	// Characters: a two-row block intersecting the shifted cells goes.
	let mut t = terminal(10, 3);
	send(&mut t, "\x1b]66;s=2;a\x07\x1b[1;1H\x1b[@");
	assert_plain(&t);
	// One-row blocks moved whole survive; split ones go.
	let mut t = terminal(10, 3);
	send(&mut t, "x\x1b]66;w=2;ab\x07\x1b[1;1H\x1b[P");
	assert_eq!(summary(&t, 1, 0).unwrap().2, (1, 0));
	send(&mut t, "\x1b[1;2H\x1b[P");
	assert_plain(&t);
	// Lines: a block split by an inserted line goes, and so does one pushed
	// half off the bottom.
	let mut t = terminal(10, 4);
	send(&mut t, "\r\n\x1b]66;s=2;a\x07\x1b[3;1H\x1b[L");
	assert_plain(&t);
	let mut t = terminal(10, 4);
	send(&mut t, "\r\n\r\n\x1b]66;s=2;a\x07\x1b[1;1H\x1b[L");
	assert_plain(&t);
	// A block above inserted lines moves down whole.
	let mut t = terminal(10, 5);
	send(&mut t, "\r\n\x1b]66;s=2;a\x07\x1b[1;1H\x1b[L");
	assert_eq!(summary(&t, 1, 3).unwrap().2, (1, 1));
	// Deleting any of its lines erases it.
	send(&mut t, "\x1b[4;1H\x1b[M");
	assert_plain(&t);
}

#[test]
fn combining_marks_join_the_block() {
	let mut t = terminal(10, 3);
	send(&mut t, "\x1b]66;s=2;e\x07\u{301}");
	assert_eq!(&*part(&t, 1, 1).unwrap().text, "e\u{301}");
	assert_eq!((t.cx, t.cy), (2, 0));
}

#[test]
fn blocks_take_the_width_of_their_text() {
	let mut t = terminal(14, 3);
	send(&mut t, "\x1b]66;s=2;猫e\u{301}\u{1100}\u{1161}\x07");
	let blocks = [(0, "猫", 2), (4, "e\u{301}", 1), (6, "\u{1100}\u{1161}", 2)];
	for (x, text, width) in blocks {
		assert_eq!(
			summary(&t, x, 0),
			Some((text.into(), (2, width), (0, 0))),
			"{text:?}"
		);
	}
	assert_eq!((t.cx, t.cy), (10, 0));
}

#[test]
fn plain_sized_text_prints_wide_and_malformed_characters_as_text_does() {
	let mut t = terminal(10, 2);
	// A wide character, a stray byte and a control inside the payload.
	Parser::new().advance(&mut t, b"\x1b]66;;\xe7\x8c\xab\xff\x01e\x07");
	let cells: Vec<(char, u8)> = t.grid[0][..4]
		.iter()
		.map(|cell| (cell.ch, cell.width))
		.collect();
	assert_eq!(cells[0], ('\u{732b}', 2));
	assert_eq!(cells[1].1, 0, "the wide character's right half");
	assert_eq!(cells[2..], [('\u{fffd}', 1), ('e', 1)]);
	assert_eq!((t.cx, t.cy), (4, 0));
	assert_plain(&t);
}
