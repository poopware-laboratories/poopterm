use std::time::{Duration, Instant};

use super::*;

fn ms(n: u64) -> Duration {
	Duration::from_millis(n)
}

fn timing(paint: u64, show: u64, total: u64) -> [Duration; 3] {
	[paint, show, total].map(Duration::from_micros)
}

#[test]
fn frames_are_counted_by_how_they_were_drawn() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	stats.frame(Kind::Gpu, timing(1, 2, 3), now);
	stats.frame(Kind::Gpu, timing(1, 2, 3), now);
	stats.frame(Kind::Software, timing(1, 2, 3), now);
	assert_eq!(stats.kinds, [1, 2], "software, gpu");
	assert_eq!(stats.total.len(), 3);
}

#[test]
fn latency_runs_from_a_key_to_the_first_frame_after_its_echo() {
	let mut stats = FrameStats::default();
	let key = Instant::now();
	stats.key(key);
	stats.frame(Kind::Gpu, timing(1, 1, 1), key + ms(2));
	assert!(
		stats.latency.is_empty(),
		"a frame before the echo is not the answer"
	);
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), key + ms(9));
	assert_eq!(stats.latency, [ms(9)]);
	stats.frame(Kind::Gpu, timing(1, 1, 1), key + ms(20));
	assert_eq!(stats.latency, [ms(9)], "measured once");
}

#[test]
fn output_without_a_key_measures_nothing() {
	let mut stats = FrameStats::default();
	stats.output();
	stats.frame(Kind::Software, timing(1, 1, 1), Instant::now());
	assert!(stats.latency.is_empty());
	// Output seen before the key does not count as its echo.
	let key = Instant::now();
	stats.key(key);
	stats.frame(Kind::Software, timing(1, 1, 1), key + ms(3));
	assert!(stats.latency.is_empty());
}

#[test]
fn keys_typed_before_the_echo_are_timed_from_the_first() {
	let mut stats = FrameStats::default();
	let first = Instant::now();
	stats.key(first);
	stats.key(first + ms(4));
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), first + ms(10));
	assert_eq!(stats.latency, [ms(10)]);
	// The next key starts a new measurement.
	stats.key(first + ms(50));
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), first + ms(56));
	assert_eq!(stats.latency, [ms(10), ms(6)]);
}

#[test]
fn samples_stop_at_the_limit_but_frames_are_still_counted() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	for _ in 0..MAX_SAMPLES + 3 {
		stats.frame(Kind::Gpu, timing(1, 1, 1), now);
	}
	assert_eq!(stats.total.len(), MAX_SAMPLES);
	assert_eq!(stats.paint.len(), MAX_SAMPLES);
	assert_eq!(stats.show.len(), MAX_SAMPLES);
	assert_eq!(
		stats.kinds[1],
		MAX_SAMPLES + 3,
		"every frame is still counted"
	);
}

#[test]
fn latencies_stop_at_the_limit_too() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	for _ in 0..MAX_SAMPLES + 1 {
		stats.key(now);
		stats.output();
		stats.frame(Kind::Gpu, timing(1, 1, 1), now + ms(1));
	}
	assert_eq!(stats.latency.len(), MAX_SAMPLES);
}

#[test]
fn the_report_is_json_with_every_sample_in_microseconds_to_a_tenth() {
	let mut stats = FrameStats::default();
	let key = Instant::now();
	stats.frame(Kind::Software, timing(1500, 250, 2000), key);
	stats.key(key);
	stats.output();
	stats.frame(
		Kind::Gpu,
		timing(500, 750, 1000),
		key + Duration::from_micros(8_123),
	);
	let report = stats.report((1000, 600), 2.0);
	let expected = concat!(
		"{\"frames\": {\"software\": 1, \"gpu\": 1}, ",
		"\"size\": [1000, 600], \"scale\": 2, ",
		"\"paint_us\": [1500.0, 500.0], \"show_us\": [250.0, 750.0], ",
		"\"total_us\": [2000.0, 1000.0], \"latency_us\": [8123.0]}",
	);
	assert_eq!(report, expected);
}

#[test]
fn a_report_without_samples_has_empty_lists_and_a_fractional_scale() {
	let report = FrameStats::default().report((0, 0), 1.25);
	assert_eq!(
		report,
		concat!(
			"{\"frames\": {\"software\": 0, \"gpu\": 0}, ",
			"\"size\": [0, 0], \"scale\": 1.25, ",
			"\"paint_us\": [], \"show_us\": [], \"total_us\": [], \"latency_us\": []}",
		)
	);
}

#[test]
fn sub_microsecond_times_keep_their_tenths() {
	let mut stats = FrameStats::default();
	let quick = [
		Duration::from_nanos(1_250),
		Duration::ZERO,
		Duration::from_nanos(99),
	];
	stats.frame(Kind::Software, quick, Instant::now());
	let report = stats.report((1, 1), 1.0);
	assert!(report.contains("\"paint_us\": [1.2]"), "{report}");
	assert!(report.contains("\"show_us\": [0.0]"), "{report}");
	assert!(report.contains("\"total_us\": [0.1]"), "{report}");
}

