//! Paste markers inside pasted text: every one is removed, however they
//! nest, in time that grows with the text and not with its square, and the
//! rest of the text is kept.
use super::paste::paste_bytes;
use crate::bench::corpus::Rng;
use std::time::{Duration, Instant};

const START: &str = "\x1b[200~";
const END: &str = "\x1b[201~";

/// The text between the markers `paste_bytes` adds.
fn inner(text: &str) -> String {
	let bytes = paste_bytes(text, true);
	let text = String::from_utf8(bytes).unwrap();
	text
		.strip_prefix(START)
		.and_then(|t| t.strip_suffix(END))
		.unwrap()
		.to_owned()
}

/// `depth` end markers, each split by the one inside it: removing the
/// innermost forms the next.
fn nested(depth: usize) -> String {
	format!("{}{END}{}", "\x1b[20".repeat(depth), "1~".repeat(depth))
}

#[test]
fn markers_nested_deep_are_all_removed_quickly() {
	for depth in [1, 2, 10] {
		assert_eq!(inner(&nested(depth)), "", "depth {depth}");
	}
	// Over a megabyte of nesting: repeated passes would take hours.
	let start = Instant::now();
	assert_eq!(inner(&format!("a{}b", nested(150_000))), "ab");
	assert!(
		start.elapsed() < Duration::from_secs(5),
		"took {:?}",
		start.elapsed()
	);
}

#[test]
fn start_markers_nest_too_and_both_kinds_mix() {
	let start_nested = format!("{}{START}{}", "\x1b[20".repeat(3), "0~".repeat(3));
	assert_eq!(inner(&start_nested), "");
	assert_eq!(
		inner(&format!("x\x1b[2{START}00~{END}y")),
		"xy",
		"a start marker split by both"
	);
	assert_eq!(inner(&format!("{END}{END}ls{START}")), "ls");
}

#[test]
fn text_that_only_looks_like_a_marker_is_kept() {
	for text in [
		"\x1b[200",
		"\x1b[20~",
		"[201~",
		"\x1b[2001~",
		"\x1b[201 ~",
		"~~~ \x1b [",
	] {
		assert_eq!(inner(text), text, "{text:?}");
	}
}

#[test]
fn whatever_is_pasted_no_marker_survives_and_other_text_stays() {
	let pieces = [
		"\x1b",
		"[",
		"20",
		"0",
		"1",
		"~",
		"a",
		"é",
		"\n",
		"\x1b[200~",
		"\x1b[201~",
	];
	let mut rng = Rng::new(52);
	for _ in 0..3000 {
		let text: String = (0..rng.below(30)).map(|_| *rng.pick(&pieces)).collect();
		let out = inner(&text);
		assert!(
			!out.contains(START) && !out.contains(END),
			"{text:?} gave {out:?}"
		);
		// Only markers go: the letters a and é stay, in order.
		let kept = |s: &str| {
			s.chars()
				.filter(|c| matches!(c, 'a' | 'é'))
				.collect::<String>()
		};
		assert_eq!(kept(&out), kept(&text), "{text:?}");
		// Line endings become carriage returns either way.
		assert!(!out.contains('\n'));
	}
}

#[test]
fn without_bracketed_paste_the_text_is_sent_as_it_is() {
	let text = format!("a{END}b\r\nc\n");
	assert_eq!(
		paste_bytes(&text, false),
		format!("a{END}b\rc\r").into_bytes()
	);
}
