//! A tab's panes with the sessions they show. The focused pane's session is
//! kept outside, where the window's list of tabs holds it so input, titles
//! and the tab strip follow it; the others wait here. Generic over the
//! session, so it is tested without shells.
use super::panes::{Panes, Place, Split};
use crate::graphics::Rect;

/// A session a pane shows, named by its id.
pub(crate) trait Shown {
	fn id(&self) -> u64;
}

/// What closing a pane did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Closed<S> {
	/// The session is not a pane of this tab.
	Missing,
	/// It is the tab's last pane: closing the tab closes it.
	Last,
	/// The pane closed; its session, to stop.
	Pane(S),
}

/// A tab's panes and the sessions of those it does not focus. The focused
/// session, passed to each method as `focused`, is always the layout's.
pub(crate) struct TabPanes<S> {
	layout: Panes,
	others: Vec<S>,
}

impl<S: Shown> TabPanes<S> {
	/// The panes of a tab showing session `id` alone.
	pub(crate) fn new(id: u64) -> Self {
		Self {
			layout: Panes::new(id),
			others: Vec::new(),
		}
	}

	pub(crate) fn layout(&self) -> &Panes {
		&self.layout
	}

	/// The sessions of the panes not focused.
	pub(crate) fn others(&self) -> &[S] {
		&self.others
	}

	pub(crate) fn others_mut(&mut self) -> &mut [S] {
		&mut self.others
	}

	/// The sessions of the panes not focused, as the tab closes.
	pub(crate) fn into_others(self) -> Vec<S> {
		self.others
	}

	/// Session `id`, if one of this tab's panes shows it.
	pub(crate) fn find<'a>(&'a self, focused: &'a S, id: u64) -> Option<&'a S> {
		std::iter::once(focused)
			.chain(&self.others)
			.find(|session| session.id() == id)
	}

	/// Focuses pane `id`, swapping its session with `focused`. Returns
	/// whether focus moved.
	pub(crate) fn focus(&mut self, focused: &mut S, id: u64) -> bool {
		let Some(index) = self.others.iter().position(|s| s.id() == id) else {
			return false;
		};
		self.layout.focus(id);
		std::mem::swap(focused, &mut self.others[index]);
		true
	}

	/// Moves focus as `step` moves it in the layout. Returns whether it moved.
	pub(crate) fn focus_by(
		&mut self,
		focused: &mut S,
		step: impl FnOnce(&mut Panes) -> bool,
	) -> bool {
		let mut layout = self.layout.clone();
		step(&mut layout) && self.focus(focused, layout.focused())
	}

	/// Where a pane for session `id` would go if the focused pane were
	/// split that way, in `area`; `None` when it cannot be split so.
	pub(crate) fn split_place(
		&self,
		split: Split,
		id: u64,
		(area, cell, divider): (Rect, (usize, usize), usize),
	) -> Option<Place> {
		let mut layout = self.layout.clone();
		layout.split(split, id).then_some(())?;
		(layout.places(area, cell, divider).into_iter()).find(|place| place.id == id)
	}

	/// Splits the focused pane that way, the new pane showing `new` and
	/// taking focus. Hands `new` back when the pane cannot be split so.
	pub(crate) fn split(&mut self, focused: &mut S, split: Split, mut new: S) -> Result<(), S> {
		if !self.layout.split(split, new.id()) {
			return Err(new);
		}
		std::mem::swap(focused, &mut new);
		self.others.push(new);
		Ok(())
	}

	/// Closes pane `id`; the pane taking its room takes its focus too.
	pub(crate) fn close(&mut self, focused: &mut S, id: u64) -> Closed<S> {
		if self.find(focused, id).is_none() {
			return Closed::Missing;
		}
		if !self.layout.close(id) {
			return Closed::Last;
		}
		let heir = self.layout.focused();
		if let Some(index) = self.others.iter().position(|s| s.id() == heir) {
			std::mem::swap(focused, &mut self.others[index]);
		}
		match self.others.iter().position(|s| s.id() == id) {
			Some(index) => Closed::Pane(self.others.remove(index)),
			None => Closed::Missing,
		}
	}

	/// The pane under `point` in `area`, unless it is the focused one.
	pub(crate) fn other_at(
		&self,
		(area, cell, divider): (Rect, (usize, usize), usize),
		point: (usize, usize),
	) -> Option<u64> {
		let hit = self.layout.hit(area, cell, divider, point)?;
		(hit != self.layout.focused()).then_some(hit)
	}
}
