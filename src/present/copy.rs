//! Which pixel rows a frame copies into a window buffer: the rows that
//! changed since the frame the buffer holds, which its age tells, so a
//! buffer that missed frames still ends up equal to the newest one. Rows
//! changed while no buffer was free carry over to the next frame shown.
use std::collections::VecDeque;

/// How many frames' changed rows are kept: enough for any buffer of the
/// pool, which holds at most one frame per buffer.
const KEPT: usize = 4;

/// The rows `[start, end)` to copy.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rows {
	All,
	/// Sorted, apart and inside the buffer.
	Some(Vec<(usize, usize)>),
}

#[derive(Clone)]
pub(crate) struct History {
	/// The rows each recent frame changed, oldest first; `None` for every
	/// row.
	frames: VecDeque<Option<Vec<(usize, usize)>>>,
	/// The rows changed since the last frame shown, as for `frames`.
	pending: Option<Vec<(usize, usize)>>,
}

impl History {
	pub(crate) fn new() -> Self {
		Self {
			frames: VecDeque::with_capacity(KEPT + 1),
			pending: None,
		}
	}

	/// The frame drawn changed `bands` of the last one, or every row.
	pub(crate) fn changed(&mut self, bands: Option<&[(usize, usize)]>) {
		match (&mut self.pending, bands) {
			(Some(pending), Some(bands)) => pending.extend_from_slice(bands),
			_ => self.pending = None,
		}
	}

	/// The rows to copy into a buffer of `rows` rows and age `age`: what
	/// changed since the frame it holds, or every row.
	pub(crate) fn copy(&self, age: u8, rows: usize) -> Rows {
		let age = usize::from(age);
		// The frame the buffer holds must be one of those kept.
		let (Some(bands), 1..) = (&self.pending, age) else {
			return Rows::All;
		};
		if age > self.frames.len() {
			return Rows::All;
		}
		let mut copied = bands.clone();
		for missed in self.frames.iter().rev().take(age - 1) {
			match missed {
				Some(missed) => copied.extend_from_slice(missed),
				None => return Rows::All,
			}
		}
		Rows::Some(merge(copied, rows))
	}

	/// The rows of `rows` that changed since the last frame shown, to tell
	/// the compositor.
	pub(crate) fn damage(&self, rows: usize) -> Rows {
		match &self.pending {
			Some(bands) => Rows::Some(merge(bands.clone(), rows)),
			None => Rows::All,
		}
	}

	/// The frame drawn was shown.
	pub(crate) fn shown(&mut self) {
		let pending = self.pending.replace(Vec::new());
		self.frames.push_back(pending);
		if self.frames.len() > KEPT {
			self.frames.pop_front();
		}
	}

	/// Forgets every frame, as when the buffers are remade, and redraws
	/// every row.
	pub(crate) fn clear(&mut self) {
		self.frames.clear();
		self.pending = None;
	}
}

/// `bands` inside `rows`, sorted, with touching ones joined.
fn merge(mut bands: Vec<(usize, usize)>, rows: usize) -> Vec<(usize, usize)> {
	bands.retain_mut(|band| {
		band.1 = band.1.min(rows);
		band.0 < band.1
	});
	bands.sort_unstable();
	let mut merged: Vec<(usize, usize)> = Vec::with_capacity(bands.len());
	for (start, end) in bands {
		match merged.last_mut() {
			Some(last) if start <= last.1 => last.1 = last.1.max(end),
			_ => merged.push((start, end)),
		}
	}
	merged
}
