//! Mouse reports read from one view of a terminal: which events belong to
//! the program, and the cell or pixel they name.
use super::hover::GridPlace;
use super::mouse_report::{Report, mouse_report};
use crate::{
	input::{
		keys::Modifiers,
		mouse::{MouseAction, MouseButtonCode},
	},
	term::{SharedTerm, Term},
	theme::{Theme, ThemeKind},
};

/// A terminal of `cols` by `rows` that has been sent `bytes`.
fn terminal(cols: usize, rows: usize, bytes: &[u8]) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	crate::parser::Parser::new().advance(&mut term, bytes);
	term
}

/// A grid at (10, 20) with 8×16 cells, its area ending 4 pixels past the
/// last row of a 3-row terminal.
const PLACE: GridPlace = GridPlace {
	left: 10,
	top: 20,
	bottom: 20 + 3 * 16 + 4,
	cell: (8, 16),
};

/// The centre of cell (`row`, `col`) on screen.
fn at(row: usize, col: usize) -> (f64, f64) {
	((10 + col * 8 + 4) as f64, (20 + row * 16 + 8) as f64)
}

fn report(term: &Term, button: MouseButtonCode, kind: MouseAction, at: (f64, f64)) -> Report {
	mouse_report(term, PLACE, button, kind, at, Modifiers::default())
}

fn sent(bytes: &str) -> Report {
	Report::Program(Some(bytes.as_bytes().to_vec()))
}

#[test]
fn the_report_is_built_while_the_caller_holds_the_terminal_lock() {
	// Building the pixel report once locked the terminal a second time on
	// the UI thread, freezing Poopterm on the first pointer motion over a
	// program that tracks the mouse in pixels (?1016).
	let shared = SharedTerm::new(terminal(10, 3, b"\x1b[?1003h\x1b[?1016h"));
	let term = shared.lock();
	let found = report(&term, MouseButtonCode::None, MouseAction::Motion, at(1, 2));
	assert_eq!(found, sent("\x1b[<35;21;25M"));
}

#[test]
fn any_event_tracking_reports_motion_in_sgr_cells() {
	let term = terminal(10, 3, b"\x1b[?1003h\x1b[?1006h");
	let found = report(&term, MouseButtonCode::None, MouseAction::Motion, at(2, 4));
	assert_eq!(found, sent("\x1b[<35;5;3M"));
}

#[test]
fn sgr_pixels_count_from_one_at_the_grid_corner() {
	let term = terminal(10, 3, b"\x1b[?1000h\x1b[?1016h");
	let corner = (10.0, 20.0);
	assert_eq!(
		report(&term, MouseButtonCode::Left, MouseAction::Press, corner),
		sent("\x1b[<0;1;1M")
	);
	assert_eq!(
		report(&term, MouseButtonCode::Left, MouseAction::Release, corner),
		sent("\x1b[<0;1;1m")
	);
}

#[test]
fn below_the_last_row_pixels_stop_at_the_grid_edge() {
	let term = terminal(10, 3, b"\x1b[?1000h\x1b[?1016h");
	// The 4 pixels under the last row still belong to it.
	let found = report(
		&term,
		MouseButtonCode::Right,
		MouseAction::Press,
		(14.0, 70.0),
	);
	assert_eq!(found, sent("\x1b[<2;5;48M"));
}

#[test]
fn events_outside_the_grid_stay_with_the_terminal() {
	let term = terminal(10, 3, b"\x1b[?1003h\x1b[?1006h");
	for outside in [(9.0, 30.0), (90.0, 30.0), (14.0, 19.0), (14.0, 72.0)] {
		let found = report(&term, MouseButtonCode::Left, MouseAction::Press, outside);
		assert_eq!(found, Report::Local, "{outside:?}");
	}
}

#[test]
fn without_tracking_every_event_stays_with_the_terminal() {
	let term = terminal(10, 3, b"\x1b[?1006h");
	for kind in [
		MouseAction::Press,
		MouseAction::Release,
		MouseAction::Motion,
	] {
		let found = report(&term, MouseButtonCode::Left, kind, at(0, 0));
		assert_eq!(found, Report::Local, "{kind:?}");
	}
}

#[test]
fn button_event_tracking_reports_drags_but_not_plain_motion() {
	let term = terminal(10, 3, b"\x1b[?1002h\x1b[?1006h");
	assert_eq!(
		report(&term, MouseButtonCode::None, MouseAction::Motion, at(0, 0)),
		Report::Local
	);
	assert_eq!(
		report(&term, MouseButtonCode::Left, MouseAction::Motion, at(0, 1)),
		sent("\x1b[<32;2;1M")
	);
}

#[test]
fn x10_compatibility_reports_presses_only_without_modifiers() {
	let term = terminal(10, 3, b"\x1b[?9h");
	let ctrl = Modifiers {
		ctrl: true,
		..Modifiers::default()
	};
	let press = mouse_report(
		&term,
		PLACE,
		MouseButtonCode::Left,
		MouseAction::Press,
		at(1, 1),
		ctrl,
	);
	assert_eq!(press, Report::Program(Some(b"\x1b[M\x20\x22\x22".to_vec())));
	assert_eq!(
		report(&term, MouseButtonCode::Left, MouseAction::Release, at(1, 1)),
		Report::Local
	);
}

#[test]
fn modifiers_reach_the_program() {
	let term = terminal(10, 3, b"\x1b[?1000h\x1b[?1006h");
	let shift_ctrl = Modifiers {
		shift: true,
		ctrl: true,
		..Modifiers::default()
	};
	let found = mouse_report(
		&term,
		PLACE,
		MouseButtonCode::WheelUp,
		MouseAction::Press,
		at(0, 0),
		shift_ctrl,
	);
	assert_eq!(found, sent("\x1b[<84;1;1M"));
}

#[test]
fn a_cell_legacy_bytes_cannot_address_is_still_the_programs() {
	// Column 224 is past what one byte can carry: the event is the
	// program's, so the terminal does not select, but nothing is sent.
	let term = terminal(230, 3, b"\x1b[?1000h");
	let found = report(&term, MouseButtonCode::Left, MouseAction::Press, at(0, 224));
	assert_eq!(found, Report::Program(None));
	let last = report(&term, MouseButtonCode::Left, MouseAction::Press, at(0, 222));
	assert_eq!(last, Report::Program(Some(b"\x1b[M\x20\xff\x21".to_vec())));
}

#[test]
fn a_scrolled_view_reports_the_row_on_screen() {
	let mut term = terminal(10, 3, b"1\r\n2\r\n3\r\n4\r\n5\x1b[?1000h\x1b[?1006h");
	assert_eq!(term.scrollback.len(), 2);
	term.scroll_offset = 2;
	let found = report(&term, MouseButtonCode::Left, MouseAction::Press, at(2, 0));
	assert_eq!(found, sent("\x1b[<0;1;3M"));
}
