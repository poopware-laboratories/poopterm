//! Frame timing for benchmarks: how long each frame took to paint and
//! show, and how long typing took to reach the screen. Recorded only when
//! `POOPTERM_FRAME_STATS` names a file, which gets the samples as JSON on
//! exit, for a benchmark to summarize.
use std::{
	ffi::OsString,
	io,
	path::PathBuf,
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

#[derive(Debug, Default)]
pub(super) struct FrameStats {
	/// Frames drawn in software, on the GPU, and painted whole for the GPU.
	pub kinds: [usize; 3],
	/// Per frame: painting in software, showing (the GPU's recording,
	/// submission and presentation, or the software presenter), all of it.
	pub paint: Vec<Duration>,
	pub show: Vec<Duration>,
	pub total: Vec<Duration>,
	pub latency: Vec<Duration>,
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

	/// A frame of `kind` took `[paint, show, total]` and ended at `end`.
	pub(super) fn frame(&mut self, kind: Kind, [paint, show, total]: [Duration; 3], end: Instant) {
		self.kinds[kind as usize] += 1;
		if self.total.len() < MAX_SAMPLES {
			self.paint.push(paint);
			self.show.push(show);
			self.total.push(total);
		}
		if self.echoed
			&& let Some(key) = self.key.take()
			&& self.latency.len() < MAX_SAMPLES
		{
			self.latency.push(end.saturating_duration_since(key));
		}
	}

	/// The samples as one line of JSON, in microseconds, for a window
	/// `size` pixels large at this display `scale`.
	pub(super) fn report(&self, (width, height): (usize, usize), scale: f64) -> String {
		let [software, gpu, whole] = self.kinds;
		let us = |samples: &[Duration]| {
			let samples: Vec<_> = samples
				.iter()
				.map(|time| format!("{:.1}", time.as_nanos() as f64 / 1e3))
				.collect();
			format!("[{}]", samples.join(", "))
		};
		format!(
			"{{\"frames\": {{\"software\": {software}, \"gpu\": {gpu}, \"gpu_whole\": {whole}}}, \
			 \"size\": [{width}, {height}], \"scale\": {scale}, \"paint_us\": {}, \"show_us\": {}, \
			 \"total_us\": {}, \"latency_us\": {}}}",
			us(&self.paint),
			us(&self.show),
			us(&self.total),
			us(&self.latency),
		)
	}
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
		variable.filter(|path| !path.is_empty()).map(|path| Self {
			path: path.into(),
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
		let paint = std::mem::take(&mut self.paint);
		let whole = std::mem::take(&mut self.whole);
		let (kind, show) = match (gpu, whole) {
			(false, _) => (Kind::Software, showing),
			(true, false) => (Kind::Gpu, showing.saturating_sub(paint)),
			(true, true) => (Kind::GpuWhole, showing.saturating_sub(paint)),
		};
		let total = end.saturating_duration_since(start);
		self.stats.frame(kind, [paint, show, total], end);
	}

	/// Writes the report, ended by a newline, for a window `size` pixels
	/// large at `scale`.
	pub(super) fn save(&self, size: (usize, usize), scale: f64) -> io::Result<()> {
		std::fs::write(&self.path, format!("{}\n", self.stats.report(size, scale)))
	}
}

#[cfg(test)]
#[path = "frame_stats_tests.rs"]
mod tests;
