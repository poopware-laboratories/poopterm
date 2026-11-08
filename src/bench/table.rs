//! The benchmark report: one row per workload, one column per stage, each
//! cell the median MB/s with the slowest and fastest run, and an overall row.

use super::stats::{Spread, overall};

/// One workload's results; `cells` follows the table's columns, `None`
/// where a stage did not run.
pub(crate) struct Row {
	pub name: &'static str,
	pub bytes: usize,
	pub cells: Vec<Option<Spread>>,
}

pub(crate) struct Table {
	pub title: String,
	pub columns: Vec<&'static str>,
	pub rows: Vec<Row>,
}

/// How a report is printed: aligned for reading in a terminal, or as a
/// Markdown table for pull requests and chat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Style {
	Text,
	Markdown,
}

/// `median (min–max)`, one decimal each; `-` for a stage that did not run.
pub(crate) fn cell(spread: Option<Spread>) -> String {
	match spread {
		Some(Spread { median, min, max }) => format!("{median:.1} ({min:.1}–{max:.1})"),
		None => "-".into(),
	}
}

impl Table {
	/// The overall rate of column `at`, over the workloads that ran it;
	/// `None` when any of them lacks it, so totals never mix workload sets.
	fn overall(&self, at: usize) -> Option<f64> {
		let runs: Option<Vec<_>> = (self.rows.iter())
			.map(|row| Some((row.bytes, row.cells.get(at).copied().flatten()?.median)))
			.collect();
		overall(runs?)
	}

	/// The report's lines as cells: the headings, a line per workload, and
	/// the overall rates.
	fn lines(&self) -> Vec<Vec<String>> {
		let columns = 0..self.columns.len();
		let headings = ["workload"].iter().chain(&self.columns);
		let mut lines = vec![headings.map(|heading| heading.to_string()).collect()];
		for row in &self.rows {
			let cells = columns
				.clone()
				.map(|at| cell(row.cells.get(at).copied().flatten()));
			lines.push([row.name.to_owned()].into_iter().chain(cells).collect());
		}
		let rate = |at| {
			self
				.overall(at)
				.map_or("-".into(), |rate| format!("{rate:.1}"))
		};
		lines.push(
			[String::from("overall")]
				.into_iter()
				.chain(columns.map(rate))
				.collect(),
		);
		lines
	}

