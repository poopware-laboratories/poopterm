//! Tests of the command line: what it accepts, how its help is laid out
//! and colored, and the suggestions its errors make.
use super::{
	COLUMN, Command, Paint, distance, documented, error, help, parse, render, suggest, unknown,
	version, wants_color,
};
use std::ffi::OsString;

const ESC: &str = "\x1b[";

fn args(list: &[&str]) -> std::vec::IntoIter<String> {
	list
		.iter()
		.map(|arg| arg.to_string())
		.collect::<Vec<_>>()
		.into_iter()
}

fn plain(text: &str) -> String {
	render(text, Paint(false))
}

#[test]
fn color_needs_a_terminal_and_no_no_color_or_dumb_term() {
	let set = |value: &str| Some(OsString::from(value));
	assert!(wants_color(true, None, set("xterm-poopterm")));
	assert!(wants_color(true, None, None));
	assert!(!wants_color(false, None, set("xterm-256color")), "piped");
	assert!(!wants_color(true, set("1"), None), "NO_COLOR=1");
	// no-color.org: only a non-empty NO_COLOR turns color off.
	assert!(wants_color(true, set(""), None));
	assert!(!wants_color(true, None, set("dumb")));
	assert!(wants_color(true, None, set("dumber")));
}

#[test]
fn no_arguments_open_the_window() {
	assert_eq!(parse(args(&[])), Ok(Command::Run));
}

#[test]
fn each_option_and_its_short_form_parse() {
	assert_eq!(parse(args(&["--help"])), Ok(Command::Help));
	assert_eq!(parse(args(&["-h"])), Ok(Command::Help));
	assert_eq!(parse(args(&["--version"])), Ok(Command::Version));
	assert_eq!(parse(args(&["-V"])), Ok(Command::Version));
	assert_eq!(
		parse(args(&["--print-terminfo"])),
		Ok(Command::PrintTerminfo)
	);
	assert_eq!(
		parse(args(&["--print-shell-integration", "fish"])),
		Ok(Command::ShellIntegration("fish".into()))
	);
	// Arguments after an option that takes none are ignored, as before.
	assert_eq!(parse(args(&["--help", "--version"])), Ok(Command::Help));
}

#[test]
fn the_benchmark_options_pass_through_in_order() {
	assert_eq!(
		parse(args(&["--benchmark", "--mib", "4", "--only", "log"])),
		Ok(Command::Benchmark(vec![
			"--mib".into(),
			"4".into(),
			"--only".into(),
			"log".into()
		]))
	);
	assert_eq!(
		parse(args(&["--bench-corpus", "tui", "--size", "80x24"])),
		Ok(Command::BenchCorpus(vec![
			"tui".into(),
			"--size".into(),
			"80x24".into()
		]))
	);
	assert_eq!(
		parse(args(&["--benchmark"])),
		Ok(Command::Benchmark(vec![]))
	);
}

#[test]
fn shell_integration_without_a_shell_says_which_and_shows_how() {
	let message = parse(args(&["--print-shell-integration"])).unwrap_err();
	assert_eq!(
		plain_error(&message),
		"error: --print-shell-integration needs a shell: zsh, bash or fish\n  \
		 For example: poopterm --print-shell-integration zsh >> ~/.zshrc\n"
	);
}

fn plain_error(message: &str) -> String {
	error(message, Paint(false))
}

#[test]
fn an_unknown_option_suggests_the_one_meant() {
	assert_eq!(
		parse(args(&["--hepl"])),
		Err(
			"unknown option --hepl\nDid you mean `--help`?\n\
		     Run `poopterm --help` to see every option."
				.into()
		)
	);
	assert_eq!(
		parse(args(&["help"])).unwrap_err(),
		unknown("option", "help", &documented("OPTIONS"))
	);
	assert!(parse(args(&["help"])).unwrap_err().contains("`--help`"));
}

