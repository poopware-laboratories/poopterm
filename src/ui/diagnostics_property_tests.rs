//! Property tests of the Diagnostics report: for random diagnostics, its
//! shape, the order and completeness of the log, and what never leaks.
use super::diagnostics::{Diagnostics, Drawing, Feature, Reason, private, report};
use crate::ui::recent_frames::{FrameCounts, GpuTime};
use crate::warnings::{LONGEST, Ring, Warning};
use std::{
	path::{Path, PathBuf},
	time::Duration,
};

/// A small deterministic generator (xorshift), so failures reproduce.
pub(super) struct Random(pub(super) u64);

impl Random {
	pub(super) fn next(&mut self) -> u64 {
		self.0 ^= self.0 << 13;
		self.0 ^= self.0 >> 7;
		self.0 ^= self.0 << 17;
		self.0
	}

	pub(super) fn below(&mut self, n: usize) -> usize {
		(self.next() % n as u64) as usize
	}

	pub(super) fn pick<'a>(&mut self, words: &[&'a str]) -> &'a str {
		words[self.below(words.len())]
	}
}

const HOMES: [&str; 4] = ["/home/vx", "/home/a b", "/Users/ünï", "/root"];
const WORDS: [&str; 10] = [
	"font",
	"ünïcödé",
	"🦀",
	"/usr/share/x",
	"a\nb",
	"\u{1b}[2J",
	"%",
	"{}",
	"~",
	"/",
];

/// A random line that may name a file in the home directory.
fn text(random: &mut Random, home: &str) -> String {
	let words = random.below(6);
	let mut line = String::new();
	for _ in 0..words {
		if random.below(3) == 0 {
			line.push_str(home);
			line.push('/');
		}
		line.push_str(random.pick(&WORDS));
		line.push(' ');
	}
	line
}

fn drawing(random: &mut Random) -> Drawing {
	match random.below(5) {
		0 => Drawing::Gpu,
		1 => Drawing::Software(Reason::Asked),
		2 => Drawing::Software(Reason::Unsupported),
		3 => Drawing::Software(Reason::Unavailable(random.pick(&WORDS).into())),
		_ => Drawing::Software(Reason::Abandoned(random.pick(&WORDS).into())),
	}
}

/// Random diagnostics whose warnings went through a ring, as the page's do.
pub(super) fn diagnostics(random: &mut Random) -> Diagnostics {
	let home = HOMES[random.below(HOMES.len())];
	let mut ring = Ring::default();
	let pushes = random.below(12);
	for i in 0..pushes {
		let at = Duration::from_secs(random.next() % (1 << 32));
		ring.push(at, &format!("w{i} {}", text(random, home)));
	}
	let facts = (0..random.below(4))
		.map(|_| {
			(
				"Fact",
				random
					.pick(&["Vulkan on Wayland", "softbuffer", "570 frames"])
					.into(),
			)
		})
		.collect();
	Diagnostics {
		version: "26.11.30".into(),
		drawing: drawing(random),
		// None at all, a few, or a full ring split any way.
		frames: match random.below(3) {
			0 => FrameCounts::default(),
			1 => FrameCounts {
				gpu: random.below(3),
				software: random.below(3),
				gpu_time: None,
			},
			_ => {
				let gpu = random.below(601);
				FrameCounts {
					gpu,
					software: 600 - gpu,
					// Times as frames on the GPU have, from 0 to 99 ms.
					gpu_time: (gpu > 0).then(|| GpuTime {
						median: Duration::from_micros(random.below(9_000) as u64),
						worst: Duration::from_micros(9_000 + random.below(90_000) as u64),
					}),
				}
			}
		},
		facts,
		warnings: ring.warnings().cloned().collect(),
		dropped: random.below(3),
		home: Some(PathBuf::from(home)),
	}
}

