//! Quick select while it is shown: the letters typed so far, what each key
//! does, and where the labels are drawn over their targets.
use winit::keyboard::{Key, NamedKey};

use super::{
	hints::{Hint, Pick, find_targets, label, pick},
	links::{LinkAction, link_action},
};
use crate::render::header::Rect;

/// The hints on the screen and the letters typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HintMode {
	pub hints: Vec<Hint>,
	pub typed: String,
}

/// What a key press in quick select leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
	/// Still choosing.
	Stay,
	/// Leave without choosing.
	Exit,
	/// A whole label: copy its text (`CopyPath`), or when it ended in a
	/// capital, open it as a modifier-click does ([`open_action`]).
	Use(LinkAction),
}

impl HintMode {
	/// Quick select over `rows` (as `hints::rows_of` gives them), or `None`
	/// when nothing on them is worth selecting.
	pub(crate) fn new(rows: &[Vec<(char, usize)>]) -> Option<Self> {
		let hints = label(find_targets(rows));
		(!hints.is_empty()).then(|| Self {
			hints,
			typed: String::new(),
		})
	}

	/// Handles `key`: a letter extends the label typed, and one no label
	/// goes on with is ignored; Backspace takes the last back; Escape leaves.
	pub(crate) fn press(&mut self, key: &Key) -> Outcome {
		let letter = match key {
			Key::Named(NamedKey::Escape) => return Outcome::Exit,
			Key::Named(NamedKey::Backspace) => {
				self.typed.pop();
				return Outcome::Stay;
			}
			Key::Character(text) => match text.chars().collect::<Vec<_>>()[..] {
				[ch] if ch.is_ascii_alphabetic() => ch,
				_ => return Outcome::Stay,
			},
			_ => return Outcome::Stay,
		};
		self.typed.push(letter.to_ascii_lowercase());
		match pick(&self.hints, &self.typed) {
			Pick::Chosen(text) if letter.is_ascii_uppercase() => Outcome::Use(open_action(text)),
			Pick::Chosen(text) => Outcome::Use(LinkAction::CopyPath(text.to_owned())),
			Pick::Narrowed(_) => Outcome::Stay,
			Pick::Nothing => {
				self.typed.pop();
				Outcome::Stay
			}
		}
	}

	/// Each shown label's chip and the letters on it, over the grid whose
	/// top left is `origin`, of `cols` columns of `cell` pixels.
	pub(crate) fn chips(
		&self,
		grid: ((usize, usize), (usize, usize)),
		cols: usize,
	) -> Vec<(Rect, &str)> {
		let shown = self.shown().into_iter();
		shown
			.map(|(hint, rest)| (chip(hint, rest.chars().count(), grid, cols), rest))
			.collect()
	}

	/// The hints whose label starts with what was typed, each with the
	/// letters left to type.
	pub(crate) fn shown(&self) -> Vec<(&Hint, &str)> {
		let typed = self.typed.as_str();
		(self.hints.iter())
			.filter_map(|hint| Some((hint, hint.label.strip_prefix(typed)?)))
			.collect()
	}
}

/// Where a label of `letters` letters goes over `hint`'s target: one cell
/// a letter from its first cell, kept inside the `cols` of the grid whose
/// top left is `origin` and whose cells are `cell` pixels.
pub(crate) fn chip(
	hint: &Hint,
	letters: usize,
	(origin, cell): ((usize, usize), (usize, usize)),
	cols: usize,
) -> Rect {
	let letters = letters.clamp(1, cols.max(1));
	let first = hint.target.cols.0.min(cols.saturating_sub(letters));
	let (x, y) = (
		origin.0 + first * cell.0,
		origin.1 + hint.target.row * cell.1,
	);
	(x, y, letters * cell.0, cell.1)
}

/// What opening `text` does: what a modifier-click on it would, and for
/// what is no link or path, such as a hash or an address, a copy.
pub(crate) fn open_action(text: &str) -> LinkAction {
	link_action(text).unwrap_or_else(|| LinkAction::CopyPath(text.to_owned()))
}