#[test]
fn an_option_like_nothing_known_only_points_at_the_help() {
	assert_eq!(
		parse(args(&["--frobnicate"])),
		Err("unknown option --frobnicate\nRun `poopterm --help` to see every option.".into())
	);
	assert_eq!(
		parse(args(&[""])).unwrap_err(),
		"unknown option \nRun `poopterm --help` to see every option."
	);
}

#[test]
fn suggestions_forgive_dashes_case_typos_and_unfinished_names() {
	let options = documented("OPTIONS");
	for (typed, meant) in [
		("help", "--help"),
		("-help", "--help"),
		("--HELP", "--help"),
		("-v", "-V"),
		("--verison", "--version"),
		("--versoin", "--version"),
		("--print-terminfi", "--print-terminfo"),
		("--print-shell", "--print-shell-integration"),
		("--bench-c", "--bench-corpus"),
		("--benchmrk", "--benchmark"),
		("benchmark", "--benchmark"),
	] {
		assert_eq!(suggest(typed, &options), Some(meant), "{typed}");
	}
}

#[test]
fn no_suggestion_when_nothing_is_close_or_two_are_equally_close() {
	let options = documented("OPTIONS");
	for typed in ["--frobnicate", "--print", "--bench", "-x", "", "--", "pr"] {
		assert_eq!(suggest(typed, &options), None, "{typed}");
	}
	assert_eq!(suggest("--help", &[]), None);
}

#[test]
fn edit_distance_counts_insertions_deletions_and_substitutions() {
	assert_eq!(distance("kitten", "sitting"), 3);
	assert_eq!(distance("", "abc"), 3);
	assert_eq!(distance("abc", ""), 3);
	assert_eq!(distance("abc", "abc"), 0);
	assert_eq!(distance("ab", "ba"), 2);
	assert_eq!(distance("flaw", "lawn"), 2);
	assert_eq!(distance("é", "e"), 1, "characters, not bytes");
}

#[test]
fn version_is_exactly_what_the_packages_check() {
	// Formula/poopterm.rb, PKGBUILD and scripts/package-*.sh compare it.
	assert_eq!(
		version(Paint(false)),
		format!("poopterm {}", env!("CARGO_PKG_VERSION"))
	);
	assert_eq!(
		version(Paint(true)),
		format!("{ESC}1;34mpoopterm{ESC}0m {}", env!("CARGO_PKG_VERSION"))
	);
}

#[test]
fn a_short_row_puts_its_text_at_the_column() {
	assert_eq!(plain("  -h, --help\tShow this help."), {
		let left = "  -h, --help";
		format!("{left}{}Show this help.\n", " ".repeat(COLUMN - left.len()))
	});
	assert_eq!(
		plain("\tmore about it"),
		format!("{}more about it\n", " ".repeat(COLUMN))
	);
}

#[test]
fn a_row_too_wide_for_the_column_starts_its_text_below() {
	// Two spaces must separate the name and its text.
	let fits = format!("{}\tx", "-".repeat(COLUMN - 2));
	assert_eq!(plain(&fits), format!("{}  x\n", "-".repeat(COLUMN - 2)));
	let wide = format!("{}\tx", "-".repeat(COLUMN - 1));
	assert_eq!(
		plain(&wide),
		format!("{}\n{}x\n", "-".repeat(COLUMN - 1), " ".repeat(COLUMN))
	);
	// With no text on the row, nothing trails the name.
	assert_eq!(
		plain("  --long NAME [OPTIONS]\t"),
		"  --long NAME [OPTIONS]\n"
	);
}

#[test]
fn rows_are_measured_in_characters_not_bytes() {
	let left = "  --größe";
	assert_eq!(
		plain(&format!("{left}\tx")),
		format!("{left}{}x\n", " ".repeat(COLUMN - 9))
	);
}

#[test]
fn plain_text_drops_the_markup() {
	assert_eq!(plain("Run `poopterm` now"), "Run poopterm now\n");
	assert_eq!(plain("  # a note"), "  # a note\n");
	assert_eq!(plain("USAGE"), "USAGE\n");
	assert_eq!(plain(""), "");
	assert_eq!(plain("a\n\nb"), "a\n\nb\n");
}

