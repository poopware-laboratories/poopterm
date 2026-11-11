//! Mouse reports for applications that enable xterm mouse tracking.
use super::keys::Modifiers;

/// The button a report refers to, in xterm's numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseButtonCode {
	Left = 0,
	Middle = 1,
	Right = 2,
	/// Motion with no button held (any-event tracking, `?1003`).
	None = 3,
	WheelUp = 64,
	WheelDown = 65,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseAction {
	Press,
	Release,
	Motion,
}

/// One mouse event at a 1-based cell position.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MouseReport {
	pub button: MouseButtonCode,
	pub kind: MouseAction,
	pub col: usize,
	pub row: usize,
	pub mods: Modifiers,
}

/// How reports are written. xterm treats the extended encodings as one
/// setting: enabling one replaces the last, and resetting the one in use
/// returns to the legacy bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MouseEncoding {
	/// `CSI M` and three bytes, each value plus 32: cells up to 223.
	#[default]
	X10,
	/// `?1005`: the same, with each value written as a UTF-8 character.
	Utf8,
	/// `?1006`: `CSI < b ; x ; y M` (`m` on release), in decimal.
	Sgr,
	/// `?1015`: `CSI b ; x ; y M` in decimal, `b` as in the legacy form.
	Urxvt,
}

impl MouseEncoding {
	/// The encoding a private mode number turns on.
	pub(crate) fn from_mode(mode: u16) -> Option<Self> {
		match mode {
			1005 => Some(Self::Utf8),
			1006 => Some(Self::Sgr),
			1015 => Some(Self::Urxvt),
			_ => None,
		}
	}

	/// The private mode that turns this encoding on, if any.
	pub(crate) fn mode(self) -> Option<u16> {
		match self {
			Self::X10 => None,
			Self::Utf8 => Some(1005),
			Self::Sgr => Some(1006),
			Self::Urxvt => Some(1015),
		}
	}
}

/// The largest value `?1005` writes: 2015 + 32 is the last character UTF-8
/// encodes in two bytes, the limit xterm keeps to.
const MAX_UTF8_VALUE: usize = 2015;

/// Whether tracking mode `tracking` (`?9`, `?1000`, `?1002`, `?1003`, or 0)
/// reports this event.
pub(crate) fn tracked(tracking: u16, button: MouseButtonCode, kind: MouseAction) -> bool {
	match (tracking, kind) {
		// X10 compatibility: presses only.
		(9, MouseAction::Press) => true,
		(1000 | 1002 | 1003, MouseAction::Press | MouseAction::Release) => true,
		(1002, MouseAction::Motion) => button != MouseButtonCode::None,
		(1003, MouseAction::Motion) => true,
		_ => false,
	}
}

