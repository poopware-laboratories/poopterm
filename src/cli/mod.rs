//! The command line: what `poopterm` accepts, its help and version text, and
//! errors that suggest the option meant.
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod tests;

use std::{env, ffi::OsString, io::IsTerminal};

/// The help, with light markup that [`render`] lays out and colors: the
/// first line is the title, upper-case lines are headings, a tab ends the
/// name of an option or variable, a leading tab continues its text, `  # `
/// starts a note, and backticks mark a command.
const HELP: &str = include_str!("help.txt");

/// Where the text of each option, variable and command line starts.
pub(crate) const COLUMN: usize = 30;

/// Where the config file is when nothing says otherwise.
const CONFIG: &str = if cfg!(target_os = "macos") {
	"~/Library/Application Support/poopterm/config.toml"
} else {
	"~/.config/poopterm/config.toml"
};

/// SGR parameters of each kind of text, in the 16 colors themes set.
const TITLE: &str = "1;34";
const HEADING: &str = "1;33";
const OPTION: &str = "1;36";
const VALUE: &str = "35";
const CODE: &str = "32";
const NOTE: &str = "2";
const LINK: &str = "4";
const ERROR: &str = "1;31";

/// Whether to color what is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Paint(pub(crate) bool);

impl Paint {
	/// Color for `stream` as [`wants_color`] decides from the environment.
	pub(crate) fn of(stream: &impl IsTerminal) -> Self {
		Self(wants_color(
			stream.is_terminal(),
			env::var_os("NO_COLOR"),
			env::var_os("TERM"),
		))
	}

	fn style(self, sgr: &str, text: &str) -> String {
		if self.0 && !text.is_empty() {
			format!("\x1b[{sgr}m{text}\x1b[0m")
		} else {
			text.to_owned()
		}
	}
}

/// Color only a terminal, unless `NO_COLOR` is set and not empty
/// (no-color.org) or `TERM` is `dumb`.
pub(crate) fn wants_color(
	terminal: bool,
	no_color: Option<OsString>,
	term: Option<OsString>,
) -> bool {
	terminal && no_color.is_none_or(|value| value.is_empty()) && term.is_none_or(|t| t != "dumb")
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
	Run,
	Help,
	Version,
	PrintTerminfo,
	ShellIntegration(String),
	Benchmark(Vec<String>),
	BenchCorpus(Vec<String>),
}

/// The command `args`, without the program's name, ask for.
pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
	let Some(argument) = args.next() else {
		return Ok(Command::Run);
	};
	Ok(match argument.as_str() {
		"--help" | "-h" => Command::Help,
		"--version" | "-V" => Command::Version,
		"--print-terminfo" => Command::PrintTerminfo,
		"--print-shell-integration" => Command::ShellIntegration(args.next().ok_or(
			"--print-shell-integration needs a shell: zsh, bash or fish\n\
			 For example: `poopterm --print-shell-integration zsh >> ~/.zshrc`",
		)?),
		"--benchmark" => Command::Benchmark(args.collect()),
		"--bench-corpus" => Command::BenchCorpus(args.collect()),
		_ => return Err(unknown("option", &argument, &documented("OPTIONS"))),
	})
}

/// `poopterm` and the version, which packages compare exactly.
pub(crate) fn version(paint: Paint) -> String {
	format!(
		"{} {}",
		paint.style(TITLE, "poopterm"),
		env!("CARGO_PKG_VERSION")
	)
}

/// The help, filled in and laid out.
pub(crate) fn help(paint: Paint) -> String {
	let text = HELP
		.replace("{version}", env!("CARGO_PKG_VERSION"))
		.replace("{codename}", env!("POOPTERM_CODENAME"))
		.replace("{config}", CONFIG);
	render(&text, paint)
}

/// Lays out and colors `text`, marked up as [`HELP`] is.
pub(crate) fn render(text: &str, paint: Paint) -> String {
	let mut out = String::new();
	for (i, line) in text.lines().enumerate() {
		if i == 0 && !line.starts_with(' ') && !line.contains(['\t', '`']) {
			out += &paint.style(TITLE, line);
		} else if !line.is_empty() && line.chars().all(|c| c.is_ascii_uppercase() || c == ' ') {
			out += &paint.style(HEADING, line);
		} else if let Some(note) = line.strip_prefix("  # ") {
			out += &format!("  {}", paint.style(NOTE, &format!("# {note}")));
		} else if let Some(more) = line.strip_prefix('\t') {
			out += &format!("{:COLUMN$}{}", "", inline(more, paint));
		} else if let Some((name, about)) = row(line) {
			out += &names(name, paint);
			let width = name.chars().count();
			if !about.is_empty() {
				let gap = if width + 2 > COLUMN {
					format!("\n{:COLUMN$}", "")
				} else {
					" ".repeat(COLUMN - width)
				};
				out += &(gap + &inline(about, paint));
			}
		} else {
			out += &inline(line, paint);
		}
		out.push('\n');
	}
	out
}

