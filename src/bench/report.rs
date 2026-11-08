//! Running every selected workload through every stage several times and
//! collecting the results into a [`Table`].

use std::error::Error;

use super::{
	Options,
	corpus::Size,
	pty::Mode,
	selected,
	stats::Spread,
	table::{Row, Table},
};

/// What one measurement covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stage {
	/// The parser and terminal state only.
	Parse,
	/// The same plus a software-rendered frame every `--frame-every` bytes.
	Render,
	/// `cat` writing the workload into a PTY, read and discarded: the
	/// kernel's ceiling.
	Pty(Mode),
	/// The same read by a tab's reader thread, queue and parser thread.
	Pipeline(Mode),
}

/// Every stage with its `--stages` name and column heading, in the order
/// they run.
const STAGES: [(Stage, &str, &str); 6] = [
	(Stage::Parse, "parse", "parse"),
	(Stage::Render, "render", "+render"),
	(Stage::Pty(Mode::Cooked), "pty", "pty"),
	(Stage::Pty(Mode::Raw), "pty-raw", "pty raw"),
	(Stage::Pipeline(Mode::Cooked), "pipeline", "+pipeline"),
	(Stage::Pipeline(Mode::Raw), "pipeline-raw", "+pipeline raw"),
];

/// Stages without a PTY, run unless `--stages` says otherwise.
pub(crate) const HEADLESS: [Stage; 2] = [Stage::Parse, Stage::Render];

/// A pipeline this close to its PTY ceiling is limited by the kernel.
const KERNEL_BOUND: f64 = 0.9;

/// The stages a comma-separated `--stages` list names; `all` names every one.
pub(crate) fn stages(list: &str) -> Result<Vec<Stage>, String> {
	let mut chosen = Vec::new();
	for name in list.split(',') {
		let named = STAGES
			.iter()
			.filter(|&&(_, known, _)| name == "all" || known == name);
		let before = chosen.len();
		chosen.extend(named.map(|&(stage, ..)| stage));
		if chosen.len() == before {
			let known: Vec<&str> = STAGES.iter().map(|&(_, name, _)| name).collect();
			let known = known.join(", ");
			return Err(format!(
				"unknown stage: {name}; the stages are {known}, all"
			));
		}
	}
	Ok(chosen)
}

/// Runs every selected workload through every stage: one untimed warm-up
/// on an eighth of its bytes, then `repeat` timed runs per stage.
/// `measure` returns MB/s; the benchmark passes the real one, tests a fake.
pub(crate) fn report(
	options: &Options,
	mut measure: impl FnMut(Stage, &[u8]) -> Result<f64, Box<dyn Error>>,
) -> Result<Table, Box<dyn Error>> {
	let stages: Vec<_> = STAGES
		.iter()
		.filter(|(stage, ..)| options.stages.contains(stage))
		.collect();
	let mut rows = Vec::new();
	for workload in selected(options) {
		let data = workload.build(options.bytes, options.size);
		measure(Stage::Parse, &data[..data.len() / 8])?;
		let mut cells = Vec::new();
		for &&(stage, ..) in &stages {
			let rates = (0..options.repeat).map(|_| measure(stage, &data));
			cells.push(Spread::of(&rates.collect::<Result<Vec<_>, _>>()?));
		}
		rows.push(Row {
			name: workload.name,
			bytes: data.len(),
			cells,
		});
	}
	let Size { cols, rows: lines } = options.size;
	let mut table = Table {
		title: format!(
			"poopterm {}, {cols}x{lines}, {} KiB per workload",
			env!("CARGO_PKG_VERSION"),
			options.bytes >> 10
		),
		columns: stages.iter().map(|&&(.., heading)| heading).collect(),
		rows,
		notes: Vec::new(),
	};
	let order: Vec<Stage> = stages.iter().map(|&&(stage, ..)| stage).collect();
	table.notes = kernel_bound(&table, &order);
	Ok(table)
}

