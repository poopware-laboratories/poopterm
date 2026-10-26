//! Which pixel rows a frame copies into a window buffer: the rows that
//! changed since the frame the buffer holds, which its age tells, so a
//! buffer that missed frames still ends up equal to the newest one.
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

pub(crate) struct History {
	/// The rows each recent frame changed, oldest first; `None` for every
	/// row.
	frames: VecDeque<Option<Vec<(usize, usize)>>>,
}

impl History {
	pub(crate) fn new() -> Self {
		Self {
			frames: VecDeque::with_capacity(KEPT + 1),
		}
	}

	/// The rows to copy into a buffer of `rows` rows and age `age`, for a
	/// frame that changed `bands` of the last one, or every row.
	pub(crate) fn copy(&self, age: u8, bands: Option<&[(usize, usize)]>, rows: usize) -> Rows {
		let age = usize::from(age);
		// The frame the buffer holds must be one of those kept.
		let (Some(bands), 1..) = (bands, age) else {
			return Rows::All;
		};
		if age > self.frames.len() {
			return Rows::All;
		}
		let mut copied = bands.to_vec();
		for missed in self.frames.iter().rev().take(age - 1) {
			match missed {
				Some(missed) => copied.extend_from_slice(missed),
				None => return Rows::All,
			}
		}
		Rows::Some(merge(copied, rows))
	}

	/// A frame that changed `bands` of the last one, or every row, was shown.
	pub(crate) fn shown(&mut self, bands: Option<&[(usize, usize)]>) {
		self.frames.push_back(bands.map(<[_]>::to_vec));
		if self.frames.len() > KEPT {
			self.frames.pop_front();
		}
	}

	/// Forgets every frame, as when the buffers are remade.
	pub(crate) fn clear(&mut self) {
		self.frames.clear();
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