/// The name and text of an option or variable `line` lists: split at a
/// tab, or an option alone on its line, its text on the lines below.
fn row(line: &str) -> Option<(&str, &str)> {
	if line.starts_with('\t') {
		return None;
	}
	let alone = line.starts_with("  ") && line.trim_start().starts_with('-');
	line.split_once('\t').or(alone.then_some((line, "")))
}

/// An option's or variable's name and its values, each colored by kind.
fn names(text: &str, paint: Paint) -> String {
	let word = |word: &str| {
		let variable = word.contains('_')
			&& word
				.chars()
				.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
		match word {
			"poopterm" => paint.style(TITLE, word),
			_ if word.starts_with('-') || variable => paint.style(OPTION, word),
			_ => paint.style(VALUE, word),
		}
	};
	text.split(' ').map(word).collect::<Vec<_>>().join(" ")
}

/// Running text: commands between backticks, and links.
fn inline(text: &str, paint: Paint) -> String {
	let mut out = String::new();
	for (i, part) in text.split('`').enumerate() {
		if i % 2 == 1 {
			out += &paint.style(CODE, part);
			continue;
		}
		let words = part
			.split(' ')
			.map(|word| match word.starts_with("https://") {
				true => paint.style(LINK, word),
				false => word.to_owned(),
			});
		out += &words.collect::<Vec<_>>().join(" ");
	}
	out
}

/// The options the help lists under `heading`.
pub(crate) fn documented(heading: &str) -> Vec<&'static str> {
	let mut lines = HELP.lines().skip_while(|line| *line != heading).skip(1);
	let rows = lines.by_ref().take_while(|line| !line.is_empty());
	let names = rows.filter_map(row).map(|(name, _)| name);
	let words = names.flat_map(|name| name.split([' ', ',']));
	words.filter(|word| word.starts_with('-')).collect()
}

/// That `word` is not a known `what`, with the closest of `known`.
pub(crate) fn unknown(what: &str, word: &str, known: &[&str]) -> String {
	let mut message = format!("unknown {what} {word}\n");
	if let Some(meant) = suggest(word, known) {
		message += &format!("Did you mean `{meant}`?\n");
	}
	message + &format!("Run `poopterm --help` to see every {what}.")
}

/// The one of `known` that `word` most likely meant: the same name with
/// other dashes or case, the only name it begins, or the only one within
/// a few typos.
pub(crate) fn suggest<'a>(word: &str, known: &[&'a str]) -> Option<&'a str> {
	let bare = |name: &str| name.trim_start_matches('-').to_ascii_lowercase();
	let typed = bare(word);
	let only = |found: Vec<&'a str>| (found.len() == 1).then(|| found[0]);
	let same = known.iter().copied().filter(|name| bare(name) == typed);
	if let Some(name) = only(same.collect()) {
		return Some(name);
	}
	if typed.chars().count() >= 3 {
		let begun = known
			.iter()
			.copied()
			.filter(|name| bare(name).starts_with(&typed));
		if let Some(name) = only(begun.collect()) {
			return Some(name);
		}
	}
	let most = (typed.chars().count() / 3).max(2);
	let near = known
		.iter()
		.map(|name| (distance(&typed, &bare(name)), *name));
	let best = near.clone().map(|(d, _)| d).min().filter(|d| *d <= most)?;
	only(
		near
			.filter(|(d, _)| *d == best)
			.map(|(_, name)| name)
			.collect(),
	)
}

/// The Levenshtein distance between `a` and `b`, in characters.
pub(crate) fn distance(a: &str, b: &str) -> usize {
	let b: Vec<char> = b.chars().collect();
	let mut row: Vec<usize> = (0..=b.len()).collect();
	for (i, x) in a.chars().enumerate() {
		let mut diagonal = row[0];
		row[0] = i + 1;
		for (j, y) in b.iter().enumerate() {
			let substitute = diagonal + usize::from(x != *y);
			diagonal = row[j + 1];
			row[j + 1] = substitute.min(row[j] + 1).min(diagonal + 1);
		}
	}
	row[b.len()]
}

/// `message` as an error: `error:` and its first line, then its hints.
pub(crate) fn error(message: &str, paint: Paint) -> String {
	let mut lines = message.split('\n');
	let first = lines.next().unwrap_or_default();
	let mut out = format!(
		"{} {}\n",
		paint.style(ERROR, "error:"),
		inline(first, paint)
	);
	for hint in lines {
		out += &format!("  {}\n", inline(hint, paint));
	}
	out
}