#[test]
fn color_marks_options_values_variables_and_commands() {
	let row = render("Title\n  -h, --help N\tSee `x`.", Paint(true));
	let pad = " ".repeat(COLUMN - "  -h, --help N".len());
	assert_eq!(
		row,
		format!(
			"{ESC}1;34mTitle{ESC}0m\n  {ESC}1;36m-h,{ESC}0m {ESC}1;36m--help{ESC}0m \
			 {ESC}35mN{ESC}0m{pad}See {ESC}32mx{ESC}0m.\n"
		)
	);
	let row = render("Title\n  POOPTERM_FONT\tA font.", Paint(true));
	assert!(
		row.contains(&format!("  {ESC}1;36mPOOPTERM_FONT{ESC}0m ")),
		"{row:?}"
	);
	let row = render("Title\n  poopterm OPTION\tRun.", Paint(true));
	assert!(
		row.contains(&format!(
			"  {ESC}1;34mpoopterm{ESC}0m {ESC}35mOPTION{ESC}0m "
		)),
		"{row:?}"
	);
}

#[test]
fn color_marks_headings_notes_and_links() {
	let text = "Title\nBENCHMARK OPTIONS\n  # why\nSee https://example.org/a now";
	assert_eq!(
		render(text, Paint(true)),
		format!(
			"{ESC}1;34mTitle{ESC}0m\n{ESC}1;33mBENCHMARK OPTIONS{ESC}0m\n  {ESC}2m# why{ESC}0m\n\
			 See {ESC}4mhttps://example.org/a{ESC}0m now\n"
		)
	);
	// A line with lower case is not a heading, nor is the first line.
	assert_eq!(
		render("TITLE\nUSAGE: x", Paint(true)),
		format!("{ESC}1;34mTITLE{ESC}0m\nUSAGE: x\n")
	);
}

#[test]
fn the_help_fills_in_the_release_and_the_config_path() {
	let text = help(Paint(false));
	let first = text.lines().next().unwrap();
	assert_eq!(
		first,
		format!(
			"Poopterm {} \"{}\"",
			env!("CARGO_PKG_VERSION"),
			env!("POOPTERM_CODENAME")
		)
	);
	let config = if cfg!(target_os = "macos") {
		"~/Library/Application Support/poopterm/config.toml"
	} else {
		"~/.config/poopterm/config.toml"
	};
	assert!(text.contains(&format!("\n{}{config}\n", " ".repeat(COLUMN))));
	assert!(!text.contains('{'), "every placeholder is filled");
}

#[test]
fn the_plain_help_has_no_escapes_tabs_or_markup_and_fits_eighty_columns() {
	let text = help(Paint(false));
	assert!(!text.contains(['\x1b', '\t', '`']));
	for line in text.lines() {
		assert!(line.chars().count() <= 80, "{line:?}");
		assert_eq!(line.trim_end(), line, "trailing spaces");
	}
}

#[test]
fn the_colored_help_says_the_same_as_the_plain_one() {
	let colored = help(Paint(true));
	assert!(colored.contains(ESC));
	assert_eq!(strip(&colored), help(Paint(false)));
	assert_eq!(colored.matches(ESC).count() % 2, 0, "each style is reset");
}

/// `text` without its SGR sequences.
fn strip(text: &str) -> String {
	let mut out = String::new();
	let mut rest = text;
	while let Some(at) = rest.find(ESC) {
		out.push_str(&rest[..at]);
		rest = &rest[at..];
		rest = &rest[rest.find('m').unwrap() + 1..];
	}
	out + rest
}

#[test]
fn every_documented_option_is_accepted() {
	let options = documented("OPTIONS");
	assert_eq!(
		options,
		[
			"-h",
			"--help",
			"-V",
			"--version",
			"--print-terminfo",
			"--print-shell-integration",
			"--benchmark",
			"--bench-corpus"
		]
	);
	for option in options {
		let parsed = parse(args(&[option, "zsh"]));
		assert!(
			matches!(parsed, Ok(ref command) if *command != Command::Run),
			"{option}"
		);
	}
}

