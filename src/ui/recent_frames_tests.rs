//! Tests of the recent frames: how many of the last ones each path drew,
//! and the line the Diagnostics page and its report show.
use super::*;

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
