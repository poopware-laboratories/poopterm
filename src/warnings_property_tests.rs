//! Property tests of the warning ring: random lines keep `clean`'s promises,
//! and random runs of warnings match a reference that keeps everything.
use super::warnings::{KEPT, LONGEST, Ring, clean};
use std::time::Duration;

/// A small deterministic generator (xorshift), so failures reproduce.
struct Random(u64);

impl Random {
	fn next(&mut self) -> u64 {
		self.0 ^= self.0 << 13;
		self.0 ^= self.0 >> 7;
		self.0 ^= self.0 << 17;
		self.0
	}

	fn below(&mut self, n: usize) -> usize {
		(self.next() % n as u64) as usize
	}
}

/// Characters that stress cleaning and cutting: ASCII, controls (C0, DEL,
/// C1), and two-, three- and four-byte characters.
const ALPHABET: [char; 12] = [
	'a', ' ', '/', '\n', '\u{1b}', '\u{7f}', '\u{9b}', 'é', '€', '…', '🦀', '\u{fffd}',
];

fn line(random: &mut Random, length: usize) -> String {
	(0..length)
		.map(|_| ALPHABET[random.below(ALPHABET.len())])
		.collect()
}

#[test]
fn cleaned_lines_are_bounded_free_of_controls_and_a_prefix_of_the_input() {
	let mut random = Random(0x9e37_79b9_7f4a_7c15);
	for case in 0..2000 {
		let length = [
			0,
			1,
			100,
			LONGEST / 3,
			LONGEST / 2,
			LONGEST,
			LONGEST + 1,
			3 * LONGEST,
		][case % 8];
		let text = line(&mut random, length);
		let cut = clean(&text);
		assert!(cut.len() <= LONGEST, "case {case}: {}", cut.len());
		assert!(!cut.chars().any(char::is_control), "case {case}");
		// Character by character, the line is the input with controls replaced.
		let replaced: String = text
			.chars()
			.map(|c| if c.is_control() { '\u{fffd}' } else { c })
			.collect();
		match cut.strip_suffix('…') {
			Some(kept) if replaced.len() > LONGEST => {
				assert!(replaced.starts_with(kept), "case {case}");
				// Nothing more was cut than the ellipsis needed.
				let next = replaced[kept.len()..].chars().next().unwrap();
				assert!(
					kept.len() + next.len_utf8() + '…'.len_utf8() > LONGEST,
					"case {case}"
				);
			}
			_ => assert_eq!(cut, replaced, "case {case}"),
		}
	}
}

#[test]
fn cleaning_twice_changes_nothing_more() {
	let mut random = Random(0x2545_f491_4f6c_dd1d);
	for case in 0..500 {
		let text = line(&mut random, case % 2 * LONGEST + case);
		let once = clean(&text);
		assert_eq!(clean(&once), once, "case {case}");
	}
}

#[test]
fn random_runs_keep_the_newest_and_count_the_rest() {
	let mut random = Random(0xd1b5_4a32_d192_ed03);
	for run in 0..40 {
		let pushes = random.below(3 * KEPT);
		let mut ring = Ring::default();
		let mut everything = Vec::new();
		for i in 0..pushes {
			let length = random.below(8);
			let text = format!("{run}.{i} {}", line(&mut random, length));
			let at = Duration::from_secs(random.next() % 86_400);
			ring.push(at, &text);
			everything.push((at, clean(&text)));
		}
		let kept: Vec<(Duration, String)> = ring
			.warnings()
			.map(|warning| (warning.at, warning.text.clone()))
			.collect();
		let skip = pushes.saturating_sub(KEPT);
		assert_eq!(kept, everything[skip..], "run {run}");
		assert_eq!(ring.dropped(), skip, "run {run}");
		assert_eq!(ring.dropped() + kept.len(), pushes, "run {run}");
	}
}