/// Encodes a report for an application tracking with `tracking`. Legacy
/// reports can only address cells up to 223 (2015 in `?1005`), so farther
/// events return `None`. X10 compatibility (`?9`) reports no modifiers.
pub(crate) fn encode_mouse(
	report: MouseReport,
	encoding: MouseEncoding,
	tracking: u16,
) -> Option<Vec<u8>> {
	let mods = if tracking == 9 {
		Modifiers::default()
	} else {
		report.mods
	};
	let mut code = report.button as usize
		+ 4 * usize::from(mods.shift)
		+ 8 * usize::from(mods.alt)
		+ 16 * usize::from(mods.ctrl);
	if report.kind == MouseAction::Motion {
		code += 32;
	}
	let release = report.kind == MouseAction::Release;
	let (col, row) = (report.col, report.row);
	if encoding == MouseEncoding::Sgr {
		let suffix = if release { 'm' } else { 'M' };
		return Some(format!("\x1b[<{code};{col};{row}{suffix}").into_bytes());
	}
	if release {
		// The legacy button code cannot say which button was released.
		code |= 3;
	}
	let mut bytes = b"\x1b[M".to_vec();
	match encoding {
		MouseEncoding::Urxvt => return Some(format!("\x1b[{};{col};{row}M", code + 32).into_bytes()),
		MouseEncoding::Utf8 => {
			if col > MAX_UTF8_VALUE || row > MAX_UTF8_VALUE {
				return None;
			}
			for value in [code, col, row] {
				let character = char::from_u32((value + 32) as u32)?;
				bytes.extend(character.encode_utf8(&mut [0; 4]).as_bytes());
			}
		}
		_ => {
			if col > 223 || row > 223 {
				return None;
			}
			bytes.extend([code, col, row].map(|value| (value + 32) as u8));
		}
	}
	Some(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::input::keys::Modifiers;

	fn at(button: MouseButtonCode, kind: MouseAction, col: usize, row: usize) -> MouseReport {
		MouseReport {
			button,
			kind,
			col,
			row,
			mods: Modifiers::default(),
		}
	}

	fn report(
		button: MouseButtonCode,
		kind: MouseAction,
		encoding: MouseEncoding,
	) -> Option<Vec<u8>> {
		encode_mouse(at(button, kind, 1, 2), encoding, 1000)
	}

	#[test]
	fn legacy_reports_use_raw_offset_bytes_not_decimal_digits() {
		assert_eq!(
			report(
				MouseButtonCode::Left,
				MouseAction::Press,
				MouseEncoding::X10
			)
			.unwrap(),
			b"\x1b[M\x20\x21\x22"
		);
		assert_eq!(
			report(
				MouseButtonCode::Left,
				MouseAction::Release,
				MouseEncoding::X10
			)
			.unwrap(),
			b"\x1b[M\x23\x21\x22",
			"X10 releases report button 3"
		);
	}

	#[test]
	fn sgr_reports_keep_the_button_on_release() {
		assert_eq!(
			report(
				MouseButtonCode::Right,
				MouseAction::Press,
				MouseEncoding::Sgr
			)
			.unwrap(),
			b"\x1b[<2;1;2M"
		);
		assert_eq!(
			report(
				MouseButtonCode::Right,
				MouseAction::Release,
				MouseEncoding::Sgr
			)
			.unwrap(),
			b"\x1b[<2;1;2m"
		);
	}

	#[test]
	fn motion_reports_the_held_button_or_none() {
		assert_eq!(
			report(
				MouseButtonCode::Middle,
				MouseAction::Motion,
				MouseEncoding::Sgr
			)
			.unwrap(),
			b"\x1b[<33;1;2M"
		);
		assert_eq!(
			report(
				MouseButtonCode::None,
				MouseAction::Motion,
				MouseEncoding::Sgr
			)
			.unwrap(),
			b"\x1b[<35;1;2M"
		);
	}

	#[test]
	fn wheel_and_modifiers_set_their_bits() {
		let mods = Modifiers {
			shift: true,
			ctrl: true,
			..Default::default()
		};
		let wheel = MouseReport {
			mods,
			..at(MouseButtonCode::WheelDown, MouseAction::Press, 3, 4)
		};
		assert_eq!(
			encode_mouse(wheel, MouseEncoding::Sgr, 1000).unwrap(),
			b"\x1b[<85;3;4M"
		);
	}

	#[test]
	fn legacy_reports_cannot_address_far_cells() {
		let far = at(MouseButtonCode::Left, MouseAction::Press, 224, 1);
		assert_eq!(encode_mouse(far, MouseEncoding::X10, 1000), None);
		assert!(encode_mouse(far, MouseEncoding::Sgr, 1000).is_some());
		let last = at(MouseButtonCode::Left, MouseAction::Press, 223, 223);
		assert_eq!(
			encode_mouse(last, MouseEncoding::X10, 1000).unwrap(),
			b"\x1b[M\x20\xff\xff"
		);
	}

	#[test]
	fn utf8_reports_match_legacy_ones_for_near_cells() {
		for kind in [
			MouseAction::Press,
			MouseAction::Release,
			MouseAction::Motion,
		] {
			let near = at(MouseButtonCode::Middle, kind, 95, 1);
			assert_eq!(
				encode_mouse(near, MouseEncoding::Utf8, 1003),
				encode_mouse(near, MouseEncoding::X10, 1003),
				"{kind:?}"
			);
		}
	}

	#[test]
	fn utf8_reports_encode_far_coordinates_as_characters() {
		// 96 + 32 is 128, the first value past ASCII: two bytes in UTF-8.
		let far = at(MouseButtonCode::Left, MouseAction::Press, 96, 224);
		let mut expected = b"\x1b[M ".to_vec();
		expected.extend("\u{80}\u{100}".as_bytes());
		assert_eq!(
			encode_mouse(far, MouseEncoding::Utf8, 1000).unwrap(),
			expected
		);
	}

	#[test]
	fn utf8_reports_reach_cell_2015_and_no_farther() {
		// 2015 + 32 is 2047, the last character UTF-8 writes in two bytes.
		let last = at(MouseButtonCode::Left, MouseAction::Press, 2015, 2015);
		let mut expected = b"\x1b[M ".to_vec();
		expected.extend("\u{7ff}\u{7ff}".as_bytes());
		assert_eq!(
			encode_mouse(last, MouseEncoding::Utf8, 1000).unwrap(),
			expected
		);
		for (col, row) in [(2016, 1), (1, 2016)] {
			let past = at(MouseButtonCode::Left, MouseAction::Press, col, row);
			assert_eq!(
				encode_mouse(past, MouseEncoding::Utf8, 1000),
				None,
				"{col},{row}"
			);
		}
	}

	#[test]
	fn urxvt_reports_are_decimal_with_the_legacy_button_code() {
		assert_eq!(
			report(
				MouseButtonCode::Left,
				MouseAction::Press,
				MouseEncoding::Urxvt
			)
			.unwrap(),
			b"\x1b[32;1;2M"
		);
		assert_eq!(
			report(
				MouseButtonCode::Right,
				MouseAction::Release,
				MouseEncoding::Urxvt
			)
			.unwrap(),
			b"\x1b[35;1;2M",
			"releases report button 3, as in X10"
		);
		assert_eq!(
			report(
				MouseButtonCode::None,
				MouseAction::Motion,
				MouseEncoding::Urxvt
			)
			.unwrap(),
			b"\x1b[67;1;2M"
		);
	}

	#[test]
	fn urxvt_reports_have_no_distance_limit() {
		let far = at(MouseButtonCode::WheelUp, MouseAction::Press, 5000, 3000);
		assert_eq!(
			encode_mouse(far, MouseEncoding::Urxvt, 1000).unwrap(),
			b"\x1b[96;5000;3000M"
		);
	}

	#[test]
	fn x10_compatibility_reports_no_modifiers() {
		let mods = Modifiers {
			shift: true,
			alt: true,
			ctrl: true,
			..Default::default()
		};
		let click = MouseReport {
			mods,
			..at(MouseButtonCode::Right, MouseAction::Press, 1, 2)
		};
		assert_eq!(
			encode_mouse(click, MouseEncoding::X10, 9).unwrap(),
			b"\x1b[M\x22\x21\x22"
		);
		assert_eq!(
			encode_mouse(click, MouseEncoding::Sgr, 9).unwrap(),
			b"\x1b[<2;1;2M"
		);
		assert_eq!(
			encode_mouse(click, MouseEncoding::X10, 1000).unwrap(),
			b"\x1b[M\x3e\x21\x22"
		);
	}

	#[test]
	fn each_tracking_mode_reports_its_own_events() {
		use MouseAction::{Motion, Press, Release};
		use MouseButtonCode::{Left, None, WheelUp};
		// Press, release, drag with a button held, hover with none held.
		let events = [
			(Left, Press),
			(WheelUp, Press),
			(Left, Release),
			(Left, Motion),
			(None, Motion),
		];
		let expected: [(u16, [bool; 5]); 6] = [
			(9, [true, true, false, false, false]),
			(1000, [true, true, true, false, false]),
			(1002, [true, true, true, true, false]),
			(1003, [true, true, true, true, true]),
			(0, [false; 5]),
			(1001, [false; 5]),
		];
		for (tracking, wanted) in expected {
			let got = events.map(|(button, kind)| tracked(tracking, button, kind));
			assert_eq!(got, wanted, "?{tracking}");
		}
	}

	#[test]
	fn encodings_name_their_modes_and_back() {
		for mode in [1005, 1006, 1015] {
			assert_eq!(
				MouseEncoding::from_mode(mode).and_then(MouseEncoding::mode),
				Some(mode)
			);
		}
		assert_eq!(MouseEncoding::X10.mode(), None);
		assert_eq!(MouseEncoding::default(), MouseEncoding::X10);
		for other in [9, 1000, 1004, 1016, 0] {
			assert_eq!(MouseEncoding::from_mode(other), None, "?{other}");
		}
	}
}
