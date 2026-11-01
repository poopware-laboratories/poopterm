//! Frame timing for benchmarks: how long each frame took to paint and
//! show, and how long typing took to reach the screen. Recorded only when
//! `POOPTERM_FRAME_STATS` names a file, which gets a JSON summary on exit
//! (`scripts/bench-frames.py` reads it).
use std::{
	ffi::OsString,
	io,
	path::{Path, PathBuf},
	time::{Duration, Instant},
};

/// The environment variable naming the report's file.
pub(super) const VARIABLE: &str = "POOPTERM_FRAME_STATS";
/// Samples kept per measure; a benchmark needs far fewer.
pub(super) const MAX_SAMPLES: usize = 1 << 16;

/// How a frame was drawn.
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
	Software,
	Gpu,
	/// Painted whole by software and shown through the GPU.
	GpuWhole,
}

/// How long one frame took: painting in software, showing it (the GPU's
/// recording, submission and presentation, or the software presenter),
/// and all of `draw`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Timing {
	pub paint: Duration,
	pub show: Duration,
	pub total: Duration,
}

#[derive(Debug, Default)]
pub(super) struct FrameStats {
	/// Frames drawn in software, on the GPU, and painted whole for the GPU.
	pub kinds: [usize; 3],
	pub paint: Vec<Duration>,
	pub show: Vec<Duration>,
	pub total: Vec<Duration>,
	pub latency: Vec<Duration>,
	/// Frames past the limit, counted but not timed.
	pub dropped: usize,
	/// The first key not answered yet, and whether output came after it.
	key: Option<Instant>,
	echoed: bool,
}

impl FrameStats {
	/// A key was pressed `at`; keys typed before its echo are timed with it.
	pub(super) fn key(&mut self, at: Instant) {
		if self.key.is_none() {
			self.key = Some(at);
			self.echoed = false;
		}
	}

	/// The active terminal had output: the echo of a pending key.
	pub(super) fn output(&mut self) {
		self.echoed |= self.key.is_some();
	}

	/// A frame of `kind` took `timing` and ended at `end`.
	pub(super) fn frame(&mut self, kind: Kind, timing: Timing, end: Instant) {
		self.kinds[kind as usize] += 1;
		if self.total.len() < MAX_SAMPLES {
			self.paint.push(timing.paint);
			self.show.push(timing.show);
			self.total.push(timing.total);
		} else {
			self.dropped += 1;
		}
		if self.echoed
			&& let Some(key) = self.key.take()
		{
			self.echoed = false;
			if self.latency.len() < MAX_SAMPLES {
				self.latency.push(end.saturating_duration_since(key));
			}
		}
	}

	/// The summary as one line of JSON, for a window `size` pixels large
	/// at this display `scale`.
	pub(super) fn report(&self, (width, height): (usize, usize), scale: f64) -> String {
		let [software, gpu, whole] = self.kinds;
		let measures = [
			("paint_us", &self.paint),
			("show_us", &self.show),
			("total_us", &self.total),
			("latency_us", &self.latency),
		];
		let spreads = measures.map(|(name, samples)| {
			let json = spread(&mut samples.clone()).map_or("null".into(), |spread| spread.json());
			format!("\"{name}\": {json}")
		});
		format!(
			"{{\"frames\": {{\"software\": {software}, \"gpu\": {gpu}, \"gpu_whole\": {whole}}}, \
			 \"size\": [{width}, {height}], \"scale\": {scale}, \"dropped\": {}, {}}}",
			self.dropped,
			spreads.join(", "),
		)
	}
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Spread {
	pub count: usize,
	pub median: Duration,
	pub p95: Duration,
	pub min: Duration,
	pub max: Duration,
}

impl Spread {
	fn json(&self) -> String {
		let us = |time: Duration| format!("{:.1}", time.as_nanos() as f64 / 1000.0);
		format!(
			"{{\"count\": {}, \"median\": {}, \"p95\": {}, \"min\": {}, \"max\": {}}}",
			self.count,
			us(self.median),
			us(self.p95),
			us(self.min),
			us(self.max),
		)
	}
}

/// The count, median, 95th percentile and bounds of `samples`, which it
/// sorts; percentiles are nearest ranks.
pub(super) fn spread(samples: &mut [Duration]) -> Option<Spread> {
	samples.sort_unstable();
	let (min, max) = (*samples.first()?, *samples.last()?);
	let rank = |fraction: f64| {
		let rank = (fraction * samples.len() as f64).ceil() as usize;
		samples[rank.max(1) - 1]
	};
	Some(Spread {
		count: samples.len(),
		median: rank(0.5),
		p95: rank(0.95),
		min,
		max,
	})
}

/// Frame statistics being recorded for the file they are saved to.
#[derive(Debug)]
pub(super) struct Recorder {
	pub path: PathBuf,
	pub stats: FrameStats,
	/// Painted in software so far this frame, and whether whole.
	paint: Duration,
	whole: bool,
}

impl Recorder {
	/// A recorder when `variable`, the environment's, names a file.
	pub(super) fn from_env(variable: Option<OsString>) -> Option<Self> {
		destination(variable).map(|path| Self {
			path,
			stats: FrameStats::default(),
			paint: Duration::ZERO,
			whole: false,
		})
	}

	/// Software painted for `took` this frame, the whole frame if `whole`.
	pub(super) fn painted(&mut self, took: Duration, whole: bool) {
		self.paint += took;
		self.whole |= whole;
	}

	/// The frame drawn from `start` to `end` is done: shown in `showing`, by
	/// the software presenter, or else by the GPU's frame call, which also
	/// painted.
	pub(super) fn finish(&mut self, gpu: bool, showing: Duration, start: Instant, end: Instant) {
		let (kind, show) = match (gpu, self.whole) {
			(false, _) => (Kind::Software, showing),
			(true, whole) => {
				let kind = if whole { Kind::GpuWhole } else { Kind::Gpu };
				(kind, showing.saturating_sub(self.paint))
			}
		};
		let total = end.saturating_duration_since(start);
		let paint = std::mem::take(&mut self.paint);
		self.whole = false;
		self.stats.frame(kind, Timing { paint, show, total }, end);
	}

	/// Writes the report for a window `size` pixels large at `scale`.
	pub(super) fn save(&self, size: (usize, usize), scale: f64) -> io::Result<()> {
		write_report(&self.stats, &self.path, size, scale)
	}
}

/// The report's file, from the environment variable; unset when empty.
pub(super) fn destination(variable: Option<OsString>) -> Option<PathBuf> {
	variable.filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// Writes the report of `stats` to `path`, ended by a newline.
pub(super) fn write_report(
	stats: &FrameStats,
	path: &Path,
	size: (usize, usize),
	scale: f64,
) -> io::Result<()> {
	std::fs::write(path, format!("{}\n", stats.report(size, scale)))
}

#[cfg(test)]
#[path = "frame_stats_tests.rs"]
mod tests;
