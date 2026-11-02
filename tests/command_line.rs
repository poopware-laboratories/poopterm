//! The built `poopterm` run as people and scripts run it: what each option
//! prints, where, with which exit status, and without color when piped.
use std::process::{Command, Output};

/// Runs `poopterm` with `args`, its output piped, as a script would.
fn poopterm(args: &[&str]) -> Output {
	Command::new(env!("CARGO_BIN_EXE_poopterm"))
		.args(args)
		.env_remove("NO_COLOR")
		.env("TERM", "xterm-256color")
		.output()
		.expect("poopterm runs")
}

fn text(bytes: &[u8]) -> String {
	String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
}

#[test]
fn help_goes_to_standard_output_uncolored_when_piped() {
	for option in ["--help", "-h"] {
		let run = poopterm(&[option]);
		assert!(run.status.success(), "{option}");
		let help = text(&run.stdout);
		assert!(help.starts_with("Poopterm "), "{help}");
		assert!(help.contains("\nOPTIONS\n") && help.contains("\nENVIRONMENT\n"));
		assert!(!help.contains('\x1b'), "piped help has no color");
		assert!(run.stderr.is_empty());
	}
}

#[test]
fn version_is_the_single_line_packages_compare() {
	for option in ["--version", "-V"] {
		let run = poopterm(&[option]);
		assert!(run.status.success());
		assert_eq!(
			text(&run.stdout),
			format!("poopterm {}\n", env!("CARGO_PKG_VERSION"))
		);
	}
}

#[test]
fn a_mistyped_option_fails_on_standard_error_with_a_suggestion() {
	let run = poopterm(&["--verison"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(run.stdout.is_empty(), "nothing on standard output");
	assert_eq!(
		text(&run.stderr),
		"error: unknown option --verison\n  Did you mean --version?\n  \
		 Run poopterm --help to see every option.\n"
	);
}

#[test]
fn an_option_like_nothing_known_only_points_at_the_help() {
	let run = poopterm(&["--zzzzzzzz"]);
	assert_eq!(run.status.code(), Some(1));
	assert_eq!(
		text(&run.stderr),
		"error: unknown option --zzzzzzzz\n  Run poopterm --help to see every option.\n"
	);
}

#[test]
fn shell_integration_needs_a_shell_and_names_the_choices() {
	let run = poopterm(&["--print-shell-integration"]);
	assert_eq!(run.status.code(), Some(1));
	let error = text(&run.stderr);
	assert!(error.starts_with("error: --print-shell-integration needs a shell: zsh, bash or fish\n"));
	assert!(error.contains("poopterm --print-shell-integration zsh >> ~/.zshrc"));
	let run = poopterm(&["--print-shell-integration", "tcsh"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(
		text(&run.stderr).starts_with("error: "),
		"not a debug-formatted string"
	);
	assert!(!text(&run.stderr).contains("Error: \""));
}

#[test]
fn shell_integration_prints_each_shells_hooks() {
	for shell in ["zsh", "bash", "fish"] {
		let run = poopterm(&["--print-shell-integration", shell]);
		assert!(run.status.success(), "{shell}");
		assert!(
			text(&run.stdout).contains("poopterm shell integration"),
			"{shell}"
		);
		assert!(run.stderr.is_empty());
	}
}

#[test]
fn terminfo_prints_the_entry_to_compile() {
	let run = poopterm(&["--print-terminfo"]);
	assert!(run.status.success());
	assert!(text(&run.stdout).contains("xterm-poopterm|"));
}

#[test]
fn a_mistyped_benchmark_option_suggests_the_one_meant() {
	let run = poopterm(&["--benchmark", "--no-rendr"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(run.stdout.is_empty());
	assert_eq!(
		text(&run.stderr),
		"error: unknown benchmark option --no-rendr\n  Did you mean --no-render?\n  \
		 Run poopterm --help to see every benchmark option.\n"
	);
}

#[test]
fn the_benchmark_lists_its_workloads() {
	let run = poopterm(&["--benchmark", "--list"]);
	assert!(run.status.success());
	let list = text(&run.stdout);
	// The help's example names these two.
	for workload in ["log", "tui"] {
		assert!(
			list
				.lines()
				.any(|line| line.split_whitespace().next() == Some(workload))
		);
	}
}

#[test]
fn the_corpus_writes_a_workloads_bytes_and_checks_its_name() {
	let run = poopterm(&["--bench-corpus", "log", "--kib", "1"]);
	assert!(run.status.success());
	assert!(run.stdout.len() >= 1024, "{} bytes", run.stdout.len());
	let run = poopterm(&["--bench-corpus"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(text(&run.stderr).starts_with("error: missing workload name"));
}