/// The numbers of the report's list `name`.
fn listed(report: &str, name: &str) -> Vec<f64> {
	let start = report.find(&format!("\"{name}\": [")).unwrap() + name.len() + 5;
	let list = &report[start..start + report[start..].find(']').unwrap()];
	list
		.split(", ")
		.filter(|n| !n.is_empty())
		.map(|n| n.parse().unwrap())
		.collect()
}

#[test]
fn every_sample_is_reported_once_in_order_to_a_tenth_of_a_microsecond() {
	let mut stats = FrameStats::default();
	let key = Instant::now();
	// Uneven times, from under a microsecond to over a second.
	let times: Vec<_> = (0..500u64)
		.map(|n| Duration::from_nanos(n * n * 7_919 + n))
		.collect();
	for (n, &time) in times.iter().enumerate() {
		stats.key(key);
		stats.output();
		stats.frame(Kind::Software, [time, time / 2, time * 3], key + time);
		assert_eq!(stats.latency.len(), n + 1);
	}
	let report = stats.report((1, 1), 1.0);
	assert!(!report.contains('\n'), "one line");
	for (name, scale) in [
		("paint_us", 1.0),
		("show_us", 0.5),
		("total_us", 3.0),
		("latency_us", 1.0),
	] {
		let numbers = listed(&report, name);
		assert_eq!(numbers.len(), times.len(), "{name}");
		for (number, time) in numbers.iter().zip(&times) {
			let micros = time.as_nanos() as f64 * scale / 1e3;
			assert!(
				(number - micros).abs() <= 0.051,
				"{name}: {number} for {micros}"
			);
		}
	}
}

#[test]
fn long_times_are_not_cut_short() {
	let mut stats = FrameStats::default();
	stats.frame(Kind::Software, [Duration::from_secs(90); 3], Instant::now());
	assert!(
		stats
			.report((1, 1), 1.0)
			.contains("\"total_us\": [90000000.0]")
	);
}

#[test]
fn typing_is_still_timed_once_frame_samples_are_full() {
	// A long flood fills the frame samples; the limits are separate.
	let mut stats = FrameStats::default();
	let now = Instant::now();
	for _ in 0..MAX_SAMPLES {
		stats.frame(Kind::Gpu, timing(1, 1, 1), now);
	}
	stats.key(now);
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), now + ms(7));
	assert_eq!(stats.total.len(), MAX_SAMPLES);
	assert_eq!(stats.latency, [ms(7)]);
	assert_eq!(stats.kinds[1], MAX_SAMPLES + 1);
}

#[test]
fn a_key_typed_after_the_echo_waits_for_the_frame_to_answer_the_first() {
	let mut stats = FrameStats::default();
	let first = Instant::now();
	stats.key(first);
	stats.output();
	// Typed after the echo but before the frame showing it: no new start.
	stats.key(first + ms(3));
	stats.frame(Kind::Software, timing(1, 1, 1), first + ms(5));
	assert_eq!(stats.latency, [ms(5)]);
	// Its echo was not seen yet, so it measures nothing more.
	stats.frame(Kind::Software, timing(1, 1, 1), first + ms(9));
	assert_eq!(stats.latency, [ms(5)]);
}

#[test]
fn output_seen_twice_is_one_echo() {
	let mut stats = FrameStats::default();
	let key = Instant::now();
	stats.key(key);
	stats.output();
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), key + ms(4));
	stats.frame(Kind::Gpu, timing(1, 1, 1), key + ms(8));
	assert_eq!(stats.latency, [ms(4)]);
}

#[test]
fn a_frame_ending_before_its_key_measures_zero_not_a_panic() {
	let mut stats = FrameStats::default();
	let key = Instant::now() + ms(10);
	stats.key(key);
	stats.output();
	stats.frame(Kind::Gpu, timing(1, 1, 1), key - ms(5));
	assert_eq!(stats.latency, [ms(0)]);
}

#[test]
fn samples_keep_the_order_frames_were_drawn_in() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	for n in [3, 1, 2] {
		stats.frame(Kind::Software, timing(n, n * 10, n * 100), now);
	}
	let micros = |samples: &[Duration]| samples.iter().map(Duration::as_micros).collect::<Vec<_>>();
	assert_eq!(micros(&stats.paint), [3, 1, 2]);
	assert_eq!(micros(&stats.show), [30, 10, 20]);
	assert_eq!(micros(&stats.total), [300, 100, 200]);
}

mod recorder {
	use super::*;

	fn recorder() -> Recorder {
		Recorder::from_env(Some("/tmp/stats.json".into())).unwrap()
	}

