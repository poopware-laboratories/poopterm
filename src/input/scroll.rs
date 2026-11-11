//! Mouse-wheel and touchpad scrolling.

/// Turns wheel and touchpad deltas into whole lines, carrying the fraction
/// so slow touchpad gestures still scroll. Positive values scroll up.
#[derive(Default)]
pub(crate) struct ScrollAccumulator {
	remainder: f64,
}

impl ScrollAccumulator {
	/// Adds a pixel delta, given the height of one line in pixels.
	pub(crate) fn pixels(&mut self, delta: f64, line_height: f64) -> isize {
		self.lines(delta / line_height.max(1.0))
	}

	/// Adds a delta measured in lines.
	pub(crate) fn lines(&mut self, delta: f64) -> isize {
		if delta.signum() != self.remainder.signum() {
			self.remainder = 0.0;
		}
		self.remainder += delta;
		let whole = self.remainder.trunc();
		self.remainder -= whole;
		whole as isize
	}
}

/// What a turn of the wheel does in the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelAction {
	/// Already sent to the program as mouse reports.
	Report,
	/// Cursor keys for the program on the alternate screen.
	Keys,
	/// Scrolls the history in view.
	ScrollView,
	/// Nothing: the alternate screen has no history, and the program
	/// turned alternate scroll (`?1007`) off.
	Nothing,
}

/// Chooses what the wheel does: reports when the program tracks the mouse
/// over its grid, cursor keys on the alternate screen when alternate scroll
/// is on, and otherwise scrolling the history in view.
pub(crate) fn wheel_action(
	tracking: bool,
	over_grid: bool,
	alternate_screen: bool,
	alternate_scroll: bool,
) -> WheelAction {
	match (tracking && over_grid, alternate_screen, alternate_scroll) {
		(true, _, _) => WheelAction::Report,
		(false, true, true) => WheelAction::Keys,
		(false, true, false) => WheelAction::Nothing,
		(false, false, _) => WheelAction::ScrollView,
	}
}

/// Cursor-key presses that emulate scrolling in alternate-screen programs
/// without mouse tracking (xterm's `alternateScroll`), e.g. `less` or `man`.
pub(crate) fn alternate_scroll_bytes(lines: isize, application_cursor: bool) -> Vec<u8> {
	let letter = if lines > 0 { b'A' } else { b'B' };
	let key: [u8; 3] = if application_cursor {
		[0x1b, b'O', letter]
	} else {
		[0x1b, b'[', letter]
	};
	key.repeat(lines.unsigned_abs())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn small_touchpad_deltas_accumulate_into_lines() {
		let mut scroll = ScrollAccumulator::default();
		assert_eq!(scroll.pixels(10.0, 24.0), 0);
		assert_eq!(scroll.pixels(10.0, 24.0), 0);
		assert_eq!(scroll.pixels(10.0, 24.0), 1);
		assert_eq!(scroll.pixels(-60.0, 24.0), -2);
	}

	#[test]
	fn whole_line_deltas_pass_through() {
		let mut scroll = ScrollAccumulator::default();
		assert_eq!(scroll.lines(3.0), 3);
		assert_eq!(scroll.lines(-1.0), -1);
	}

	#[test]
	fn a_tracking_program_gets_the_wheel_over_its_grid() {
		for alternate in [false, true] {
			for keys in [false, true] {
				assert_eq!(
					wheel_action(true, true, alternate, keys),
					WheelAction::Report
				);
			}
		}
	}

	#[test]
	fn the_alternate_screen_gets_cursor_keys_only_with_alternate_scroll() {
		for (tracking, over_grid) in [(false, true), (false, false), (true, false)] {
			assert_eq!(
				wheel_action(tracking, over_grid, true, true),
				WheelAction::Keys
			);
			assert_eq!(
				wheel_action(tracking, over_grid, true, false),
				WheelAction::Nothing
			);
		}
	}

	#[test]
	fn the_main_screen_scrolls_its_history_whatever_alternate_scroll_says() {
		for (tracking, over_grid) in [(false, true), (false, false), (true, false)] {
			for keys in [false, true] {
				assert_eq!(
					wheel_action(tracking, over_grid, false, keys),
					WheelAction::ScrollView
				);
			}
		}
	}

	#[test]
	fn alternate_screen_scrolling_sends_cursor_keys() {
		assert_eq!(alternate_scroll_bytes(2, false), b"\x1b[A\x1b[A");
		assert_eq!(alternate_scroll_bytes(-1, true), b"\x1bOB");
		assert!(alternate_scroll_bytes(0, false).is_empty());
	}
}
