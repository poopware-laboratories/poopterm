//! Saving the scrollback: the text of every line kept, history and screen,
//! and the dated name of the file it goes to.
use crate::term::Term;

/// Every line of `term`'s history and screen as plain text, oldest first:
/// each row's trailing blanks and the blank rows at the end left out, wide
/// characters once, combining marks kept.
pub(crate) fn history_text(term: &Term) -> String {
	let rows = term.scrollback.len() + term.rows;
	let mut lines: Vec<String> = (0..rows)
		.map(|row| {
			let cells = term.row_cells(row).unwrap_or_default();
			let visible = cells.iter().filter(|cell| cell.width != 0);
			let line: String = visible
				.flat_map(|cell| std::iter::once(cell.ch).chain(term.marks(cell).iter().copied()))
				.collect();
			line.trim_end_matches([' ', '\0']).to_owned()
		})
		.collect();
	while lines.last().is_some_and(String::is_empty) {
		lines.pop();
	}
	let mut text = lines.join("\n");
	if !text.is_empty() {
		text.push('\n');
	}
	text
}

/// The file name for a scrollback saved at `unix` seconds, in UTC:
/// `poopterm-scrollback-20261122-180501.txt`.
pub(crate) fn file_name(unix: u64) -> String {
	let (days, seconds) = (unix / 86_400, unix % 86_400);
	let (year, month, day) = civil(days);
	let (hour, minute, second) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
	format!("poopterm-scrollback-{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}.txt")
}

/// The Gregorian date `days` after 1970-01-01.
fn civil(days: u64) -> (u64, u64, u64) {
	// Days since 0000-03-01, in 400-year eras of 146097 days.
	let z = days + 719_468;
	let (era, day_of_era) = (z / 146_097, z % 146_097);
	let year_of_era =
		(day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_index = (5 * day_of_year + 2) / 153;
	let day = day_of_year - (153 * month_index + 2) / 5 + 1;
	let month = if month_index < 10 {
		month_index + 3
	} else {
		month_index - 9
	};
	(era * 400 + year_of_era + u64::from(month <= 2), month, day)
}
