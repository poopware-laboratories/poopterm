//! Presentation state: DECRQPSR (`CSI Ps $ w`) answers with the cursor
//! information report (DECCIR, `Ps` 1) or the tab stop report (DECTABSR,
//! 2), and DECRSPS (`DCS Ps $ t … ST`) restores the state such a report
//! describes. Only G0 and G1 exist here, so G2 and G3 are reported as ASCII
//! and every set as a 94-character one.
use crate::term::{Term, attr::*};

/// Rendition bits in DECCIR's `Srend`, on top of `@`.
const BOLD: u8 = 1;
const UNDERLINED: u8 = 2;
const BLINKING: u8 = 4;
const REVERSED: u8 = 8;
/// DECCIR's `Satt`: DECSCA protection.
const PROTECTED: u8 = 1;
/// Flags in DECCIR's `Sflag`: origin mode and a pending wrap.
const ORIGIN: u8 = 1;
const WRAP: u8 = 8;

impl Term {
	/// DECRQPSR: reports `which` presentation state; others are ignored.
	pub(super) fn report_presentation(&mut self, which: u16) {
		let report = match which {
			1 => self.cursor_information(),
			2 => self.tab_stop_report(),
			_ => return,
		};
		self
			.responses
			.push(format!("\x1bP{which}$u{report}\x1b\\").into_bytes());
	}

	/// DECRSPS: restores the state a DECCIR (1) or DECTABSR (2) describes.
	pub(super) fn restore_presentation(&mut self, which: u16, report: &[u8]) {
		match which {
			1 => {
				self.restore_cursor_information(report);
			}
			2 => self.restore_tab_stops(report),
			_ => {}
		}
	}

	/// DECCIR: position, rendition, flags, GL and the four character sets.
	fn cursor_information(&self) -> String {
		let (row, col) = self.reported_cursor();
		let rendition = flags([
			(self.bold, BOLD),
			(self.attrs & UNDERLINE != 0, UNDERLINED),
			(self.attrs & BLINK != 0, BLINKING),
			(self.reverse, REVERSED),
		]);
		let protection = flags([(self.protected, PROTECTED)]);
		let state = flags([(self.origin_mode, ORIGIN), (self.wrap_pending, WRAP)]);
		let [g0, g1] = self
			.charsets
			.map(|graphics| if graphics { '0' } else { 'B' });
		let gl = self.active_charset;
		format!("{row};{col};1;{rendition};{protection};{state};{gl};2;@;{g0}{g1}BB")
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

	/// Applies a DECCIR report, or nothing if it is malformed. Its fields:
	/// row, column, page, rendition, protection, flags, GL, GR, set sizes
	/// and the designators of G0–G3.
	fn restore_cursor_information(&mut self, report: &[u8]) -> Option<()> {
		let fields: Vec<&[u8]> = report.split(|&byte| byte == b';').collect();
		if fields.len() != 10 {
			return None;
		}
		let (row, col, gl) = (number(fields[0])?, number(fields[1])?, number(fields[6])?);
		let (rendition, state) = (flag_byte(fields[3])?, flag_byte(fields[5])?);
		let protection = flag_byte(fields[4])?;
		let sets = designators(fields[9])?;
		self.protected = protection & PROTECTED != 0;
		self.update_active_extra();
		self.bold = rendition & BOLD != 0;
		self.reverse = rendition & REVERSED != 0;
		self.attrs &= !(UNDERLINE | DOUBLE_UNDERLINE | BLINK);
		for (bit, attr) in [(UNDERLINED, UNDERLINE), (BLINKING, BLINK)] {
			if rendition & bit != 0 {
				self.attrs |= attr;
			}
		}
		self.origin_mode = state & ORIGIN != 0;
		self.apply_csi_dispatch(&[row, col][..], b"", false, 'H');
		self.wrap_pending = state & WRAP != 0;
		if gl <= 1 {
			self.active_charset = usize::from(gl);
		}
		self.charsets = [sets[0] == b"0", sets[1] == b"0"];
		Some(())
	}
}

/// `@` plus the bits whose condition holds, as DECCIR's flag characters.
fn flags<const N: usize>(bits: [(bool, u8); N]) -> char {
	let value = (bits.iter())
		.filter(|(on, _)| *on)
		.fold(0x40, |all, (_, bit)| all | bit);
	char::from(value)
}

/// A single flag character, `@` through `~`, as its bits.
fn flag_byte(field: &[u8]) -> Option<u8> {
	match field {
		&[byte @ 0x40..=0x7e] => Some(byte & 0x3f),
		_ => None,
	}
}

/// A decimal parameter of one to five digits that fits in 16 bits.
fn number(field: &[u8]) -> Option<u16> {
	if field.is_empty() || field.len() > 5 || !field.iter().all(u8::is_ascii_digit) {
		return None;
	}
	std::str::from_utf8(field).ok()?.parse().ok()
}

/// The four designators of G0–G3: each is intermediates (`SP` to `/`)
/// followed by a final character (`0` to `~`).
fn designators(field: &[u8]) -> Option<[&[u8]; 4]> {
	let mut sets = [&field[..0]; 4];
	let mut rest = field;
	for set in &mut sets {
		let end = rest.iter().position(|byte| (0x30..=0x7e).contains(byte))?;
		if !rest[..end].iter().all(|byte| (0x20..=0x2f).contains(byte)) {
			return None;
		}
		(*set, rest) = rest.split_at(end + 1);
	}
	rest.is_empty().then_some(sets)
}

#[cfg(test)]
mod tests {
	use super::{designators, flag_byte, flags, number};

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

