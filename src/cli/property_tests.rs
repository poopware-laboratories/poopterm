//! Property and differential tests of the command line: edit distance
//! against the textbook recursion, suggestions that undo any one typo,
//! colored text that says exactly what plain text says, and the real help
//! checked row by row and against the variables the code reads.
use super::{COLUMN, HELP, Paint, distance, documented, help, render, suggest};

/// A small deterministic generator, so failures repeat.
struct Random(u64);

impl Random {
	fn below(&mut self, n: usize) -> usize {
		self.0 ^= self.0 << 13;
		self.0 ^= self.0 >> 7;
		self.0 ^= self.0 << 17;
		(self.0 % n as u64) as usize
	}

	fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
		from[self.below(from.len())]
	}
}

/// Edit distance by its definition: the cheapest of deleting, inserting
/// or substituting at the front, recursively.
fn reference(a: &[char], b: &[char]) -> usize {
	match (a, b) {
		([], _) => b.len(),
		(_, []) => a.len(),
		([x, rest_a @ ..], [y, rest_b @ ..]) => (reference(rest_a, rest_b) + usize::from(x != y))
			.min(reference(rest_a, b) + 1)
			.min(reference(a, rest_b) + 1),
	}
}

fn word(random: &mut Random) -> String {
	let length = random.below(7);
	(0..length)
		.map(|_| random.pick(&["a", "b", "-", "é"]))
		.collect()
}

#[test]
fn distance_agrees_with_the_textbook_recursion() {
	let mut random = Random(0x9e37_79b9_7f4a_7c15);
	for case in 0..3000 {
		let (a, b) = (word(&mut random), word(&mut random));
		let chars = |s: &str| s.chars().collect::<Vec<_>>();
		assert_eq!(
			distance(&a, &b),
			reference(&chars(&a), &chars(&b)),
			"case {case}: {a:?} {b:?}"
		);
	}
}

#[test]
fn distance_is_a_metric_bounded_by_the_lengths() {
	let mut random = Random(0x2545_f491_4f6c_dd1d);
	for case in 0..2000 {
		let (a, b, c) = (word(&mut random), word(&mut random), word(&mut random));
		let length = |s: &str| s.chars().count();
		let d = distance(&a, &b);
		assert_eq!(d, distance(&b, &a), "case {case}: symmetric");
		assert_eq!(d == 0, a == b, "case {case}: zero only when equal");
		assert!(
			d <= distance(&a, &c) + distance(&c, &b),
			"case {case}: triangle"
		);
		assert!(d >= length(&a).abs_diff(length(&b)), "case {case}");
		assert!(d <= length(&a).max(length(&b)), "case {case}");
	}
}

/// Every option the help lists, in both sections.
fn every_option() -> Vec<&'static str> {
	let mut options = documented("OPTIONS");
	options.extend(documented("BENCHMARK OPTIONS"));
	options
}

/// Each way of mistyping `option` by one character in its name: a
/// character left out, a wrong one, or two swapped.
fn typos(option: &str) -> Vec<String> {
	let dashes = option.len() - option.trim_start_matches('-').len();
	let (prefix, name) = option.split_at(dashes);
	let name: Vec<char> = name.chars().collect();
	let mut typos = Vec::new();
	for at in 0..name.len() {
		let mut left_out = name.clone();
		left_out.remove(at);
		let mut wrong = name.clone();
		wrong[at] = if wrong[at] == 'q' { 'z' } else { 'q' };
		typos.extend([left_out, wrong]);
		if at + 1 < name.len() && name[at] != name[at + 1] {
			let mut swapped = name.clone();
			swapped.swap(at, at + 1);
			typos.push(swapped);
		}
	}
	let typed = |chars: Vec<char>| format!("{prefix}{}", chars.into_iter().collect::<String>());
	typos.into_iter().map(typed).collect()
}

#[test]
fn one_typo_in_a_long_option_still_finds_it() {
	let options = every_option();
	for option in options.iter().filter(|option| option.len() >= 7) {
		for typo in typos(option) {
			assert_eq!(suggest(&typo, &options), Some(*option), "{typo}");
		}
	}
}

#[test]
fn every_option_is_found_without_its_dashes_or_in_capitals() {
	let options = every_option();
	for option in &options {
		let bare = option.trim_start_matches('-');
		assert_eq!(suggest(bare, &options), Some(*option), "{bare}");
		let loud = format!("--{}", bare.to_ascii_uppercase());
		// `-v` and `-V` differ only in case, so only long names count.
		if bare.len() > 1 {
			assert_eq!(suggest(&loud, &options), Some(*option), "{loud}");
		}
		assert_eq!(suggest(option, &options), Some(*option), "itself");
	}
}

#[test]
fn a_suggestion_is_always_one_of_the_known_options() {
	let options = every_option();
	let mut random = Random(0x5851_f42d_4c95_7f2d);
	let pieces = [
		"-", "--", "help", "mib", "read", "print", "x", "e", "-only", "size",
	];
	for _ in 0..3000 {
		let typed: String = (0..1 + random.below(3))
			.map(|_| random.pick(&pieces))
			.collect();
		if let Some(meant) = suggest(&typed, &options) {
			assert!(options.contains(&meant), "{typed} → {meant}");
		}
	}
}

