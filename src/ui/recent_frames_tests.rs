//! Tests of the recent frames: how many of the last ones each path drew,
//! and the line the Diagnostics page and its report show.
use super::*;
use crate::ui::diagnostics_property_tests::Random;

/// Frames recorded from `gpu`, in order.
fn recorded(gpu: impl IntoIterator<Item = bool>) -> RecentFrames {
	let mut frames = RecentFrames::default();
	for frame in gpu {
		frames.push(frame);
	}
	frames
}

fn split(gpu: usize, software: usize) -> FrameCounts {
	FrameCounts {
		gpu,
		software,
		gpu_time: None,
	}
}

#[test]
fn nothing_drawn_counts_nothing_and_says_nothing() {
	let counts = RecentFrames::default().counts();
	assert_eq!(counts, FrameCounts::default());
	assert_eq!(counts.line(), None);
}

#[test]
fn each_frame_counts_for_the_path_that_drew_it() {
	let counts = recorded([true, false, true, true]).counts();
	assert_eq!(counts, split(3, 1));
}

#[test]
fn only_the_last_600_frames_count() {
	// 600 software frames, then 571 on the GPU: the oldest software ones
	// fall out as the GPU ones arrive.
	let counts = recorded((0..600).map(|_| false).chain((0..571).map(|_| true))).counts();
	assert_eq!(counts, split(571, 29));
}

#[test]
fn a_frame_replaces_the_oldest_whatever_drew_either() {
	let mut frames = recorded((0..KEPT).map(|_| true));
	assert_eq!(frames.counts(), split(600, 0));
	frames.push(false);
	assert_eq!(frames.counts(), split(599, 1));
	for _ in 0..KEPT {
		frames.push(false);
	}
	assert_eq!(frames.counts(), split(0, 600));
	frames.push(true);
	assert_eq!(frames.counts(), split(1, 599));
}

#[test]
fn every_position_of_the_ring_is_kept_and_replaced() {
	// One GPU frame at each position in turn among software ones: words
	// and the bits at their edges (63, 64, 599) included.
	for at in 0..KEPT {
		let mut frames = recorded((0..KEPT).map(|i| i == at));
		assert_eq!(frames.counts().gpu, 1, "at {at}");
		for _ in 0..=at {
			frames.push(false);
		}
		assert_eq!(frames.counts().gpu, 0, "at {at}");
	}
}

#[test]
fn many_laps_keep_the_counts_exact() {
	// Every third frame on the GPU over ten laps of the ring.
	let counts = recorded((0..10 * KEPT + 7).map(|i| i % 3 == 0)).counts();
	assert_eq!(counts.gpu + counts.software, KEPT);
	assert_eq!(counts.gpu, KEPT / 3);
}

#[test]
fn the_line_names_how_many_frames_it_covers() {
	let line = |gpu, software| split(gpu, software).line().unwrap();
	assert_eq!(line(571, 29), "Last 600 frames: 571 GPU \u{b7} 29 software");
	assert_eq!(line(12, 0), "Last 12 frames: 12 GPU \u{b7} 0 software");
	assert_eq!(line(0, 1), "Last frame: 0 GPU \u{b7} 1 software");
}

#[test]
fn the_ring_agrees_with_a_queue_of_the_last_frames() {
	// Differential: a plain queue of the last KEPT frames, against the
	// ring, after every frame of runs that mix the paths in long and short
	// streaks.
	use crate::ui::diagnostics_property_tests::Random;
	use std::collections::VecDeque;
	let mut random = Random(0x2545_f491_4f6c_dd1d);
	for case in 0..20 {
		let (mut ring, mut queue) = (RecentFrames::default(), VecDeque::new());
		let streak = 1 + random.below(200);
		let mut gpu = random.below(2) == 1;
		for frame in 0..random.below(3 * KEPT) {
			if random.below(streak) == 0 {
				gpu = !gpu;
			}
			ring.push(gpu);
			queue.push_back(gpu);
			if queue.len() > KEPT {
				queue.pop_front();
			}
			let on_gpu = queue.iter().filter(|&&gpu| gpu).count();
			let want = split(on_gpu, queue.len() - on_gpu);
			assert_eq!(ring.counts(), want, "case {case}, frame {frame}");
		}
	}
}

