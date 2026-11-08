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

/// Whether `cell` reads `median (min–max)`, with min ≤ median ≤ max.
fn is_spread(cell: &str) -> bool {
	let parsed = || -> Option<(f64, f64, f64)> {
		let (median, range) = cell.strip_suffix(')')?.split_once(" (")?;
		let (min, max) = range.split_once('–')?;
		Some((median.parse().ok()?, min.parse().ok()?, max.parse().ok()?))
	};
	matches!(parsed(), Some((median, min, max)) if min <= median && median <= max && min > 0.0)
}

/// A quick benchmark of `log` and `tui` at a small size, with `more` options.
fn quick_benchmark(more: &[&str]) -> (Output, String) {
	let mut args = vec![
		"--benchmark",
		"--kib",
		"4",
		"--only",
		"log,tui",
		"--size",
		"40x12",
	];
	args.extend(more);
	let run = poopterm(&args);
	assert!(run.status.success(), "{}", text(&run.stderr));
	assert!(run.stderr.is_empty());
	let report = text(&run.stdout);
	(run, report)
}

#[test]
fn the_benchmark_prints_medians_with_their_spread_per_workload_and_stage() {
	let (_, report) = quick_benchmark(&["--repeat", "2"]);
	let lines: Vec<&str> = report.lines().collect();
	assert_eq!(lines.len(), 6, "{report}");
	let version = env!("CARGO_PKG_VERSION");
	assert_eq!(
		lines[0],
		format!("poopterm {version}, 40x12, 4 KiB per workload")
	);
	assert_eq!(lines[1], "MB/s, median (min–max)");
	assert_eq!(
		lines[2].split_whitespace().collect::<Vec<_>>(),
		["workload", "parse", "+render"]
	);
	for (line, workload) in lines[3..5].iter().zip(["log", "tui"]) {
		let (name, cells) = line.split_once(' ').unwrap();
		assert_eq!(name, workload);
		let cells: Vec<&str> = cells.trim().split("  ").filter(|c| !c.is_empty()).collect();
		assert_eq!(cells.len(), 2, "{line}");
		assert!(cells.iter().all(|cell| is_spread(cell.trim())), "{line}");
	}
	let overall: Vec<&str> = lines[5].split_whitespace().collect();
	assert_eq!(overall[0], "overall");
	assert_eq!(overall.len(), 3, "{report}");
	assert!(
		overall[1..]
			.iter()
			.all(|rate| rate.parse::<f64>().is_ok_and(|rate| rate > 0.0))
	);
}

#[test]
fn the_benchmark_prints_a_markdown_table_when_asked() {
	let (_, report) = quick_benchmark(&["--repeat", "1", "--markdown"]);
	let lines: Vec<&str> = report.lines().collect();
	assert!(lines[0].starts_with("**poopterm ") && lines[0].ends_with("MB/s, median (min–max)"));
	assert_eq!(lines[1], "");
	assert_eq!(lines[2], "| workload | parse | +render |");
	assert_eq!(lines[3], "|---|--:|--:|");
	let cells = |line: &str| -> Vec<String> {
		let inner = line
			.strip_prefix("| ")
			.and_then(|l| l.strip_suffix(" |"))
			.unwrap();
		inner.split(" | ").map(str::to_owned).collect()
	};
	assert_eq!(cells(lines[4])[0], "log");
	assert_eq!(cells(lines[5])[0], "tui");
	assert!(
		cells(lines[4])[1..].iter().all(|cell| is_spread(cell)),
		"{report}"
	);
	assert_eq!(cells(lines[6])[0], "overall");
	assert_eq!(lines.len(), 7, "{report}");
}

#[test]
fn without_rendering_the_markdown_has_one_number_column() {
	let (_, report) = quick_benchmark(&["--repeat", "1", "--markdown", "--no-render"]);
	assert!(
		report.contains("\n| workload | parse |\n|---|--:|\n"),
		"{report}"
	);
	assert!(!report.contains("+render"));
}

#[test]
fn pty_stages_run_through_a_real_terminal_in_the_order_given() {
	let stages = [
		"--stages",
		"pipeline-raw,pty",
		"--repeat",
		"1",
		"--markdown",
		"--kib",
		"64",
	];
	let (_, report) = quick_benchmark(&stages);
	// Columns keep their fixed order, whatever order --stages names them in;
	// 64 KiB so a slow machine's rates still show above 0.0.
	assert!(
		report.contains("\n| workload | pty | +pipeline raw |\n|---|--:|--:|\n"),
		"{report}"
	);
	for workload in ["log", "tui"] {
		let line = (report.lines())
			.find(|line| line.starts_with(&format!("| {workload} |")))
			.unwrap();
		let cells: Vec<&str> = line.trim_matches(['|', ' ']).split(" | ").skip(1).collect();
		assert_eq!(cells.len(), 2, "{line}");
		assert!(cells.iter().all(|cell| is_spread(cell)), "{line}");
	}
}

