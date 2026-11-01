//! Terminal behavior around the trimmed scrollback: what reaches history,
//! how short history rows are shown, searched and copied.
use super::{Cell, Frame, Term};
use crate::{
	parser::Parser,
	theme::{Theme, ThemeKind},
	ui::search::find_matches,
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/tmp".into())
}

fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}

#[test]
fn a_cloned_terminal_keeps_its_history_while_the_original_scrolls() {
	let mut t = terminal(4, 1);
	t.scrollback_limit = 1;
	feed(&mut t, b"old\r\n");
	let copy = t.clone();
	feed(&mut t, b"new\r\nlast\r\n");
	assert_eq!(lines(&copy), ["old"]);
	assert_eq!(lines(&t), ["last"]);
	assert_eq!(t.history_base, 2);
}

#[test]
fn trimmed_history_is_shown_padded_with_default_colors() {
	let mut t = terminal(4, 1);
	// The current background differs from the default when the view is drawn.
	feed(&mut t, b"ab\r\n\x1b[41mcd");
	t.scroll_view(1);
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	let shown: String = frame.row(0).iter().map(|cell| cell.ch).collect();
	assert_eq!(shown, "ab  ");
	assert_eq!(frame.row(0)[3], t.padding());
	assert_ne!(t.blank().bg, t.padding().bg);
}

#[test]
fn rows_with_colored_blanks_keep_them_in_history() {
	let mut t = terminal(4, 1);
	feed(&mut t, b"\x1b[44m\x1b[K\x1b[0mx\r\n");
	assert_eq!(t.scrollback[0].len(), 4);
	assert_eq!(t.scrollback[0][3].bg, t.scrollback[0][1].bg);
	assert_ne!(t.scrollback[0][3].bg, t.default_bg);
}

#[test]
fn history_stays_aligned_with_line_numbers_while_evicting() {
	let mut t = terminal(6, 2);
	t.scrollback_limit = 3;
	for i in 0..20 {
		feed(&mut t, format!("{i}\r\n").as_bytes());
	}
	assert_eq!(lines(&t), ["16", "17", "18"]);
	assert_eq!(t.history_base, 16);
	assert_eq!(t.row_cells(0).map(text).as_deref(), Some("16"));
	assert_eq!(t.row_cells(3).map(text).as_deref(), Some("19    "));
}

#[test]
fn narrowing_then_widening_pads_old_history_to_the_new_width() {
	let mut t = terminal(6, 1);
	feed(&mut t, b"abcdef\r\n");
	t.resize(3, 1);
	t.resize(5, 1);
	let mut frame = Frame::default();
	t.scroll_view(1);
	t.snapshot(&mut frame);
	let shown: String = frame.row(0).iter().map(|cell| cell.ch).collect();
	assert_eq!(shown, "abc  ");
}

#[test]
fn recoloring_the_defaults_reaches_history_and_its_padding() {
	let mut t = terminal(4, 1);
	feed(&mut t, b"ab\r\n");
	let old = t.padding();
	t.set_default_colors(0x112233, 0x445566);
	assert_eq!(t.scrollback[0][0].bg, 0x445566);
	assert_ne!(t.padding(), old);
	let mut frame = Frame::default();
	t.scroll_view(1);
	t.snapshot(&mut frame);
	assert!(frame.row(0).iter().all(|cell| cell.bg == 0x445566));
}

fn text(cells: &[Cell]) -> String {
	cells.iter().map(|cell| cell.ch).collect()
}

fn lines(term: &Term) -> Vec<String> {
	term.scrollback.iter().map(text).collect()
}

fn shown(term: &Term) -> Vec<String> {
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	(0..term.rows).map(|row| text(frame.row(row))).collect()
}

#[test]
fn rows_scrolled_in_under_a_colored_background_are_colored_to_the_edge() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"ab\r\n\x1b[41m\n");
	assert_eq!(lines(&t), ["ab"]);
	assert_ne!(t.blank(), t.padding());
	assert!(t.grid[1].iter().all(|cell| *cell == t.blank()));
}