	#[test]
	fn it_records_only_when_the_variable_names_a_file() {
		assert!(Recorder::from_env(None).is_none());
		assert!(
			Recorder::from_env(Some("".into())).is_none(),
			"an empty variable is unset"
		);
		assert_eq!(recorder().path, std::path::Path::new("/tmp/stats.json"));
	}

	#[test]
	fn a_software_frame_shows_for_as_long_as_its_presenter_took() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(4));
		recorder.finish(false, ms(2), start, start + ms(7));
		assert_eq!(recorder.stats.kinds, [1, 0]);
		assert_eq!(recorder.stats.paint, [ms(4)]);
		assert_eq!(recorder.stats.show, [ms(2)]);
		assert_eq!(recorder.stats.total, [ms(7)]);
	}

	#[test]
	fn a_gpu_frame_shows_for_what_its_drawing_took_beyond_painting() {
		// The GPU's frame call paints the chrome in software, then draws.
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(3));
		recorder.finish(true, ms(5), start, start + ms(6));
		assert_eq!(recorder.stats.kinds, [0, 1]);
		assert_eq!(
			(recorder.stats.paint[0], recorder.stats.show[0]),
			(ms(3), ms(2))
		);
	}

	#[test]
	fn each_frame_starts_from_nothing_painted() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(4));
		recorder.finish(true, ms(5), start, start + ms(5));
		recorder.finish(true, ms(2), start, start + ms(2));
		assert_eq!(recorder.stats.kinds, [0, 2]);
		assert_eq!(recorder.stats.paint, [ms(4), ms(0)]);
		assert_eq!(recorder.stats.show, [ms(1), ms(2)]);
	}

	#[test]
	fn painting_longer_than_the_frame_call_shows_for_zero() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(9));
		recorder.finish(true, ms(5), start, start + ms(9));
		assert_eq!(recorder.stats.show, [ms(0)]);
	}

	#[test]
	fn keys_and_output_reach_the_latency_measure() {
		let mut recorder = recorder();
		let key = Instant::now();
		recorder.stats.key(key);
		recorder.stats.output();
		recorder.finish(true, ms(1), key, key + ms(12));
		assert_eq!(recorder.stats.latency, [ms(12)]);
	}

	#[test]
	fn saving_writes_the_report_and_a_newline_to_its_file() {
		let path = std::env::temp_dir().join(format!("poopterm-recorder-{}.json", std::process::id()));
		let mut recorder = Recorder::from_env(Some(path.clone().into_os_string())).unwrap();
		let start = Instant::now();
		recorder.finish(false, ms(1), start, start + ms(1));
		recorder.save((80, 24), 1.0).unwrap();
		let written = std::fs::read_to_string(&path).unwrap();
		assert_eq!(
			written,
			format!("{}\n", recorder.stats.report((80, 24), 1.0))
		);
		assert!(written.contains("\"size\": [80, 24]"), "{written}");
		std::fs::remove_file(path).unwrap();
	}

	#[test]
	fn a_path_that_is_not_utf8_is_kept_byte_for_byte() {
		use std::os::unix::ffi::OsStringExt;
		let bytes = b"/tmp/stats-\xff\xfe.json".to_vec();
		let recorder = Recorder::from_env(Some(OsString::from_vec(bytes.clone()))).unwrap();
		assert_eq!(recorder.path.into_os_string().into_vec(), bytes);
	}

	#[test]
	fn software_painted_in_several_parts_adds_up() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(2));
		recorder.painted(ms(3));
		recorder.finish(false, ms(1), start, start + ms(6));
		assert_eq!(recorder.stats.paint, [ms(5)]);
		assert_eq!(
			recorder.stats.show,
			[ms(1)],
			"software shows for what its presenter took"
		);
	}

	#[test]
	fn a_window_falling_back_to_software_counts_each_frame_as_it_was_drawn() {
		let mut recorder = recorder();
		let start = Instant::now();
		// A GPU frame painting in two parts, then software from then on.
		recorder.painted(ms(1));
		recorder.painted(ms(2));
		recorder.finish(true, ms(7), start, start + ms(8));
		recorder.finish(false, ms(4), start, start + ms(4));
		recorder.finish(false, ms(3), start, start + ms(3));
		assert_eq!(recorder.stats.kinds, [2, 1]);
		assert_eq!(recorder.stats.paint, [ms(3), ms(0), ms(0)]);
		assert_eq!(recorder.stats.show, [ms(4), ms(4), ms(3)]);
	}

	#[test]
	fn a_frame_ending_before_it_started_takes_zero() {
		let mut recorder = recorder();
		let start = Instant::now() + ms(10);
		recorder.finish(false, ms(1), start, start - ms(1));
		assert_eq!(recorder.stats.total, [ms(0)]);
	}

	#[test]
	fn a_report_that_cannot_be_written_is_an_error() {
		let recorder = Recorder::from_env(Some("/nonexistent-dir/stats.json".into())).unwrap();
		assert!(recorder.save((1, 1), 1.0).is_err());
	}
}