#[test]
fn every_pty_stage_has_its_column_and_an_overall_rate() {
	let stages = [
		"--stages",
		"all",
		"--no-render",
		"--repeat",
		"1",
		"--markdown",
		"--kib",
		"64",
	];
	let (_, report) = quick_benchmark(&stages);
	let heading = "| workload | parse | pty | pty raw | +pipeline | +pipeline raw |";
	assert!(report.contains(heading), "{report}");
	let overall = report
		.lines()
		.find(|line| line.starts_with("| overall |"))
		.unwrap();
	let rates: Vec<&str> = overall
		.trim_matches(['|', ' '])
		.split(" | ")
		.skip(1)
		.collect();
	assert_eq!(rates.len(), 5, "{overall}");
	assert!(
		rates
			.iter()
			.all(|rate| rate.parse::<f64>().is_ok_and(|rate| rate > 0.0)),
		"{overall}"
	);
}

#[test]
fn an_unknown_stage_fails_before_measuring_anything() {
	let run = poopterm(&["--benchmark", "--stages", "pty,gpu"]);
	assert!(!run.status.success());
	assert!(run.stdout.is_empty(), "{}", text(&run.stdout));
	let error = text(&run.stderr);
	assert!(
		error.contains("unknown stage: gpu; the stages are parse, render"),
		"{error}"
	);
}

#[test]
fn one_run_per_stage_is_its_own_median_and_spread() {
	let (_, report) = quick_benchmark(&["--repeat", "1", "--markdown"]);
	for line in report
		.lines()
		.filter(|line| line.starts_with("| log") || line.starts_with("| tui"))
	{
		for cell in line.trim_matches(['|', ' ']).split(" | ").skip(1) {
			let (median, range) = cell.split_once(" (").unwrap();
			let (min, max) = range.trim_end_matches(')').split_once('–').unwrap();
			assert_eq!((min, max), (median, median), "{line}");
		}
	}
}

#[test]
fn without_rendering_the_text_has_one_number_per_workload() {
	let (_, report) = quick_benchmark(&["--repeat", "1", "--no-render"]);
	let lines: Vec<&str> = report.lines().collect();
	assert_eq!(
		lines[2].split_whitespace().collect::<Vec<_>>(),
		["workload", "parse"]
	);
	for line in &lines[3..5] {
		// A name, then one cell: `median (min–max)` is two words.
		assert_eq!(line.split_whitespace().count(), 3, "{line}");
	}
	assert_eq!(lines[5].split_whitespace().count(), 2, "{report}");
}

#[test]
fn the_overall_of_one_workload_is_its_median() {
	let args = [
		"--benchmark",
		"--kib",
		"4",
		"--only",
		"tui",
		"--no-render",
		"--repeat",
		"3",
	];
	let run = poopterm(&args);
	assert!(run.status.success());
	let report = text(&run.stdout);
	let lines: Vec<&str> = report.lines().collect();
	let median = lines[3].split_whitespace().nth(1).unwrap();
	let overall = lines[4].split_whitespace().nth(1).unwrap();
	// Both are rounded to one decimal from the same median.
	assert_eq!(overall, median, "{report}");
}

#[test]
fn a_mistyped_repeat_suggests_the_option_meant() {
	let run = poopterm(&["--benchmark", "--repet", "2"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(run.stdout.is_empty());
	assert!(
		text(&run.stderr).contains("Did you mean --repeat?"),
		"{}",
		text(&run.stderr)
	);
}

#[test]
fn repeat_needs_a_count() {
	let cases = [
		(
			&["--benchmark", "--repeat"][..],
			"error: --repeat needs a value\n",
		),
		(
			&["--benchmark", "--repeat", "many"],
			"error: invalid digit found in string\n",
		),
	];
	for (args, message) in cases {
		let run = poopterm(args);
		assert_eq!(run.status.code(), Some(1), "{args:?}");
		assert!(run.stdout.is_empty(), "nothing is measured: {args:?}");
		assert_eq!(text(&run.stderr), message, "{args:?}");
	}
}

#[test]
fn an_unknown_workload_is_refused_before_anything_runs() {
	let run = poopterm(&["--benchmark", "--only", "log,nope", "--kib", "1"]);
	assert_eq!(run.status.code(), Some(1));
	assert!(run.stdout.is_empty());
	assert!(
		text(&run.stderr).contains("unknown workload: nope"),
		"{}",
		text(&run.stderr)
	);
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

/// Measuring the terminal with none to measure fails at once, saying why,
/// instead of a bare "No such device or address".
#[cfg(target_os = "linux")]
#[test]
fn measuring_the_terminal_without_one_says_it_needs_one() {
	for stages in [&["--terminal"][..], &["--stages", "parse,terminal-raw"]] {
		let run = Command::new("setsid")
			.arg(env!("CARGO_BIN_EXE_poopterm"))
			.args(["--benchmark", "--kib", "4"])
			.args(stages)
			.output()
			.expect("setsid runs");
		assert!(!run.status.success(), "{stages:?}");
		let error = text(&run.stderr);
		let why = "--terminal needs a terminal to run in: ";
		assert!(error.contains(why), "{error}");
		assert!(run.stdout.is_empty());
	}
}
