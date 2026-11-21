//! Asking before closing a pane or tab whose program is still running:
//! what to ask, what keys answer it, and the prompt drawn in the command
//! palette's place.
use winit::keyboard::{Key, NamedKey};

use crate::{
	render::popover::Popover,
	theme::Theme,
	ui::overlays::{Place, command_palette},
};

/// What a close would end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseTarget {
	/// The pane of this session.
	Pane(u64),
	/// The tab holding this session, with all its panes.
	Tab(u64),
}

/// A close waiting for an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CloseRequest {
	pub target: CloseTarget,
	/// What the prompt says is running.
	pub message: String,
}

/// Whether closing `target` ends session `id`, given the tab each session
/// is in: a pane ends its own session, a tab every session in it.
pub(crate) fn ends(target: CloseTarget, id: u64, tab_of: impl Fn(u64) -> Option<usize>) -> bool {
	match target {
		CloseTarget::Pane(pane) => id == pane,
		CloseTarget::Tab(of) => tab_of(id).is_some_and(|tab| Some(tab) == tab_of(of)),
	}
}

/// The most characters of a program's name shown.
const NAME_CHARS: usize = 40;

/// The note under the prompt, saying what the keys do.
pub(crate) const NOTE: &str = "Enter closes and ends it, Esc keeps it";

/// Whether closing `target` must be asked first, given the programs other
/// than shells running in the sessions it would end; `None` closes at once.
pub(crate) fn close_request(target: CloseTarget, running: &[String]) -> Option<CloseRequest> {
	let short = |name: &str| -> String {
		let name: String = name.chars().filter(|c| !c.is_control()).collect();
		if name.trim().is_empty() {
			return crate::pty::running::UNNAMED.to_owned();
		}
		let cut: String = name.chars().take(NAME_CHARS).collect();
		if cut.len() < name.len() {
			format!("{cut}…")
		} else {
			cut
		}
	};
	let message = match running {
		[] => return None,
		[one] => format!("{} is still running", short(one)),
		[first, rest @ ..] => format!("{} and {} more are still running", short(first), rest.len()),
	};
	Some(CloseRequest { target, message })
}

/// How a key answers the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
	Close,
	Keep,
	/// Neither: the prompt stays.
	Wait,
}

/// How `key` answers: Enter, or the close shortcut pressed again
/// (`close_chord`), closes; Escape keeps.
pub(crate) fn answer(key: &Key, close_chord: bool) -> Answer {
	match key {
		_ if close_chord => Answer::Close,
		Key::Named(NamedKey::Enter) => Answer::Close,
		Key::Named(NamedKey::Escape) => Answer::Keep,
		_ => Answer::Wait,
	}
}

/// The prompt, where the command palette floats: the message in the field
/// and the note under it.
pub(crate) fn close_prompt<'a>(place: Place, text: &'a str, px: f32, theme: &Theme) -> Popover<'a> {
	// The palette while renaming has a field and a note, and no rows.
	let mut popover = command_palette(place, (text, true), px, theme);
	popover.field.placeholder = "";
	popover.note = popover.note.map(|(rect, _)| (rect, NOTE));
	popover
}
