//! Properties of quick select's model that hold for any screen: labels are
//! unique, prefix-free and pick their own text for any number of targets,
//! and targets found on generated rows are exactly the words put there.
use super::hints::{Hint, MAX_HINTS, Pick, Target, find_targets, label, pick};
use crate::bench::corpus::Rng;

/// `count` targets with distinct texts, one a row.
fn distinct(count: usize) -> Vec<Target> {
	(0..count)
		.map(|row| Target {
			row,
			cols: (0, 3),
			text: format!("/t{row}"),
		})
		.collect()
}

fn check_labels(hints: &[Hint], texts: usize) {
	let shown = texts.min(MAX_HINTS);
	assert_eq!(hints.len(), shown, "{texts} texts");
	let width = if shown > 26 { 2 } else { 1 };
	for (i, a) in hints.iter().enumerate() {
		assert_eq!(a.label.len(), width, "{texts} texts: {a:?}");
		assert!(a.label.bytes().all(|b| b.is_ascii_lowercase()), "{a:?}");
		for b in &hints[i + 1..] {
			assert!(
				!b.label.starts_with(&a.label) && !a.label.starts_with(&b.label),
				"{a:?} {b:?}"
			);
		}
		assert_eq!(
			pick(hints, &a.label),
			Pick::Chosen(&a.target.text),
			"{texts} texts"
		);
		assert_eq!(
			pick(hints, &a.label.to_uppercase()),
			Pick::Chosen(&a.target.text)
		);
	}
}

#[test]
fn labels_are_unique_prefix_free_and_pick_their_own_text_for_any_count() {
	for texts in (1..=60).chain([100, 300, MAX_HINTS - 1, MAX_HINTS, MAX_HINTS + 1, 1000]) {
		check_labels(&label(distinct(texts)), texts);
	}
}

#[test]
fn every_start_of_a_label_narrows_to_the_labels_that_share_it() {
	let hints = label(distinct(200));
	for hint in &hints {
		let first = &hint.label[..1];
		let Pick::Narrowed(left) = pick(&hints, first) else {
			panic!("{first} chose before its second letter");
		};
		assert!(left.iter().any(|h| h.label == hint.label));
		assert!(left.iter().all(|h| h.label.starts_with(first)));
	}
}

#[test]
fn repeated_texts_share_labels_and_keep_their_places() {
	let mut rng = Rng::new(7);
	for _ in 0..200 {
		let count = 1 + rng.below(80);
		let targets: Vec<Target> = (0..count)
			.map(|row| Target {
				row,
				cols: (0, 3),
				text: format!("/t{}", rng.below(40)),
			})
			.collect();
		let hints = label(targets.clone());
		let texts: Vec<&String> = targets.iter().map(|t| &t.text).collect();
		let unique = texts.iter().collect::<std::collections::HashSet<_>>().len();
		assert_eq!(hints.len(), count, "every target labelled while all fit");
		assert_eq!(
			hints.iter().map(|h| &h.target).collect::<Vec<_>>(),
			targets.iter().collect::<Vec<_>>()
		);
		for a in &hints {
			for b in &hints {
				assert_eq!(
					a.label == b.label,
					a.target.text == b.target.text,
					"{a:?} {b:?}"
				);
			}
		}
		let mut labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();
		labels.sort_unstable();
		labels.dedup();
		assert_eq!(labels.len(), unique);
	}
}

/// Words that are targets, and words that are not.
const TARGETS: [&str; 10] = [
	"https://example.com/x",
	"src/app/mod.rs:42",
	"~/notes.txt",
	"/etc/hosts",
	"9b9e490",
	"173b8396e1d2c3",
	"10.0.0.1",
	"192.168.1.1:8080",
	"ssh://git@host/repo",
	"../up/there",
];
const PLAIN: [&str; 8] = [
	"hello", "1/2", "deadbeef", "1234567", "v1.2.3", "a,b", "//", "--",
];
/// What may stand between words.
const GAPS: [&str; 6] = [" ", "  ", " | ", "\t", " │ ", "\" '"];
/// What may wrap a word without joining it.
const WRAPS: [(&str, &str); 5] = [("", ""), ("(", ")"), ("<", ">"), ("[", "]."), ("'", "',")];

#[test]
fn generated_rows_find_exactly_the_targets_put_in_them_where_they_are() {
	let mut rng = Rng::new(42);
	for _ in 0..500 {
		let mut line = String::new();
		let mut expected = Vec::new();
		for _ in 0..1 + rng.below(6) {
			if !line.is_empty() {
				line.push_str(rng.pick(&GAPS));
			}
			let (open, close) = *rng.pick(&WRAPS);
			line.push_str(open);
			if rng.below(2) == 0 {
				let word = *rng.pick(&TARGETS);
				let start = line.chars().count();
				expected.push((word.to_owned(), (start, start + word.chars().count() - 1)));
				line.push_str(word);
			} else {
				line.push_str(rng.pick(&PLAIN));
			}
			line.push_str(close);
		}
		let rows = vec![line.chars().zip(0..).collect::<Vec<_>>()];
		let found: Vec<(String, (usize, usize))> = find_targets(&rows)
			.into_iter()
			.map(|t| (t.text, t.cols))
			.collect();
		assert_eq!(found, expected, "{line:?}");
		// Each target's columns hold its text, and none overlap.
		let chars: Vec<char> = line.chars().collect();
		for (text, (first, last)) in &found {
			assert_eq!(
				chars[*first..=*last].iter().collect::<String>(),
				*text,
				"{line:?}"
			);
		}
		assert!(
			found.windows(2).all(|pair| pair[0].1.1 < pair[1].1.0),
			"{line:?}"
		);
	}
}

#[test]
fn whatever_the_row_targets_are_clean_and_find_themselves() {
	// Character soup: letters, hex, digits, every kind of punctuation the
	// finder treats specially, and separators.
	let soup: Vec<char> = "ab9f0x/~.:,;!?()[]{}<>|'\" -_+=@#é漢".chars().collect();
	let mut rng = Rng::new(1234);
	for _ in 0..3000 {
		let line: String = (0..rng.below(40)).map(|_| *rng.pick(&soup)).collect();
		let rows = vec![line.chars().zip(0..).collect::<Vec<_>>()];
		for target in find_targets(&rows) {
			let text = &target.text;
			let chars: Vec<char> = line.chars().collect();
			let (first, last) = target.cols;
			assert_eq!(
				chars[first..=last].iter().collect::<String>(),
				*text,
				"{line:?}"
			);
			assert!(
				!text
					.chars()
					.any(|c| c.is_whitespace() || "\"'`<>|".contains(c)),
				"{text:?}"
			);
			assert!(
				!text.ends_with(['.', ',', ';', ':', '!', '?']),
				"{text:?} in {line:?}"
			);
			assert!(!text.starts_with(['(', '[', '{']), "{text:?} in {line:?}");
			// Found on its own, a target is found whole again.
			let alone = vec![text.chars().zip(0..).collect::<Vec<_>>()];
			let again: Vec<String> = find_targets(&alone).into_iter().map(|t| t.text).collect();
			assert_eq!(again, std::slice::from_ref(text), "{text:?} from {line:?}");
		}
	}
}
