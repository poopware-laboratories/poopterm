//! The last frames the main window drew, each on the GPU or in software,
//! for the Diagnostics page: a fixed ring of bits, so a frame costs a
//! store and nothing is allocated.

/// Frames remembered.
pub(crate) const KEPT: usize = 600;

/// One bit per remembered frame, set when the GPU drew it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RecentFrames {
	gpu: [u64; KEPT.div_ceil(64)],
	/// Where the next frame goes, and how many have been recorded, up to
	/// [`KEPT`].
	next: usize,
	recorded: usize,
}

/// How many of the recent frames each path drew.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameCounts {
	pub(crate) gpu: usize,
	pub(crate) software: usize,
}

impl RecentFrames {
	/// A frame was drawn, on the GPU if `gpu`.
	pub(crate) fn push(&mut self, gpu: bool) {
		let (word, bit) = (self.next / 64, 1 << (self.next % 64));
		if gpu {
			self.gpu[word] |= bit;
		} else {
			self.gpu[word] &= !bit;
		}
		self.next = (self.next + 1) % KEPT;
		self.recorded = (self.recorded + 1).min(KEPT);
	}

	/// The remembered frames by path. Bits not recorded yet are clear.
	pub(crate) fn counts(&self) -> FrameCounts {
		let gpu = self.gpu.iter().map(|word| word.count_ones() as usize).sum();
		FrameCounts {
			gpu,
			software: self.recorded - gpu,
		}
	}
}

impl FrameCounts {
	/// The page's line, `None` before the first frame.
	pub(crate) fn line(self) -> Option<String> {
		let Self { gpu, software } = self;
		let frames = match gpu + software {
			0 => return None,
			1 => "Last frame".to_owned(),
			total => format!("Last {total} frames"),
		};
		Some(format!("{frames}: {gpu} GPU \u{b7} {software} software"))
	}
}

#[cfg(test)]
#[path = "recent_frames_tests.rs"]
mod tests;