/// `text` without its SGR sequences, checking each is closed.
fn strip(text: &str) -> String {
	let opened = text.matches("\x1b[").count();
	let closed = text.matches("\x1b[0m").count();
	assert_eq!(opened, 2 * closed, "every style is reset: {text:?}");
	let mut out = String::new();
	let mut rest = text;
	while let Some(at) = rest.find("\x1b[") {
		out.push_str(&rest[..at]);
		rest = &rest[at..];
		rest = &rest[rest.find('m').unwrap() + 1..];
	}
	out + rest
}

#[test]
fn colored_text_says_exactly_what_plain_text_says() {
	let pieces = [
		"  ",
		"\t",
		"`",
		"-x",
		"--long",
		"NAME",
		"POOP_VAR",
		"https://a.b/c",
		"word",
		" ",
		"  # ",
		"UPPER",
		"é",
		"poopterm",
		"",
	];
	let mut random = Random(0xd1b5_4a32_d192_ed03);
	for case in 0..3000 {
		let lines: Vec<String> = (0..1 + random.below(5))
			.map(|_| (0..random.below(6)).map(|_| random.pick(&pieces)).collect())
			.collect();
		let text = lines.join("\n");
		let plain = render(&text, Paint(false));
		assert_eq!(
			strip(&render(&text, Paint(true))),
			plain,
			"case {case}: {text:?}"
		);
		assert!(!plain.contains('\x1b'), "case {case}");
		assert!(plain.lines().count() >= text.lines().count(), "case {case}");
	}
}

/// The help's source with its placeholders filled as [`help`] fills them.
fn source() -> String {
	let plain = help(Paint(false));
	let title = plain.lines().next().unwrap();
	let mut lines = HELP.lines();
	lines.next();
	let rest = lines.collect::<Vec<_>>().join("\n");
	format!("{title}\n{}", rest.replace("{config}", super::CONFIG))
}

#[test]
fn every_row_of_the_help_starts_its_text_at_the_column() {
	let source = source();
	let mut rendered = help(Paint(false))
		.lines()
		.map(str::to_owned)
		.collect::<Vec<_>>();
	rendered.reverse();
	let mut next = || rendered.pop().expect("as many lines as the source");
	for line in source.lines() {
		let text = line.replace('`', "");
		if let Some(more) = text.strip_prefix('\t') {
			assert_eq!(next(), format!("{:COLUMN$}{more}", ""));
		} else if let Some((name, about)) = text.split_once('\t') {
			if name.chars().count() + 2 <= COLUMN {
				assert_eq!(next(), format!("{name:COLUMN$}{about}"));
			} else {
				assert_eq!(next(), name);
				assert_eq!(next(), format!("{:COLUMN$}{about}", ""));
			}
		} else {
			assert_eq!(next(), text);
		}
	}
	assert!(rendered.is_empty(), "no lines beyond the source's");
}

#[test]
fn the_colored_help_marks_every_option_variable_and_heading() {
	let colored = help(Paint(true));
	for option in every_option() {
		let marked = format!("\x1b[1;36m{option}");
		assert!(colored.contains(&marked), "{option}");
	}
	for line in HELP.lines().skip(1) {
		if !line.is_empty() && line.chars().all(|c| c.is_ascii_uppercase() || c == ' ') {
			assert!(
				colored.contains(&format!("\x1b[1;33m{line}\x1b[0m\n")),
				"{line}"
			);
		}
	}
	for variable in variables() {
		assert!(
			colored.contains(&format!("  \x1b[1;36m{variable}\x1b[0m")),
			"{variable}"
		);
	}
}

/// The variables the help's ENVIRONMENT section lists.
fn variables() -> Vec<&'static str> {
	let section = HELP.split("ENVIRONMENT\n").nth(1).unwrap();
	let rows = section.lines().take_while(|line| !line.is_empty());
	let names = rows.filter(|row| !row.starts_with('\t'));
	names
		.map(|row| row.split('\t').next().unwrap().trim())
		.collect()
}

/// Every Rust source file under `dir` but the test files, read.
fn sources(dir: &std::path::Path, out: &mut Vec<String>) {
	for entry in std::fs::read_dir(dir).unwrap() {
		let path = entry.unwrap().path();
		if path.is_dir() {
			sources(&path, out);
		} else if path.extension().is_some_and(|ext| ext == "rs")
			&& !path.to_string_lossy().ends_with("tests.rs")
		{
			out.push(std::fs::read_to_string(&path).unwrap());
		}
	}
}

#[test]
fn every_variable_the_help_lists_is_one_the_code_reads() {
	let mut files = Vec::new();
	sources(
		&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
		&mut files,
	);
	let variables = variables();
	assert!(variables.len() >= 7, "{variables:?}");
	for variable in variables {
		let quoted = format!("\"{variable}\"");
		assert!(
			files.iter().any(|file| file.contains(&quoted)),
			"{variable} is listed but never read"
		);
	}
}