	pub(crate) fn render(&self, style: Style) -> String {
		let lines = self.lines();
		if style == Style::Markdown {
			let row = |cells: &Vec<String>| format!("| {} |\n", cells.join(" | "));
			let rule = format!("|---|{}\n", "--:|".repeat(self.columns.len()));
			let body: String = lines[1..].iter().map(row).collect();
			let title = format!("**{}**, MB/s, median (min–max)\n\n", self.title);
			return title + &row(&lines[0]) + &rule + &body;
		}
		let width = |at: usize| lines.iter().map(|line| line[at].chars().count()).max();
		let widths: Vec<usize> = (0..lines[0].len())
			.map(|at| width(at).unwrap_or(0))
			.collect();
		let mut out = format!("{}\nMB/s, median (min–max)\n", self.title);
		for line in &lines {
			let padded: String = line
				.iter()
				.zip(&widths)
				.map(|(cell, width)| format!("{cell:<width$}  "))
				.collect();
			out += padded.trim_end();
			out.push('\n');
		}
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn spread(median: f64, min: f64, max: f64) -> Option<Spread> {
		Some(Spread { median, min, max })
	}

	fn table() -> Table {
		Table {
			title: "poopterm 1.2.3 headless, 90x26".into(),
			columns: vec!["parse", "+render"],
			rows: vec![
				Row {
					name: "log",
					bytes: 100_000_000,
					cells: vec![spread(100.0, 98.04, 101.96), spread(50.0, 49.0, 51.0)],
				},
				Row {
					name: "emoji",
					bytes: 100_000_000,
					cells: vec![spread(25.0, 24.0, 26.0), None],
				},
			],
		}
	}

	#[test]
	fn a_cell_shows_the_median_and_the_spread() {
		assert_eq!(cell(spread(395.62, 390.11, 401.26)), "395.6 (390.1–401.3)");
	}

	#[test]
	fn a_stage_that_did_not_run_is_a_dash() {
		assert_eq!(cell(None), "-");
	}

	#[test]
	fn the_overall_weighs_every_workload_by_its_time() {
		// 200 MB in 1 s + 4 s.
		assert_eq!(table().overall(0), Some(40.0));
	}

	#[test]
	fn the_overall_weighs_workloads_by_their_bytes() {
		let mut table = table();
		table.rows[0].bytes = 300_000_000;
		// 400 MB in 3 s + 4 s, at the medians, not at the slowest runs.
		let rate = table.overall(0).unwrap();
		assert!((rate - 400.0 / 7.0).abs() < 1e-9, "{rate}");
		let text = table.render(Style::Text);
		assert!(
			text.ends_with("overall   57.1                -\n"),
			"{text}"
		);
	}

	#[test]
	fn a_column_some_workloads_skipped_has_no_overall() {
		assert_eq!(table().overall(1), None);
	}

	#[test]
	fn text_aligns_columns_under_their_headings() {
		assert_eq!(
			table().render(Style::Text),
			"poopterm 1.2.3 headless, 90x26\n\
			 MB/s, median (min–max)\n\
			 workload  parse               +render\n\
			 log       100.0 (98.0–102.0)  50.0 (49.0–51.0)\n\
			 emoji     25.0 (24.0–26.0)    -\n\
			 overall   40.0                -\n"
		);
	}

	#[test]
	fn markdown_is_a_table_with_right_aligned_numbers() {
		assert_eq!(
			table().render(Style::Markdown),
			"**poopterm 1.2.3 headless, 90x26**, MB/s, median (min–max)\n\
			 \n\
			 | workload | parse | +render |\n\
			 |---|--:|--:|\n\
			 | log | 100.0 (98.0–102.0) | 50.0 (49.0–51.0) |\n\
			 | emoji | 25.0 (24.0–26.0) | - |\n\
			 | overall | 40.0 | - |\n"
		);
	}

	#[test]
	fn a_long_workload_name_widens_its_column() {
		let mut table = table();
		table.rows[0].name = "kitty-rgba-long";
		let text = table.render(Style::Text);
		let lines: Vec<&str> = text.lines().collect();
		assert!(lines[2].starts_with("workload         parse"), "{text}");
		assert!(lines[3].starts_with("kitty-rgba-long  100.0"), "{text}");
	}

	#[test]
	fn a_row_shorter_than_the_columns_shows_dashes_for_the_rest() {
		let mut table = table();
		table.rows[0].cells.truncate(1);
		table.rows[1].cells.clear();
		let text = table.render(Style::Text);
		let lines: Vec<&str> = text.lines().collect();
		assert_eq!(lines[3], "log       100.0 (98.0–102.0)  -");
		assert_eq!(lines[4], "emoji     -                   -");
		assert_eq!(lines[5], "overall   -                   -");
	}

	#[test]
	fn markdown_with_one_column_aligns_one_number_column() {
		let mut table = table();
		table.columns.truncate(1);
		let markdown = table.render(Style::Markdown);
		let lines: Vec<&str> = markdown.lines().collect();
		assert_eq!(lines[2], "| workload | parse |");
		assert_eq!(lines[3], "|---|--:|");
		assert_eq!(lines[4], "| log | 100.0 (98.0–102.0) |");
	}

	#[test]
	fn an_empty_table_still_has_its_heading_and_no_overall() {
		let table = Table {
			title: "t".into(),
			columns: vec!["parse"],
			rows: Vec::new(),
		};
		assert_eq!(
			table.render(Style::Text),
			"t\nMB/s, median (min–max)\nworkload  parse\noverall   -\n"
		);
	}
}
