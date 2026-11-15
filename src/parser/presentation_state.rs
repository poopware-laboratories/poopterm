//! Presentation state: DECRQPSR (`CSI Ps $ w`) answers with the tab stop
//! report (DECTABSR, `Ps` 2), and DECRSPS (`DCS 2 $ t … ST`) restores the
//! tab stops such a report lists.
use crate::term::Term;

impl Term {
	/// DECRQPSR: reports `which` presentation state; others are ignored.
	pub(super) fn report_presentation(&mut self, which: u16) {
		let report = match which {
			2 => self.tab_stop_report(),
			_ => return,
		};
		self
			.responses
			.push(format!("\x1bP{which}$u{report}\x1b\\").into_bytes());
	}

	/// DECRSPS: restores the state a DECTABSR (2) describes.
	pub(super) fn restore_presentation(&mut self, which: u16, report: &[u8]) {
		if which == 2 {
			self.restore_tab_stops(report);
		}
	}

	/// The columns with a tab stop, from 1, separated by `/`.
	fn tab_stop_report(&self) -> String {
		let stops: Vec<String> = (self.tab_stops.iter().enumerate())
			.filter(|&(_, &stop)| stop)
			.map(|(col, _)| (col + 1).to_string())
			.collect();
		stops.join("/")
	}

	/// Clears every tab stop, then sets the listed columns that are on the
	/// screen; entries that are not numbers are skipped.
	fn restore_tab_stops(&mut self, report: &[u8]) {
		self.tab_stops.fill(false);
		for entry in report.split(|&byte| byte == b'/') {
			if let Some(col) = number(entry).filter(|&col| col >= 1)
				&& let Some(stop) = self.tab_stops.get_mut(usize::from(col) - 1)
			{
				*stop = true;
			}
		}
	}
}

/// A decimal parameter of one to five digits that fits in 16 bits.
fn number(field: &[u8]) -> Option<u16> {
	if field.is_empty() || field.len() > 5 || !field.iter().all(u8::is_ascii_digit) {
		return None;
	}
	std::str::from_utf8(field).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
	use super::number;

	#[test]
	fn numbers_are_one_to_five_plain_digits() {
		assert_eq!(number(b"7"), Some(7));
		assert_eq!(number(b"009"), Some(9), "leading zeros are fine");
		assert_eq!(number(b"65535"), Some(65535));
		for bad in [
			&b""[..],
			b"65536",
			b"99999",
			b"123456",
			b"+1",
			b"-1",
			b" 1",
			b"1a",
			b"1.5",
		] {
			assert_eq!(number(bad), None, "{:?}", String::from_utf8_lossy(bad));
		}
	}
}