#[test]
fn rows_scrolled_in_after_trimmed_ones_are_blank() {
	let mut t = terminal(5, 2);
	feed(&mut t, b"a b\r\nxyz\r\n\r\n");
	assert_eq!(lines(&t), ["a b", "xyz"]);
	assert_eq!(shown(&t), ["     ", "     "]);
	assert!(t.grid.iter().flatten().all(|cell| *cell == t.blank()));
}

#[test]
fn rows_scrolled_away_without_history_are_blanked_whole() {
	let mut t = terminal(3, 1);
	t.scrollback_limit = 0;
	feed(&mut t, b"abc\r\n");
	assert_eq!(shown(&t), ["   "]);
}

#[test]
fn rows_scrolled_in_inside_margins_are_blank() {
	let mut t = terminal(2, 3);
	feed(&mut t, b"\x1b[3Hcc\x1b[1;2raa\r\nbb\r\n");
	assert_eq!(shown(&t), ["bb", "  ", "cc"]);
}

#[test]
fn deleted_lines_leave_blank_rows_at_the_bottom() {
	let mut t = terminal(2, 3);
	feed(&mut t, b"aa\r\nbb\r\ncc\x1b[H\x1b[2M");
	assert_eq!(shown(&t), ["cc", "  ", "  "]);
}

#[test]
fn rows_scrolled_inside_margins_never_reach_history() {
	let mut t = terminal(4, 4);
	feed(&mut t, b"\x1b[2;3r\x1b[2Ha\r\nb\r\nc\r\nd");
	assert_eq!(t.scrollback.len(), 0);
	assert_eq!(t.history_base, 0);
	feed(&mut t, b"\x1b[r\x1b[4H\r\n");
	assert_eq!(lines(&t), [""]);
}

#[test]
fn the_alternate_screen_scrolls_without_history() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"\x1b[?1049hx\r\ny\r\nz\r\n");
	assert_eq!(t.scrollback.len(), 0);
	feed(&mut t, b"\x1b[?1049lq\r\nr\r\n");
	assert_eq!(lines(&t), ["q"]);
}

#[test]
fn a_zero_limit_counts_lines_without_keeping_them() {
	let mut t = terminal(4, 1);
	t.scrollback_limit = 0;
	feed(&mut t, b"a\r\nb\r\nc\r\n");
	assert_eq!(t.scrollback.len(), 0);
	assert_eq!(t.history_base, 3);
	t.scrollback_limit = 2;
	feed(&mut t, b"d\r\n");
	assert_eq!(lines(&t), ["d"]);
	assert_eq!(t.history_base, 3);
}

#[test]
fn erasing_history_then_scrolling_starts_a_fresh_history() {
	let mut t = terminal(4, 1);
	feed(&mut t, b"a\r\nb\r\nc\r\n\x1b[3J");
	assert_eq!((t.scrollback.len(), t.history_base), (0, 3));
	feed(&mut t, b"d\r\ne\r\n");
	assert_eq!(lines(&t), ["d", "e"]);
	assert_eq!(t.history_base, 3);
}

#[test]
fn scrolling_several_rows_at_once_keeps_their_order() {
	let mut t = terminal(3, 3);
	feed(&mut t, b"a\r\nb\r\nc\x1b[2S");
	assert_eq!(lines(&t), ["a", "b"]);
	assert_eq!(shown(&t), ["c  ", "   ", "   "]);
}

#[test]
fn a_wide_character_at_the_end_of_a_row_keeps_its_spacer() {
	let mut t = terminal(4, 1);
	feed(&mut t, "ab猫\r\n".as_bytes());
	let row = &t.scrollback[0];
	assert_eq!(row.len(), 4);
	assert_eq!((row[2].ch, row[2].width, row[3].width), ('猫', 2, 0));
}

