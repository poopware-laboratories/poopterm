//! Running every selected workload through every stage several times and
//! collecting the results into a [`Table`].

use super::{
	Options,
	corpus::Size,
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
}

/// Every stage with its column heading, in the order they run.
const STAGES: [(Stage, &str); 2] = [(Stage::Parse, "parse"), (Stage::Render, "+render")];

/// Runs every selected workload through every stage: one untimed warm-up
/// on an eighth of its bytes, then `repeat` timed runs per stage.
/// `measure` returns MB/s; the benchmark passes the real one, tests a fake.
pub(crate) fn report(options: &Options, mut measure: impl FnMut(Stage, &[u8]) -> f64) -> Table {
	let stages = &STAGES[..if options.render { 2 } else { 1 }];
	let mut rows = Vec::new();
	for workload in selected(options) {
		let data = workload.build(options.bytes, options.size);
		measure(Stage::Parse, &data[..data.len() / 8]);
		let cells = stages.iter().map(|&(stage, _)| {
			let rates: Vec<f64> = (0..options.repeat).map(|_| measure(stage, &data)).collect();
			Spread::of(&rates)
		});
		rows.push(Row {
			name: workload.name,
			bytes: data.len(),
			cells: cells.collect(),
		});
	}
	let Size { cols, rows: lines } = options.size;
	Table {
		title: format!(
			"poopterm {} headless, {cols}x{lines}, {} KiB per workload",
			env!("CARGO_PKG_VERSION"),
			options.bytes >> 10
		),
		columns: stages.iter().map(|&(_, heading)| heading).collect(),
		rows,
	}
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
			calls.len() as f64
		});
		(table, calls)
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
			format!("poopterm {version} headless, 90x26, 8 KiB per workload")
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
			1.0
		});
		let mut second = Vec::new();
		report(&options("200x50"), |_, data| {
			second.push(data.to_vec());
			1.0
		});
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
