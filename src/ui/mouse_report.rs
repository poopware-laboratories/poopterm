//! What a mouse event over the grid sends to a program that tracks the mouse.
//!
//! Everything is read from one view of the terminal, which the caller has
//! locked once: locking it again on the same thread would deadlock.
use super::hover::GridPlace;
use crate::{
	input::{
		keys::Modifiers,
		mouse::{MouseAction, MouseButtonCode, MouseReport, encode_mouse, tracked},
	},
	term::Term,
};

/// Who a mouse event belongs to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Report {
	/// The terminal's: it selects or scrolls.
	Local,
	/// The program's, with the bytes to send, or none when its encoding
	/// cannot address the cell.
	Program(Option<Vec<u8>>),
}

/// The report for a `kind` event of `button` at window pixel `at`, over the
/// grid of `term` drawn at `place`.
pub(crate) fn mouse_report(
	term: &Term,
	place: GridPlace,
	button: MouseButtonCode,
	kind: MouseAction,
	(x, y): (f64, f64),
	mods: Modifiers,
) -> Report {
	let geometry = place.geometry(term);
	let Some(point) = geometry.point_at(x, y) else {
		return Report::Local;
	};
	if !tracked(term.mouse_tracking, button, kind) {
		return Report::Local;
	}
	let visible_start = term.scrollback.len().saturating_sub(term.scroll_offset);
	let report = MouseReport {
		button,
		kind,
		col: point.col + 1,
		row: point.row.saturating_sub(visible_start) + 1,
		pixel: geometry.pixel_at(x, y).unwrap_or((1, 1)),
		mods,
	};
	Report::Program(encode_mouse(
		report,
		term.mouse_encoding,
		term.mouse_tracking,
	))
}
