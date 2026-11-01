use std::time::{Duration, Instant};

use super::*;

fn ms(n: u64) -> Duration {
	Duration::from_millis(n)
}

fn timing(paint: u64, show: u64, total: u64) -> Timing {
	Timing {
		paint: Duration::from_micros(paint),
		show: Duration::from_micros(show),
		total: Duration::from_micros(total),
	}
}

#[test]
fn an_empty_list_has_no_spread() {
	assert_eq!(spread(&mut []), None);
}

#[test]
fn one_sample_is_its_own_median_percentile_and_bounds() {
	let spread = spread(&mut [ms(7)]).unwrap();
	assert_eq!(
		spread,
		Spread {
			count: 1,
			median: ms(7),
			p95: ms(7),
			min: ms(7),
			max: ms(7)
		}
	);
}

#[test]
fn the_median_and_95th_percentile_are_nearest_ranks_of_the_sorted_samples() {
	// 1..=20 ms out of order: rank ceil(0.5 × 20) = 10, ceil(0.95 × 20) = 19.
	let mut samples: Vec<_> = (1..=20).rev().map(ms).collect();
	let spread = spread(&mut samples).unwrap();
	assert_eq!((spread.median, spread.p95), (ms(10), ms(19)));
	assert_eq!((spread.min, spread.max, spread.count), (ms(1), ms(20), 20));
}

#[test]
fn an_odd_count_takes_the_middle_sample() {
	let spread = spread(&mut [ms(5), ms(1), ms(9)]).unwrap();
	assert_eq!(spread.median, ms(5));
	assert_eq!(spread.p95, ms(9), "rank ceil(2.85) = 3");
}