/// For each mode with both a PTY ceiling and a pipeline column, the
/// workloads whose pipeline reaches [`KERNEL_BOUND`] of the ceiling: there
/// the kernel, not Poopterm, sets the speed.
fn kernel_bound(table: &Table, order: &[Stage]) -> Vec<String> {
	let column = |wanted: Stage| order.iter().position(|&stage| stage == wanted);
	let mut notes = Vec::new();
	for mode in [Mode::Cooked, Mode::Raw] {
		let (Some(ceiling), Some(pipeline)) = (column(Stage::Pty(mode)), column(Stage::Pipeline(mode)))
		else {
			continue;
		};
		let median = |row: &Row, at: usize| row.cells[at].map(|spread| spread.median);
		let bound: Vec<&str> = (table.rows.iter())
			.filter(|row| match (median(row, ceiling), median(row, pipeline)) {
				(Some(ceiling), Some(pipeline)) => pipeline >= KERNEL_BOUND * ceiling,
				_ => false,
			})
			.map(|row| row.name)
			.collect();
		if !bound.is_empty() {
			let (pipe, pty) = (table.columns[pipeline], table.columns[ceiling]);
			notes.push(format!(
				"kernel-bound ({pipe} ≥ 90% of {pty}): {}",
				bound.join(", ")
			));
		}
	}
	notes
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bench::{options, table::Style};

	/// The report of a fake run over `args` whose rates count up from 1 in
	/// call order, with each call's stage and length.
	fn fake_report(args: &[&str]) -> (Table, Vec<(Stage, usize)>) {
		let options = options(args.iter().map(|arg| arg.to_string())).unwrap();
		let mut calls = Vec::new();
		let table = report(&options, |stage, data| {
			calls.push((stage, data.len()));
			Ok(calls.len() as f64)
		})
		.unwrap();
		(table, calls)
	}

	/// A report whose rates come from `rate` by stage.
	fn report_of(args: &[&str], rate: impl Fn(Stage) -> f64) -> Table {
		let options = options(args.iter().map(|arg| arg.to_string())).unwrap();
		report(&options, |stage, _| Ok(rate(stage))).unwrap()
	}

	#[test]
	fn stages_are_named_by_comma_separated_lists() {
		assert_eq!(stages("parse"), Ok(vec![Stage::Parse]));
		assert_eq!(
			stages("pty,pipeline-raw"),
			Ok(vec![Stage::Pty(Mode::Cooked), Stage::Pipeline(Mode::Raw)])
		);
		let every: Vec<Stage> = STAGES.iter().map(|&(stage, ..)| stage).collect();
		assert_eq!(stages("all"), Ok(every));
	}

	#[test]
	fn an_unknown_or_empty_stage_is_refused_with_the_known_ones() {
		let known = "parse, render, pty, pty-raw, pipeline, pipeline-raw, all";
		assert_eq!(
			stages("pty,fast"),
			Err(format!("unknown stage: fast; the stages are {known}"))
		);
		assert_eq!(
			stages(""),
			Err(format!("unknown stage: ; the stages are {known}"))
		);
		assert!(stages("parse,").is_err());
	}

	#[test]
	fn columns_follow_the_stage_order_not_the_order_asked() {
		let (table, _) = fake_report(&[
			"--only",
			"log",
			"--kib",
			"8",
			"--stages",
			"pipeline,parse,pty",
		]);
		assert_eq!(table.columns, ["parse", "pty", "+pipeline"]);
	}

	#[test]
	fn pty_stages_are_measured_like_any_other() {
		let args = [
			"--only",
			"log",
			"--kib",
			"8",
			"--stages",
			"pty-raw,pipeline-raw",
			"--repeat",
			"1",
		];
		let (table, calls) = fake_report(&args);
		let stages: Vec<Stage> = calls.iter().map(|&(stage, _)| stage).collect();
		// The warm-up stays headless: it only warms caches and allocators.
		assert_eq!(
			stages,
			[
				Stage::Parse,
				Stage::Pty(Mode::Raw),
				Stage::Pipeline(Mode::Raw)
			]
		);
		assert_eq!(table.columns, ["pty raw", "+pipeline raw"]);
	}

	#[test]
	fn a_failed_measurement_stops_the_report() {
		let options = options(
			["--only", "log,tui", "--kib", "8"]
				.map(String::from)
				.into_iter(),
		)
		.unwrap();
		let mut calls = 0;
		let result = report(&options, |_, _| {
			calls += 1;
			if calls == 3 {
				Err("no PTY".into())
			} else {
				Ok(1.0)
			}
		});
		assert_eq!(result.err().unwrap().to_string(), "no PTY");
		assert_eq!(calls, 3, "nothing runs after the failure");
	}

	#[test]
	fn a_pipeline_near_its_ceiling_is_named_kernel_bound() {
		let args = [
			"--only",
			"log,tui",
			"--kib",
			"8",
			"--stages",
			"pty,pipeline",
		];
		// Exactly 90% of the ceiling counts.
		let table = report_of(&args, |stage| match stage {
			Stage::Pty(_) => 100.0,
			_ => 90.0,
		});
		assert_eq!(
			table.notes,
			["kernel-bound (+pipeline ≥ 90% of pty): log, tui"]
		);
		let table = report_of(&args, |stage| match stage {
			Stage::Pty(_) => 100.0,
			_ => 89.9,
		});
		assert!(table.notes.is_empty(), "{:?}", table.notes);
	}

	#[test]
	fn only_the_workloads_near_their_ceiling_are_named() {
		let args = ["--only", "log,tui", "--kib", "8", "--repeat", "1"];
		let options = options(args.iter().map(|arg| arg.to_string()));
		let mut options = options.unwrap();
		options.stages = vec![Stage::Pty(Mode::Raw), Stage::Pipeline(Mode::Raw)];
		// Per workload: the warm-up, the ceiling, then the pipeline; log's
		// pipeline is near its ceiling, tui's far below.
		let mut rates = [1.0, 100.0, 95.0, 1.0, 100.0, 50.0].into_iter();
		let table = report(&options, |_, _| Ok(rates.next().unwrap())).unwrap();
		assert_eq!(
			table.notes,
			["kernel-bound (+pipeline raw ≥ 90% of pty raw): log"]
		);
	}

	#[test]
	fn a_workload_missing_either_rate_is_never_kernel_bound() {
		let spread = |median| {
			Some(Spread {
				median,
				min: median,
				max: median,
			})
		};
		let row = |name, cells| Row {
			name,
			bytes: 1,
			cells,
		};
		let table = Table {
			title: String::new(),
			columns: vec!["pty", "+pipeline"],
			rows: vec![
				row("log", vec![spread(100.0), None]),
				row("tui", vec![None, spread(100.0)]),
				row("emoji", vec![spread(100.0), spread(100.0)]),
			],
			notes: Vec::new(),
		};
		let order = [Stage::Pty(Mode::Cooked), Stage::Pipeline(Mode::Cooked)];
		assert_eq!(
			kernel_bound(&table, &order),
			["kernel-bound (+pipeline ≥ 90% of pty): emoji"]
		);
	}

	#[test]
	fn a_stage_named_twice_runs_once() {
		let args = ["--only", "log", "--kib", "8", "--repeat", "2"];
		let mut args = args.to_vec();
		args.extend(["--stages", "pty,all,pty"]);
		let (table, calls) = fake_report(&args);
		assert_eq!(table.columns.len(), STAGES.len());
		// The warm-up, then two runs of each stage.
		assert_eq!(calls.len(), 1 + 2 * STAGES.len());
	}

	#[test]
	fn the_note_is_printed_under_the_table() {
		let args = [
			"--only",
			"log",
			"--kib",
			"8",
			"--stages",
			"pty-raw,pipeline-raw",
		];
		let table = report_of(&args, |_| 100.0);
		let text = table.render(Style::Text);
		assert!(
			text.ends_with("\nkernel-bound (+pipeline raw ≥ 90% of pty raw): log\n"),
			"{text}"
		);
		assert_eq!(
			text
				.lines()
				.filter(|line| line.starts_with("kernel"))
				.count(),
			1
		);
	}

	#[test]
	fn each_mode_is_compared_with_its_own_ceiling() {
		let args = ["--only", "log", "--kib", "8", "--stages", "all"];
		let table = report_of(&args, |stage| match stage {
			Stage::Pty(Mode::Cooked) => 100.0,
			Stage::Pipeline(Mode::Cooked) => 95.0,
			Stage::Pty(Mode::Raw) => 400.0,
			// Above the cooked ceiling, but far below its own.
			Stage::Pipeline(Mode::Raw) => 200.0,
			_ => 300.0,
		});
		assert_eq!(table.notes, ["kernel-bound (+pipeline ≥ 90% of pty): log"]);
		let table = report_of(&args, |stage| match stage {
			Stage::Pty(_) => 100.0,
			_ => 99.0,
		});
		assert_eq!(
			table.notes,
			[
				"kernel-bound (+pipeline ≥ 90% of pty): log",
				"kernel-bound (+pipeline raw ≥ 90% of pty raw): log"
			]
		);
	}

	#[test]
	fn without_a_ceiling_nothing_is_called_kernel_bound() {
		let table = report_of(
			&["--only", "log", "--kib", "8", "--stages", "pipeline"],
			|_| 1.0,
		);
		assert!(table.notes.is_empty());
	}

	fn names(table: &Table) -> Vec<&str> {
		table.rows.iter().map(|row| row.name).collect()
	}

	#[test]
	fn each_stage_is_timed_repeat_times_after_one_warm_up() {
		let (table, calls) = fake_report(&["--only", "log", "--kib", "64", "--repeat", "2"]);
		let len = table.rows[0].bytes;
		// Workloads end on whole bursts, so a little over the size asked.
		assert!((64 << 10..80 << 10).contains(&len), "{len}");
		let stages = [
			(Stage::Parse, len / 8),
			(Stage::Parse, len),
			(Stage::Parse, len),
			(Stage::Render, len),
			(Stage::Render, len),
		];
		assert_eq!(calls, stages);
		// The row counts the bytes built and measured, not the size asked.
		assert_ne!(len, 64 << 10);
		// The warm-up's rate (1) is not part of any spread.
		let parse = table.rows[0].cells[0].unwrap();
		assert_eq!((parse.median, parse.min, parse.max), (2.5, 2.0, 3.0));
		let render = table.rows[0].cells[1].unwrap();
		assert_eq!((render.median, render.min, render.max), (4.5, 4.0, 5.0));
	}

	#[test]
	fn without_rendering_there_is_only_the_parse_column() {
		let (table, calls) = fake_report(&["--only", "log,tui", "--kib", "8", "--no-render"]);
		assert_eq!(table.columns, ["parse"]);
		assert!(calls.iter().all(|(stage, _)| *stage == Stage::Parse));
		// Two workloads, a warm-up and three runs each.
		assert_eq!(calls.len(), 8);
		assert_eq!(names(&table), ["log", "tui"]);
		assert!(table.rows.iter().all(|row| row.cells.len() == 1));
	}

	#[test]
	fn rows_follow_the_corpus_order_not_the_order_asked() {
		let (table, _) = fake_report(&["--only", "tui,log", "--kib", "8", "--no-render"]);
		assert_eq!(names(&table), ["log", "tui"]);
	}

	#[test]
	fn every_workload_runs_when_none_is_named() {
		let (table, _) = fake_report(&["--kib", "1", "--no-render", "--repeat", "1"]);
		let all: Vec<&str> = crate::bench::corpus::WORKLOADS
			.iter()
			.map(|w| w.name)
			.collect();
		assert_eq!(names(&table), all);
	}

	#[test]
	fn one_run_reports_that_run_as_median_and_spread() {
		let (table, calls) = fake_report(&["--only", "log", "--kib", "8", "--repeat", "1"]);
		assert_eq!(calls.len(), 3);
		let parse = table.rows[0].cells[0].unwrap();
		assert_eq!((parse.median, parse.min, parse.max), (2.0, 2.0, 2.0));
	}

	#[test]
	fn the_title_names_the_build_size_and_bytes_asked() {
		let (table, _) = fake_report(&["--only", "log", "--kib", "8", "--size", "90x26"]);
		let version = env!("CARGO_PKG_VERSION");
		assert_eq!(
			table.title,
			format!("poopterm {version}, 90x26, 8 KiB per workload")
		);
		assert_eq!(table.columns, ["parse", "+render"]);
	}

	#[test]
	fn the_workloads_are_built_for_the_size_asked() {
		// A TUI redraw fills the screen, so a wider one sends more per frame;
		// the same byte budget then holds fewer, larger frames.
		let options = |size: &str| {
			let args = ["--only", "tui", "--kib", "8", "--size", size];
			options(args.iter().map(|arg| arg.to_string())).unwrap()
		};
		let mut first = Vec::new();
		report(&options("40x10"), |_, data| {
			first.push(data.to_vec());
			Ok(1.0)
		})
		.unwrap();
		let mut second = Vec::new();
		report(&options("200x50"), |_, data| {
			second.push(data.to_vec());
			Ok(1.0)
		})
		.unwrap();
		assert_ne!(first[1], second[1]);
	}

	#[test]
	fn a_fake_report_renders_with_an_overall_row() {
		let (table, _) = fake_report(&["--only", "log", "--kib", "8", "--no-render"]);
		let text = table.render(Style::Text);
		let last = text.lines().last().unwrap();
		assert!(last.starts_with("overall"), "{text}");
		// One workload: its overall is its own median.
		assert!(last.ends_with("3.0"), "{text}");
	}
}
