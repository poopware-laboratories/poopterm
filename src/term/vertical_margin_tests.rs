//! CUU, CUD, CNL and CPL stop at the top and bottom margins when the
//! cursor starts between them, origin mode or not (VT100, DEC STD 070).
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into())
}

/// Where `moves` take the cursor from row `from` (counted from 1) with
/// margins at rows 3 to 6 of a 10-row screen, as `(column, row)`.
fn after(from: usize, moves: &str) -> (usize, usize) {
	let mut t = terminal(10, 10);
	let bytes = format!("\x1b[3;6r\x1b[{from};4H{moves}");
	Parser::new().advance(&mut t, bytes.as_bytes());
	(t.cx + 1, t.cy + 1)
}

#[test]
fn cursor_up_stops_at_the_top_margin_from_inside_or_below_the_region() {
	// esctest CUU_StopsAtTopMarginInScrollRegion.
	assert_eq!(after(5, "\x1b[99A"), (4, 3));
	assert_eq!(after(3, "\x1b[A"), (4, 3));
	assert_eq!(after(9, "\x1b[99A"), (4, 3));
	// From above the region, the top of the screen.
	assert_eq!(after(2, "\x1b[99A"), (4, 1));
	assert_eq!(after(5, "\x1b[A"), (4, 4));
}

#[test]
fn cursor_down_stops_at_the_bottom_margin_from_inside_or_above_the_region() {
	// esctest CUD_StopsAtBottomMarginInScrollRegion.
	assert_eq!(after(4, "\x1b[99B"), (4, 6));
	assert_eq!(after(6, "\x1b[B"), (4, 6));
	assert_eq!(after(1, "\x1b[99B"), (4, 6));
	// From below the region, the bottom of the screen.
	assert_eq!(after(7, "\x1b[99B"), (4, 10));
	assert_eq!(after(4, "\x1b[B"), (4, 5));
}

#[test]
fn next_and_previous_line_stop_at_the_margins_and_return_the_carriage() {
	// esctest CNL_StopsAtBottomMarginInScrollRegion and
	// CPL_StopsAtTopMarginInScrollRegion.
	assert_eq!(after(4, "\x1b[99E"), (1, 6));
	assert_eq!(after(5, "\x1b[99F"), (1, 3));
	assert_eq!(after(8, "\x1b[99E"), (1, 10));
	assert_eq!(after(2, "\x1b[99F"), (1, 1));
}

#[test]
fn vertical_position_relative_and_backward_still_ignore_the_margins() {
	// VPR and VPB stop at the margins only in origin mode.
	assert_eq!(after(4, "\x1b[99e"), (4, 10));
	assert_eq!(after(5, "\x1b[99k"), (4, 1));
}