#[test]
fn frames_are_counted_by_how_they_were_drawn() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	stats.frame(Kind::Gpu, timing(1, 2, 3), now);
	stats.frame(Kind::Gpu, timing(1, 2, 3), now);
	stats.frame(Kind::GpuWhole, timing(1, 2, 3), now);
	stats.frame(Kind::Software, timing(1, 2, 3), now);
	assert_eq!(stats.kinds, [1, 2, 1], "software, gpu, gpu painted whole");
	assert_eq!(stats.total.len(), 4);
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
fn samples_stop_at_the_limit_and_count_what_was_dropped() {
	let mut stats = FrameStats::default();
	let now = Instant::now();
	for _ in 0..MAX_SAMPLES + 3 {
		stats.frame(Kind::Gpu, timing(1, 1, 1), now);
	}
	assert_eq!(stats.total.len(), MAX_SAMPLES);
	assert_eq!(stats.paint.len(), MAX_SAMPLES);
	assert_eq!(stats.dropped, 3);
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
fn the_report_is_json_with_microseconds_to_a_tenth() {
	let mut stats = FrameStats::default();
	let key = Instant::now();
	stats.frame(Kind::GpuWhole, timing(1500, 250, 2000), key);
	stats.key(key);
	stats.output();
	stats.frame(
		Kind::Gpu,
		timing(500, 750, 1000),
		key + Duration::from_micros(8_123),
	);
	let report = stats.report((1000, 600), 2.0);
	let expected = concat!(
		"{\"frames\": {\"software\": 0, \"gpu\": 1, \"gpu_whole\": 1}, ",
		"\"size\": [1000, 600], \"scale\": 2, \"dropped\": 0, ",
		"\"paint_us\": {\"count\": 2, \"median\": 500.0, \"p95\": 1500.0, \"min\": 500.0, \"max\": 1500.0}, ",
		"\"show_us\": {\"count\": 2, \"median\": 250.0, \"p95\": 750.0, \"min\": 250.0, \"max\": 750.0}, ",
		"\"total_us\": {\"count\": 2, \"median\": 1000.0, \"p95\": 2000.0, \"min\": 1000.0, \"max\": 2000.0}, ",
		"\"latency_us\": {\"count\": 1, \"median\": 8123.0, \"p95\": 8123.0, \"min\": 8123.0, \"max\": 8123.0}}",
	);
	assert_eq!(report, expected);
}

#[test]
fn a_report_without_samples_has_null_spreads_and_a_fractional_scale() {
	let report = FrameStats::default().report((0, 0), 1.25);
	assert_eq!(
		report,
		concat!(
			"{\"frames\": {\"software\": 0, \"gpu\": 0, \"gpu_whole\": 0}, ",
			"\"size\": [0, 0], \"scale\": 1.25, \"dropped\": 0, ",
			"\"paint_us\": null, \"show_us\": null, \"total_us\": null, \"latency_us\": null}",
		)
	);
}

#[test]
fn sub_microsecond_times_keep_their_tenths() {
	let mut stats = FrameStats::default();
	let quick = Timing {
		paint: Duration::from_nanos(1_250),
		show: Duration::ZERO,
		total: Duration::from_nanos(99),
	};
	stats.frame(Kind::Software, quick, Instant::now());
	let report = stats.report((1, 1), 1.0);
	assert!(
		report.contains("\"paint_us\": {\"count\": 1, \"median\": 1.2,"),
		"{report}"
	);
	assert!(
		report.contains("\"show_us\": {\"count\": 1, \"median\": 0.0,"),
		"{report}"
	);
	assert!(
		report.contains("\"total_us\": {\"count\": 1, \"median\": 0.1,"),
		"{report}"
	);
}

#[test]
fn the_report_goes_where_the_variable_points() {
	let path = std::env::temp_dir().join(format!("poopterm-frame-stats-{}.json", std::process::id()));
	let target = destination(Some(path.clone().into_os_string()));
	assert_eq!(target.as_deref(), Some(path.as_path()));
	assert_eq!(destination(None), None);
	assert_eq!(
		destination(Some("".into())),
		None,
		"an empty variable is unset"
	);
	let mut stats = FrameStats::default();
	stats.frame(Kind::Gpu, timing(1, 1, 1), Instant::now());
	write_report(&stats, &path, (10, 20), 1.0).unwrap();
	let written = std::fs::read_to_string(&path).unwrap();
	assert_eq!(written, format!("{}\n", stats.report((10, 20), 1.0)));
	std::fs::remove_file(path).unwrap();
}

#[test]
fn a_report_that_cannot_be_written_is_an_error() {
	let path = std::path::Path::new("/nonexistent-dir/stats.json");
	assert!(write_report(&FrameStats::default(), path, (1, 1), 1.0).is_err());
}

mod recorder {
	use super::*;

	fn recorder() -> Recorder {
		Recorder::from_env(Some("/tmp/stats.json".into())).unwrap()
	}

	#[test]
	fn it_records_only_when_the_variable_names_a_file() {
		assert!(Recorder::from_env(None).is_none());
		assert!(Recorder::from_env(Some("".into())).is_none());
		assert_eq!(recorder().path, std::path::Path::new("/tmp/stats.json"));
	}

	#[test]
	fn a_software_frame_shows_for_as_long_as_its_presenter_took() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(4), false);
		recorder.finish(false, ms(2), start, start + ms(7));
		assert_eq!(recorder.stats.kinds, [1, 0, 0]);
		assert_eq!(recorder.stats.paint, [ms(4)]);
		assert_eq!(recorder.stats.show, [ms(2)]);
		assert_eq!(recorder.stats.total, [ms(7)]);
	}

	#[test]
	fn a_gpu_frame_shows_for_what_its_drawing_took_beyond_painting() {
		// The GPU's frame call paints the chrome in software, then draws.
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(3), false);
		recorder.finish(true, ms(5), start, start + ms(6));
		assert_eq!(recorder.stats.kinds, [0, 1, 0]);
		assert_eq!(
			(recorder.stats.paint[0], recorder.stats.show[0]),
			(ms(3), ms(2))
		);
	}

	#[test]
	fn a_gpu_frame_painted_whole_by_software_is_counted_apart() {
		let mut recorder = recorder();
		let start = Instant::now();
		// Refused first (chrome painted), then painted whole: both count.
		recorder.painted(ms(1), false);
		recorder.painted(ms(4), true);
		recorder.finish(true, ms(9), start, start + ms(10));
		assert_eq!(recorder.stats.kinds, [0, 0, 1]);
		assert_eq!(
			(recorder.stats.paint[0], recorder.stats.show[0]),
			(ms(5), ms(4))
		);
	}

	#[test]
	fn each_frame_starts_from_nothing_painted() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(4), true);
		recorder.finish(true, ms(5), start, start + ms(5));
		recorder.finish(true, ms(2), start, start + ms(2));
		assert_eq!(
			recorder.stats.kinds,
			[0, 1, 1],
			"the second frame was not whole"
		);
		assert_eq!(recorder.stats.paint, [ms(4), ms(0)]);
		assert_eq!(recorder.stats.show, [ms(1), ms(2)]);
	}

	#[test]
	fn painting_longer_than_the_frame_call_shows_for_zero() {
		let mut recorder = recorder();
		let start = Instant::now();
		recorder.painted(ms(9), false);
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
	fn saving_writes_the_report_to_its_file() {
		let path = std::env::temp_dir().join(format!("poopterm-recorder-{}.json", std::process::id()));
		let mut recorder = Recorder::from_env(Some(path.clone().into_os_string())).unwrap();
		let start = Instant::now();
		recorder.finish(false, ms(1), start, start + ms(1));
		recorder.save((80, 24), 1.0).unwrap();
		let written = std::fs::read_to_string(&path).unwrap();
		assert!(
			written.starts_with("{\"frames\": {\"software\": 1,"),
			"{written}"
		);
		assert!(written.contains("\"size\": [80, 24]"), "{written}");
		std::fs::remove_file(path).unwrap();
	}
}
