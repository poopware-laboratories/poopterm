//! Property tests of the Diagnostics page's log: for random diagnostics and
//! room, it counts what it leaves out, shows the newest warnings oldest
//! first, and says of each what the copied report says.
use super::diagnostics::{clock, private, report};
use super::diagnostics_page::log_rows;
use super::diagnostics_property_tests::{Random, diagnostics};

#[test]
fn log_rows_count_what_is_left_out_and_show_the_newest_in_order() {
	let mut random = Random(0xbb67_ae85_84ca_a73b);
	for case in 0..3000 {
		let diagnostics = diagnostics(&mut random);
		let rows = random.below(16);
		let whole = log_rows(&diagnostics, rows);
		assert!(whole.len() <= rows, "case {case}");
		let (kept, dropped) = (diagnostics.warnings.len(), diagnostics.dropped);
		if rows == 0 || kept + dropped == 0 {
			let expected: &[&str] = if rows == 0 { &[] } else { &["No warnings."] };
			assert_eq!(whole, expected, "case {case}");
			continue;
		}
		let noted = whole[0].contains(" older warning");
		assert_eq!(noted, dropped > 0 || kept > rows, "case {case}: {whole:?}");
		let listed = whole.len() - usize::from(noted);
		// Every row is used while there is something to put in it.
		assert_eq!(listed, kept.min(rows - usize::from(noted)), "case {case}");
		if noted {
			let hidden = kept - listed + dropped;
			let plural = if hidden == 1 { "" } else { "s" };
			let note = format!("{hidden} older warning{plural} not shown");
			assert_eq!(whole[0], note, "case {case}");
		}
		let home = diagnostics.home.as_deref();
		let newest = &diagnostics.warnings[kept - listed..];
		for (row, warning) in whole[usize::from(noted)..].iter().zip(newest) {
			let text = private(&warning.text, home);
			assert_eq!(
				*row,
				format!("{} UTC  {text}", clock(warning.at)),
				"case {case}"
			);
		}
	}
}

#[test]
fn a_log_with_room_for_everything_says_what_the_copied_report_says() {
	let mut random = Random(0x3c6e_f372_fe94_f82b);
	for case in 0..1000 {
		let diagnostics = diagnostics(&mut random);
		let room = diagnostics.warnings.len() + 1;
		let page: Vec<String> = log_rows(&diagnostics, room)
			.into_iter()
			.filter(|row| row.contains(" UTC  "))
			.collect();
		let report = report(&diagnostics);
		let (_, log) = report.split_once("\nLog\n").expect("a Log section");
		// The report pads the time into a column; the page keeps two spaces.
		let copied: Vec<String> = log
			.lines()
			.filter(|line| line.contains(" UTC "))
			.map(|line| {
				let (time, text) = line.trim_start().split_at("00:00:00 UTC".len());
				format!("{time}  {}", text.trim_start())
			})
			.collect();
		assert_eq!(page, copied, "case {case}");
		assert_eq!(page.len(), diagnostics.warnings.len(), "case {case}");
	}
}