#[test]
fn every_report_has_the_same_shape() {
	let mut random = Random(0x853c_49e6_748f_ea9b);
	for case in 0..300 {
		let diagnostics = diagnostics(&mut random);
		let text = report(&diagnostics);
		let lines: Vec<&str> = text.lines().collect();
		let log_lines = diagnostics.warnings.len().max(1) + usize::from(diagnostics.dropped > 0)
			- usize::from(diagnostics.warnings.is_empty() && diagnostics.dropped > 0);
		let frames = usize::from(diagnostics.frames.line().is_some());
		let graphics = Feature::ALL.len() + frames + diagnostics.facts.len();
		let expected = 3 + graphics + 2 + log_lines;
		assert_eq!(lines.len(), expected, "case {case}:\n{text}");
		assert_eq!(lines[0], "Poopterm 26.11.30 diagnostics");
		assert_eq!(lines[2], "Graphics");
		assert_eq!(lines[3 + graphics + 1], "Log");
		for (i, feature) in Feature::ALL.iter().enumerate() {
			assert!(
				lines[3 + i].starts_with(&format!("  {:<17}", feature.name())),
				"case {case}"
			);
		}
		if frames == 1 {
			let line = lines[3 + Feature::ALL.len()];
			assert!(line.starts_with("  Last "), "case {case}:\n{text}");
			let end = match diagnostics.frames.gpu_time {
				Some(_) => " ms worst",
				None => " software",
			};
			assert!(line.ends_with(end), "case {case}:\n{text}");
		}
		assert!(text.ends_with('\n'));
	}
}

#[test]
fn every_warning_appears_once_in_order_on_its_own_line() {
	let mut random = Random(0xda94_2042_e4dd_58b5);
	for case in 0..300 {
		let diagnostics = diagnostics(&mut random);
		let text = report(&diagnostics);
		let log: Vec<&str> = text.split("\nLog\n").nth(1).unwrap().lines().collect();
		let rows: Vec<&&str> = log.iter().filter(|line| line.contains(" UTC ")).collect();
		assert_eq!(
			rows.len(),
			diagnostics.warnings.len(),
			"case {case}:\n{text}"
		);
		for (i, row) in rows.iter().enumerate() {
			assert!(row.contains(&format!(" w{i} ")), "case {case}: {row}");
		}
	}
}

#[test]
fn no_report_carries_controls_or_the_home_directory() {
	let mut random = Random(0x9fb2_1c65_1e98_df25);
	for case in 0..300 {
		let diagnostics = diagnostics(&mut random);
		let text = report(&diagnostics);
		let home = diagnostics.home.as_ref().unwrap().display().to_string();
		let log = text.split("\nLog\n").nth(1).unwrap();
		assert!(!log.contains(&format!("{home}/")), "case {case}:\n{text}");
		for line in text.lines() {
			assert!(!line.chars().any(char::is_control), "case {case}: {line:?}");
			// Two-space indent, a 17-column label, a bounded value.
			assert!(line.len() <= 2 + 17 + LONGEST + 16, "case {case}");
		}
	}
}

#[test]
fn private_text_never_names_the_home_and_keeps_everything_else() {
	let mut random = Random(0x1234_5678_9abc_def1);
	for case in 0..1000 {
		let home = HOMES[random.below(HOMES.len())];
		let line = text(&mut random, home);
		let shown = private(&line, Some(Path::new(home)));
		assert!(!shown.contains(home), "case {case}: {shown}");
		// Putting the home back gives the line again.
		assert_eq!(
			shown.replace('~', home),
			line.replace('~', home),
			"case {case}"
		);
		// Without a home, or with a root one, nothing changes.
		assert_eq!(private(&line, None), line);
		assert_eq!(private(&line, Some(Path::new("/"))), line);
	}
}

#[test]
fn warnings_built_by_hand_report_as_given() {
	// The page may hand over warnings straight from `recent`; whatever the
	// time, the clock is a time of day.
	let mut random = Random(0x0bad_cafe_f00d_d00d);
	for _ in 0..200 {
		let at = Duration::from_secs(random.next());
		let warnings = vec![Warning {
			at,
			text: "x".into(),
		}];
		let text = report(&Diagnostics {
			version: String::new(),
			drawing: Drawing::Gpu,
			frames: FrameCounts::default(),
			facts: vec![],
			warnings,
			dropped: 0,
			home: None,
		});
		let row = text.lines().last().unwrap();
		let seconds = at.as_secs() % 86_400;
		let clock = format!(
			"{:02}:{:02}:{:02}",
			seconds / 3600,
			seconds / 60 % 60,
			seconds % 60
		);
		assert_eq!(row, format!("  {clock} UTC     x"));
	}
}