#[test]
fn narrowing_truncates_history_to_the_new_width() {
	let mut t = terminal(8, 1);
	feed(&mut t, b"abcdefgh\r\nab\r\n");
	t.resize(4, 1);
	assert_eq!(lines(&t), ["abcd", "ab"]);
	assert!((0..t.scrollback.len()).all(|row| t.row_cells(row).is_some_and(|c| c.len() <= 4)));
}

#[test]
fn scrolling_the_view_shows_trimmed_rows_at_full_width() {
	let mut t = terminal(5, 2);
	feed(&mut t, b"one\r\ntwo\r\nthree\r\nfour");
	t.scroll_view(10);
	assert_eq!(t.scroll_offset, 2);
	assert_eq!(shown(&t), ["one  ", "two  "]);
}

#[test]
fn search_finds_text_in_trimmed_history_rows() {
	let mut t = terminal(10, 1);
	feed(&mut t, b"alpha\r\nbeta alpha\r\nnone");
	let found = find_matches(&t, "alpha");
	let points: Vec<_> = found
		.iter()
		.map(|m| (m.start.row, m.start.col, m.end.col))
		.collect();
	assert_eq!(points, [(0, 0, 4), (1, 5, 9)]);
	// Trimmed padding is not text: a trailing space matches nothing.
	assert!(find_matches(&t, "alpha ").is_empty());
}

#[test]
fn command_output_spanning_trimmed_history_keeps_wide_characters() {
	let mut t = terminal(10, 2);
	feed(
		&mut t,
		"\x1b]133;A\x07$ x\x1b]133;B\x07\r\n\x1b]133;C\x07猫a\r\n\r\nb\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ "
			.as_bytes(),
	);
	assert_eq!(t.last_command_output().as_deref(), Some("猫a\n\nb"));
}

#[test]
fn row_cells_spans_history_then_the_screen() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"a\r\nbb\r\nccc\r\nd");
	let row = |n| t.row_cells(n).map(text);
	assert_eq!(row(0).as_deref(), Some("a"));
	assert_eq!(row(1).as_deref(), Some("bb"));
	assert_eq!(row(2).as_deref(), Some("ccc "));
	assert_eq!(row(3).as_deref(), Some("d   "));
	assert_eq!(row(4), None);
}

#[test]
fn shrinking_then_raising_the_limit_keeps_only_the_survivors() {
	let mut t = terminal(3, 1);
	feed(&mut t, b"a\r\nb\r\nc\r\nd\r\n");
	t.scroll_view(3);
	t.set_scrollback_limit(2);
	assert_eq!(lines(&t), ["c", "d"]);
	assert_eq!((t.history_base, t.scroll_offset), (2, 2));
	t.set_scrollback_limit(100);
	feed(&mut t, b"e\r\nf\r\n");
	assert_eq!(lines(&t), ["c", "d", "e", "f"]);
	assert_eq!(t.history_base, 2);
}

#[test]
fn scrolling_the_view_stops_at_the_oldest_and_newest_rows() {
	let mut t = terminal(3, 1);
	feed(&mut t, b"a\r\nb\r\nc");
	t.scroll_view(10);
	assert_eq!(t.scroll_offset, 2);
	assert_eq!(shown(&t), ["a  "]);
	t.scroll_view(-1);
	assert_eq!(shown(&t), ["b  "]);
	t.scroll_view(-10);
	assert_eq!((t.scroll_offset, shown(&t)), (0, vec!["c  ".to_string()]));
}

#[test]
fn erased_cells_take_the_current_background_and_stay_in_history() {
	let mut t = terminal(4, 1);
	// EL 0 after the text: the colored blanks are content, not padding.
	feed(&mut t, b"ab\x1b[42m\x1b[K\x1b[m\r\n");
	assert_eq!(t.scrollback[0].len(), 4);
	assert!(
		t.scrollback[0][2..]
			.iter()
			.all(|cell| cell.bg == t.palette[2] && cell.ch == ' ')
	);
	// ED 2 with the default background leaves nothing to keep.
	feed(&mut t, b"cd\x1b[2J\r\n");
	assert_eq!(t.scrollback[1].len(), 0);
}

