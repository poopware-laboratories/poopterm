//! Tests of the warning ring: its bounds, how it cleans lines, and the
//! process-wide ring that `warning!` feeds.
use super::warnings::{KEPT, LONGEST, Ring, Warning, clean, recent, warning};
use std::time::Duration;

fn texts(ring: &Ring) -> Vec<&str> {
	ring
		.warnings()
		.map(|warning| warning.text.as_str())
		.collect()
}

#[test]
fn a_new_ring_is_empty() {
	let ring = Ring::default();
	assert_eq!(ring.warnings().count(), 0);
	assert_eq!(ring.dropped(), 0);
}

#[test]
fn warnings_come_back_oldest_first_with_their_times() {
	let mut ring = Ring::default();
	ring.push(Duration::from_secs(3), "first");
	ring.push(Duration::from_secs(9), "second");
	let kept: Vec<Warning> = ring.warnings().cloned().collect();
	assert_eq!(
		kept,
		[
			Warning {
				at: Duration::from_secs(3),
				text: "first".into()
			},
			Warning {
				at: Duration::from_secs(9),
				text: "second".into()
			},
		]
	);
}

#[test]
fn the_ring_holds_exactly_its_limit() {
	let mut ring = Ring::default();
	for i in 0..KEPT {
		ring.push(Duration::ZERO, &i.to_string());
	}
	assert_eq!(ring.warnings().count(), KEPT);
	assert_eq!(ring.dropped(), 0);
	assert_eq!(texts(&ring)[0], "0");
}

#[test]
fn past_its_limit_the_oldest_go_and_are_counted() {
	let mut ring = Ring::default();
	for i in 0..KEPT + 3 {
		ring.push(Duration::ZERO, &i.to_string());
	}
	assert_eq!(ring.warnings().count(), KEPT);
	assert_eq!(ring.dropped(), 3);
	let kept = texts(&ring);
	assert_eq!(kept[0], "3");
	assert_eq!(kept[KEPT - 1], (KEPT + 2).to_string());
}

#[test]
fn clean_leaves_ordinary_text_alone() {
	assert_eq!(
		clean("font: no glyph for U+1FAE8 — ünïcödé"),
		"font: no glyph for U+1FAE8 — ünïcödé"
	);
	assert_eq!(clean(""), "");
}

#[test]
fn control_characters_cannot_reach_the_page_or_the_report() {
	// A path or URL from a program could carry escapes or line breaks.
	assert_eq!(
		clean("a\nb\rc\td\u{1b}[31me\u{7f}f\u{9b}g"),
		"a�b�c�d�[31me�f�g"
	);
}

#[test]
fn a_line_at_the_limit_is_kept_whole() {
	let line = "x".repeat(LONGEST);
	assert_eq!(clean(&line), line);
}

#[test]
fn a_longer_line_is_cut_to_the_limit_with_an_ellipsis() {
	let cut = clean(&"x".repeat(LONGEST + 1));
	assert_eq!(cut.len(), LONGEST);
	assert!(cut.ends_with('…'));
	assert_eq!(
		cut.trim_end_matches('…'),
		"x".repeat(LONGEST - '…'.len_utf8())
	);
}

#[test]
fn cutting_never_splits_a_character() {
	// Two-, three- and four-byte characters straddling the limit.
	for wide in ["é", "€", "🦀"] {
		for pad in 0..4 {
			let text = format!("{}{}", "x".repeat(pad), wide.repeat(LONGEST));
			let cut = clean(&text);
			assert!(cut.len() <= LONGEST, "{wide} {pad}: {}", cut.len());
			assert!(
				cut.len() > LONGEST - 4 - '…'.len_utf8(),
				"{wide} {pad}: {}",
				cut.len()
			);
			assert!(cut.ends_with('…'));
		}
	}
}

#[test]
fn the_ring_stores_lines_cleaned_and_bounded() {
	let mut ring = Ring::default();
	ring.push(
		Duration::ZERO,
		&format!("bad\u{1b}]52;c;\u{7}{}", "y".repeat(4 * LONGEST)),
	);
	let text = &ring.warnings().next().unwrap().text;
	assert!(text.starts_with("bad�]52;c;�y"));
	assert_eq!(text.len(), LONGEST);
}

#[test]
fn warning_records_in_the_process_wide_ring() {
	// Other tests may warn at the same time: look for this one's own line.
	let marker = "warnings_tests: recorded 6c1f";
	warning!("{marker} {}", 42);
	let (kept, _) = recent();
	let found = kept
		.iter()
		.filter(|warning| warning.text == format!("{marker} 42"));
	assert_eq!(found.count(), 1);
}

#[test]
fn recent_is_a_copy_that_later_warnings_do_not_change() {
	let marker = "warnings_tests: snapshot 91d3";
	let (before, _) = recent();
	warning!("{marker}");
	assert!(!before.iter().any(|warning| warning.text == marker));
	assert!(recent().0.iter().any(|warning| warning.text == marker));
}

#[test]
fn warnings_from_many_threads_are_all_kept() {
	// The PTY, parser, portal and UI threads all warn.
	let threads: Vec<_> = (0..8)
		.map(|thread| {
			std::thread::spawn(move || {
				for i in 0..10 {
					warning!("warnings_tests: thread e7a2 {thread}.{i}");
				}
			})
		})
		.collect();
	for thread in threads {
		thread.join().unwrap();
	}
	let (kept, _) = recent();
	let ours: Vec<&str> = kept
		.iter()
		.filter_map(|warning| warning.text.strip_prefix("warnings_tests: thread e7a2 "))
		.collect();
	// Other tests warn too, but never 256 times in between.
	assert_eq!(ours.len(), 80);
	for thread in 0..8 {
		let order: Vec<&str> = ours
			.iter()
			.filter(|line| line.starts_with(&format!("{thread}.")))
			.copied()
			.collect();
		let want: Vec<String> = (0..10).map(|i| format!("{thread}.{i}")).collect();
		assert_eq!(order, want, "each thread's warnings stay in its order");
	}
}

#[test]
fn a_warning_keeps_its_formatting_and_is_bounded_and_cleaned() {
	let marker = "warnings_tests: formatted 3b8e";
	warning!("{marker} {{literal}} {:>4}|{:.2}|{}", 7, 1.0 / 3.0, "a\nb");
	warning!("{marker} long {}", "z".repeat(10 * LONGEST));
	let (kept, _) = recent();
	let ours: Vec<&String> = kept
		.iter()
		.map(|warning| &warning.text)
		.filter(|text| text.starts_with(marker))
		.collect();
	assert_eq!(ours[0], &format!("{marker} {{literal}}    7|0.33|a�b"));
	assert_eq!(ours[1].len(), LONGEST);
	assert!(ours[1].ends_with("zz…"));
}

#[test]
fn a_warning_is_stamped_with_the_time_it_was_made() {
	let marker = "warnings_tests: stamped 55c0";
	let now = || {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
	};
	let before = now();
	warning!("{marker}");
	let after = now();
	let (kept, _) = recent();
	let at = kept
		.iter()
		.find(|warning| warning.text == marker)
		.unwrap()
		.at;
	assert!(before <= at && at <= after, "{before:?} {at:?} {after:?}");
}