#[test]
fn the_counts_never_exceed_the_frames_drawn_or_the_ring() {
	let mut frames = RecentFrames::default();
	for drawn in 1..=KEPT + 64 {
		frames.push(drawn % 7 != 0);
		let FrameCounts { gpu, software, .. } = frames.counts();
		assert_eq!(gpu + software, drawn.min(KEPT), "after {drawn}");
	}
}

#[test]
fn the_line_counts_what_it_says() {
	// The total it names is the sum of the two it splits it into.
	for (gpu, software) in [(1, 0), (0, 2), (300, 300), (599, 1)] {
		let line = split(gpu, software).line().unwrap();
		let total = gpu + software;
		let head = if total == 1 {
			"Last frame: ".to_owned()
		} else {
			format!("Last {total} frames: ")
		};
		assert!(line.starts_with(&head), "{line}");
		let tail = format!("{gpu} GPU \u{b7} {software} software");
		assert!(line.ends_with(&tail), "{line}");
	}
}

#[test]
fn gpu_times_give_the_median_and_the_worst_of_those_kept() {
	use super::GpuTime;
	use std::time::Duration;
	let us = Duration::from_micros;
	let mut frames = RecentFrames::default();
	assert_eq!(frames.counts().gpu_time, None);
	for micros in [400, 300, 1100, 350, 500] {
		frames.push_drawn(true, Some(us(micros)));
	}
	let expected = GpuTime {
		median: us(400),
		worst: us(1100),
	};
	assert_eq!(frames.counts().gpu_time, Some(expected));
	// An even count takes the lower of the middle two.
	frames.push_drawn(true, Some(us(450)));
	assert_eq!(frames.counts().gpu_time.unwrap().median, us(400));
}

#[test]
fn software_frames_and_gpu_frames_without_a_time_count_but_are_not_timed() {
	use std::time::Duration;
	let mut frames = RecentFrames::default();
	frames.push_drawn(true, Some(Duration::from_micros(500)));
	// A stale time beside a software frame is not counted.
	frames.push_drawn(false, Some(Duration::from_millis(90)));
	frames.push_drawn(true, None);
	let counts = frames.counts();
	assert_eq!((counts.gpu, counts.software), (2, 1));
	let time = counts.gpu_time.unwrap();
	assert_eq!(
		(time.median, time.worst),
		(Duration::from_micros(500), Duration::from_micros(500))
	);
}

#[test]
fn only_the_last_600_times_are_kept() {
	use std::time::Duration;
	let mut frames = RecentFrames::default();
	frames.push_drawn(true, Some(Duration::from_millis(50)));
	for _ in 0..KEPT {
		frames.push_drawn(true, Some(Duration::from_micros(200)));
	}
	// The slow first frame has been pushed out.
	let time = frames.counts().gpu_time.unwrap();
	assert_eq!(time.worst, Duration::from_micros(200));
}

#[test]
fn the_line_ends_with_the_gpu_time_in_milliseconds() {
	use super::GpuTime;
	use std::time::Duration;
	let counts = FrameCounts {
		gpu: 600,
		software: 0,
		gpu_time: Some(GpuTime {
			median: Duration::from_micros(420),
			worst: Duration::from_micros(1_149),
		}),
	};
	let line = "Last 600 frames: 600 GPU \u{b7} 0 software \u{b7} GPU 0.4 ms median, 1.1 ms worst";
	assert_eq!(counts.line().as_deref(), Some(line));
	// Without times the line is as before.
	let untimed = FrameCounts {
		gpu_time: None,
		..counts
	};
	assert_eq!(
		untimed.line().as_deref(),
		Some("Last 600 frames: 600 GPU \u{b7} 0 software")
	);
	// Slow frames keep whole milliseconds readable.
	let slow = GpuTime {
		median: Duration::from_millis(16),
		worst: Duration::from_micros(250_049),
	};
	let line = FrameCounts {
		gpu_time: Some(slow),
		..counts
	}
	.line()
	.unwrap();
	assert!(
		line.ends_with("GPU 16.0 ms median, 250.0 ms worst"),
		"{line}"
	);
}

#[test]
fn a_time_past_what_microseconds_in_32_bits_hold_is_kept_as_the_most() {
	use std::time::Duration;
	let mut frames = RecentFrames::default();
	frames.push_drawn(true, Some(Duration::from_secs(10_000)));
	let worst = frames.counts().gpu_time.unwrap().worst;
	assert_eq!(worst, Duration::from_micros(u64::from(u32::MAX)));
}