/// The options a `match` in `source` accepts: its `"--name"` and `"-x"`
/// patterns.
fn matched(source: &str) -> Vec<&str> {
	let mut found = Vec::new();
	for line in source.lines() {
		let line = line.trim_start();
		let Some((patterns, _)) = line.split_once("=>") else {
			continue;
		};
		for pattern in patterns.split('|') {
			let pattern = pattern.trim();
			if let Some(name) = pattern.strip_prefix('"').and_then(|p| p.strip_suffix('"'))
				&& name.starts_with('-')
			{
				found.push(name);
			}
		}
	}
	found.sort_unstable();
	found
}

#[test]
fn every_accepted_option_is_documented() {
	let mut options = documented("OPTIONS");
	options.sort_unstable();
	assert_eq!(matched(include_str!("mod.rs")), options);
	let mut benchmark = documented("BENCHMARK OPTIONS");
	benchmark.sort_unstable();
	assert_eq!(matched(include_str!("../bench/mod.rs")), benchmark);
}

#[test]
fn every_documented_benchmark_option_is_accepted() {
	let sample = |value: &str| match value {
		"N" | "BYTES" => "4",
		"COLSxROWS" => "80x24",
		"NAME,..." => "log",
		"LIST" => "parse,pty-raw",
		other => panic!("no sample for {other}"),
	};
	let help = include_str!("help.txt");
	let section = help.split("BENCHMARK OPTIONS\n").nth(1).unwrap();
	for row in section.lines().take_while(|line| !line.is_empty()) {
		let left = row.split('\t').next().unwrap();
		if left.is_empty() {
			continue;
		}
		let mut words = left.split_whitespace();
		let option = words.next().unwrap();
		// `--list` prints and exits the process.
		if option == "--list" {
			continue;
		}
		let mut given = vec![option.to_owned()];
		given.extend(words.map(|value| sample(value).to_owned()));
		assert!(crate::bench::options(given.into_iter()).is_ok(), "{left}");
	}
}

#[test]
fn an_unknown_benchmark_option_suggests_the_one_meant() {
	let wrong = crate::bench::options(args(&["--mbi", "4"])).err().unwrap();
	assert_eq!(
		wrong.to_string(),
		"unknown benchmark option --mbi\nDid you mean `--mib`?\n\
		 Run `poopterm --help` to see every benchmark option."
	);
	let wrong = crate::bench::options(args(&["--no-rendr"])).err().unwrap();
	assert!(wrong.to_string().contains("`--no-render`"));
}

#[test]
fn errors_lead_with_a_red_error_and_indent_the_hints() {
	let message = "unknown option --x\nDid you mean `--help`?";
	assert_eq!(
		error(message, Paint(false)),
		"error: unknown option --x\n  Did you mean --help?\n"
	);
	assert_eq!(
		error(message, Paint(true)),
		format!("{ESC}1;31merror:{ESC}0m unknown option --x\n  Did you mean {ESC}32m--help{ESC}0m?\n")
	);
	assert_eq!(error("", Paint(false)), "error: \n");
}

#[test]
fn each_section_documents_its_own_options() {
	assert!(documented("BENCHMARK OPTIONS").contains(&"--frame-every"));
	assert!(!documented("OPTIONS").contains(&"--mib"));
	assert!(
		documented("ENVIRONMENT").is_empty(),
		"variables are not options"
	);
	assert!(documented("NO SUCH SECTION").is_empty());
}

#[test]
fn output_that_is_not_a_terminal_is_never_colored() {
	// A file, as when help is redirected, whatever NO_COLOR and TERM say.
	let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
	let file = std::fs::File::open(path).unwrap();
	assert_eq!(Paint::of(&file), Paint(false));
	assert_eq!(help(Paint::of(&file)), help(Paint(false)));
}
