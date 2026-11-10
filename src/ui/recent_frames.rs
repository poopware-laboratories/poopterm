//! The last frames the main window drew, each on the GPU or in software,
//! for the Diagnostics page: a fixed ring of bits, so a frame costs a
//! store and nothing is allocated; and the GPU's time for those it drew.
use std::time::Duration;

/// Frames remembered.
pub(crate) const KEPT: usize = 600;

/// One bit per remembered frame, set when the GPU drew it, and the GPU's
/// time for each it drew and timed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecentFrames {
	gpu: [u64; KEPT.div_ceil(64)],
	/// Where the next frame goes, and how many have been recorded, up to
	/// [`KEPT`].
	next: usize,
	recorded: usize,
	/// Microseconds, where the next goes, and how many are kept.
	times: [u32; KEPT],
	next_time: usize,
	timed: usize,
}

impl Default for RecentFrames {
	fn default() -> Self {
		Self {
			gpu: Default::default(),
			next: 0,
			recorded: 0,
			times: [0; KEPT],
			next_time: 0,
			timed: 0,
		}
	}
}

/// The GPU's time for the recent frames it drew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuTime {
	pub(crate) median: Duration,
	pub(crate) worst: Duration,
}

/// How many of the recent frames each path drew.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameCounts {
	pub(crate) gpu: usize,
	pub(crate) software: usize,
	pub(crate) gpu_time: Option<GpuTime>,
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

	/// A frame was drawn, on the GPU if `gpu`, which took the GPU `time`
	/// if it says; a time beside a software frame is an older one's.
	pub(crate) fn push_drawn(&mut self, gpu: bool, time: Option<Duration>) {
		self.push(gpu);
		if let (true, Some(time)) = (gpu, time) {
			self.times[self.next_time] = u32::try_from(time.as_micros()).unwrap_or(u32::MAX);
			self.next_time = (self.next_time + 1) % KEPT;
			self.timed = (self.timed + 1).min(KEPT);
		}
	}

	/// The remembered frames by path. Bits not recorded yet are clear.
	pub(crate) fn counts(&self) -> FrameCounts {
		let gpu = self.gpu.iter().map(|word| word.count_ones() as usize).sum();
		FrameCounts {
			gpu,
			software: self.recorded - gpu,
			gpu_time: self.gpu_time(),
		}
	}

	/// The median and worst of the kept times; the lower middle one of an
	/// even count.
	fn gpu_time(&self) -> Option<GpuTime> {
		// Until the ring is full, the times kept are the first ones.
		let mut times = self.times[..self.timed].to_vec();
		times.sort_unstable();
		let micros = |time: u32| Duration::from_micros(u64::from(time));
		Some(GpuTime {
			median: micros(*times.get((times.len().max(1) - 1) / 2)?),
			worst: micros(*times.last()?),
		})
	}
}

impl FrameCounts {
	/// The page's line, `None` before the first frame.
	pub(crate) fn line(self) -> Option<String> {
		let Self {
			gpu,
			software,
			gpu_time,
		} = self;
		let frames = match gpu + software {
			0 => return None,
			1 => "Last frame".to_owned(),
			total => format!("Last {total} frames"),
		};
		let mut line = format!("{frames}: {gpu} GPU \u{b7} {software} software");
		if let Some(GpuTime { median, worst }) = gpu_time {
			let ms = |time: Duration| time.as_secs_f64() * 1000.0;
			let (median, worst) = (ms(median), ms(worst));
			line += &format!(" \u{b7} GPU {median:.1} ms median, {worst:.1} ms worst");
		}
		Some(line)
	}
}

#[cfg(test)]
#[path = "recent_frames_tests.rs"]
mod tests;