/// What the ring is checked against: every frame and every time, of which
/// the last 600 of each count, sorted afresh.
#[derive(Default)]
struct Model {
	frames: Vec<bool>,
	times: Vec<u32>,
}

impl Model {
	fn push(&mut self, gpu: bool, time: Option<Duration>) {
		self.frames.push(gpu);
		if let (true, Some(time)) = (gpu, time) {
			self
				.times
				.push(u32::try_from(time.as_micros()).unwrap_or(u32::MAX));
		}
	}

	fn counts(&self) -> FrameCounts {
		let frames = &self.frames[self.frames.len().saturating_sub(KEPT)..];
		let gpu = frames.iter().filter(|&&gpu| gpu).count();
		let mut times = self.times[self.times.len().saturating_sub(KEPT)..].to_vec();
		times.sort();
		let micros = |time: u32| Duration::from_micros(u64::from(time));
		let gpu_time = (!times.is_empty()).then(|| GpuTime {
			median: micros(times[(times.len() - 1) / 2]),
			worst: micros(times[times.len() - 1]),
		});
		FrameCounts {
			gpu,
			software: frames.len() - gpu,
			gpu_time,
		}
	}
}

#[test]
fn random_frames_count_and_time_as_a_sorting_model_does() {
	let mut random = Random(0x9e37_79b9_7f4a_7c15);
	for _ in 0..40 {
		let (mut ring, mut model) = (RecentFrames::default(), Model::default());
		// Up to three times the ring, so most runs wrap it, some twice.
		let frames = random.below(3 * KEPT + 1);
		for frame in 0..frames {
			let gpu = random.below(4) > 0;
			let time = match random.below(6) {
				0 => None,
				// Past what 32 bits of microseconds hold.
				1 => Some(Duration::from_secs(5_000 + random.below(1_000) as u64)),
				_ => Some(Duration::from_micros(random.below(40_000) as u64)),
			};
			ring.push_drawn(gpu, time);
			model.push(gpu, time);
			if frame % 97 == 0 {
				assert_eq!(ring.counts(), model.counts(), "after frame {frame}");
			}
		}
		assert_eq!(ring.counts(), model.counts(), "after {frames} frames");
	}
}

#[test]
fn random_counts_read_back_from_their_line() {
	let mut random = Random(0x2545_f491_4f6c_dd1d);
	for _ in 0..2_000 {
		let gpu = random.below(KEPT + 1);
		let software = random.below(KEPT + 1 - gpu);
		let mut time = || Duration::from_micros(random.below(50_000_000) as u64);
		let (a, b) = (time(), time());
		let timed = GpuTime {
			median: a.min(b),
			worst: a.max(b),
		};
		let gpu_time = (gpu > 0 && random.below(2) == 0).then_some(timed);
		let counts = FrameCounts {
			gpu,
			software,
			gpu_time,
		};
		let Some(line) = counts.line() else {
			assert_eq!(gpu + software, 0);
			continue;
		};
		let parts: Vec<&str> = line.split(" \u{b7} ").collect();
		let frames = match gpu + software {
			1 => "Last frame".to_owned(),
			total => format!("Last {total} frames"),
		};
		assert_eq!(parts[0], format!("{frames}: {gpu} GPU"), "{line}");
		assert_eq!(parts[1], format!("{software} software"), "{line}");
		let Some(time) = gpu_time else {
			assert_eq!(parts.len(), 2, "{line}");
			continue;
		};
		assert_eq!(parts.len(), 3, "{line}");
		let shown = parts[2]
			.strip_prefix("GPU ")
			.and_then(|s| s.strip_suffix(" ms worst"));
		let (median, worst) = shown.and_then(|s| s.split_once(" ms median, ")).unwrap();
		for (shown, time) in [(median, time.median), (worst, time.worst)] {
			// Milliseconds to one place, rounded.
			assert_eq!(
				shown.split_once('.').map(|(_, places)| places.len()),
				Some(1)
			);
			let ms: f64 = shown.parse().unwrap();
			assert!(
				(ms - time.as_secs_f64() * 1e3).abs() <= 0.05 + 1e-9,
				"{line}"
			);
		}
	}
}