#[test]
fn clearing_history_releases_the_ring_and_later_output_regrows_it() {
	let mut t = terminal(80, 4);
	t.scrollback_limit = 1000;
	let full = "x".repeat(70);
	feed(&mut t, format!("{full}\r\n").repeat(2000).as_bytes());
	let (cells, rows) = t.scrollback.capacity();
	assert!(
		cells >= 1000 * 70 && rows >= 1000,
		"{cells} cells, {rows} rows"
	);
	// A flood leaves megabytes of history behind: `ESC [3J` gives them back.
	feed(&mut t, b"\x1b[3J");
	assert_eq!(t.scrollback.capacity(), (0, 0));
	assert!(lines(&t).is_empty());
	// The screen kept its rows; the next scroll stores one again.
	feed(&mut t, b"again\r\n");
	assert_eq!(lines(&t), [full]);
	assert!((1..cells).contains(&t.scrollback.capacity().0));
}

#[test]
fn erasing_the_screen_keeps_the_history_ring() {
	let mut t = terminal(80, 4);
	feed(&mut t, "x\r\n".repeat(100).as_bytes());
	let before = t.scrollback.capacity();
	assert_ne!(before.0, 0);
	feed(&mut t, b"\x1b[J\x1b[1J\x1b[2J");
	assert_eq!(t.scrollback.capacity(), before);
}

#[test]
fn a_zero_line_limit_holds_no_ring() {
	let mut t = terminal(80, 4);
	t.scrollback_limit = 0;
	feed(&mut t, "x\r\n".repeat(100).as_bytes());
	assert_eq!(t.scrollback.capacity(), (0, 0));
	assert_eq!(t.history_base, 97);
}

#[test]
fn lowering_the_limit_gives_back_the_ring_it_no_longer_needs() {
	let mut t = terminal(80, 4);
	t.scrollback_limit = 1000;
	let full = "x".repeat(70);
	feed(&mut t, format!("{full}\r\n").repeat(2000).as_bytes());
	assert!(t.scrollback.capacity().0 >= 1000 * 70);
	t.set_scrollback_limit(10);
	// Ten rows of the window's width, and the row a lap may skip.
	let (cells, rows) = t.scrollback.capacity();
	assert!((10 * 70..=11 * 80).contains(&cells), "{cells} cells");
	assert!((10..100).contains(&rows), "room for {rows} rows");
	assert_eq!(lines(&t), vec![full.clone(); 10]);
	// The smaller ring still takes output at the new limit.
	feed(&mut t, "y\r\n".repeat(5).as_bytes());
	assert_eq!(lines(&t).last().map(String::as_str), Some("y"));
	assert_eq!(lines(&t).len(), 10);
	t.set_scrollback_limit(0);
	assert_eq!(t.scrollback.capacity(), (0, 0));
}

#[test]
fn raising_the_limit_keeps_every_row() {
	let mut t = terminal(80, 4);
	t.scrollback_limit = 50;
	feed(&mut t, "z\r\n".repeat(60).as_bytes());
	let before = t.scrollback.capacity();
	t.set_scrollback_limit(5000);
	assert_eq!(t.scrollback.capacity(), before);
	assert_eq!(lines(&t).len(), 50);
}

#[test]
fn a_lower_limit_after_narrowing_keeps_the_rewrapped_rows() {
	let mut t = terminal(200, 2);
	t.scrollback_limit = 100;
	let wide = "w".repeat(150);
	feed(&mut t, format!("{wide}\r\n").repeat(40).as_bytes());
	t.resize(20, 2);
	let stored = lines(&t);
	// The history was rewrapped to twenty columns: a ring sized for 31
	// such rows still holds the 30 kept, uncut.
	t.set_scrollback_limit(30);
	assert_eq!(lines(&t), stored[stored.len() - 30..]);
}