	#[test]
	fn flags_add_the_set_bits_to_an_at_sign() {
		assert_eq!(flags([(false, 1), (false, 8)]), '@');
		assert_eq!(flags([(true, 1), (false, 2), (true, 8)]), 'I');
		assert_eq!(flags([(true, 1), (true, 2), (true, 4), (true, 8)]), 'O');
		assert_eq!(flags::<0>([]), '@');
	}

	#[test]
	fn flag_bytes_are_one_character_from_at_to_tilde() {
		assert_eq!(flag_byte(b"@"), Some(0));
		assert_eq!(flag_byte(b"O"), Some(15));
		assert_eq!(flag_byte(b"~"), Some(0x3e));
		for bad in [&b""[..], b"?", b"\x7f", b" ", b"@@", b"A@", b"1"] {
			assert_eq!(flag_byte(bad), None, "{:?}", String::from_utf8_lossy(bad));
		}
	}

	#[test]
	fn flag_bytes_read_back_what_flags_wrote() {
		for bits in 0..16u8 {
			let on = |bit: u8| (bits & bit != 0, bit);
			let flag = flags([on(1), on(2), on(4), on(8)]);
			assert_eq!(flag_byte(&[flag as u8]), Some(bits), "{flag}");
		}
	}

	#[test]
	fn designators_are_four_finals_with_optional_intermediates() {
		let sets = designators(b"B0BB").unwrap();
		assert_eq!(sets, [&b"B"[..], b"0", b"B", b"B"]);
		// Intermediates belong to the set that follows them: `%5` is DEC
		// Supplemental Graphic and `"?` DEC Greek.
		let sets = designators(b"B%5\"?<").unwrap();
		assert_eq!(sets, [&b"B"[..], b"%5", b"\"?", b"<"]);
		// `-` and `.` are 96-character intermediates; `~` is a valid final.
		let sets = designators(b"-A.~BB").unwrap();
		assert_eq!(sets, [&b"-A"[..], b".~", b"B", b"B"]);
	}

	#[test]
	fn designators_reject_the_wrong_count_or_bytes() {
		for bad in [
			&b""[..],
			b"BBB",
			b"BBBBB",
			b"BB%",
			b"BBB%",
			b"B!B\x7fB",
			b"BB\x1fBB",
			b"BBB\x80",
			b"%%%%",
		] {
			assert_eq!(designators(bad), None, "{:?}", String::from_utf8_lossy(bad));
		}
	}
}
