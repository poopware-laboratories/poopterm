//! DECDSR (`CSI ? Ps n`) reports about hardware Poopterm does not have,
//! answered as xterm answers them for a terminal without it.

/// The reply to `CSI ? code ; id n`, or `None` for a code left unanswered.
/// The cursor position (6) is answered by the caller.
pub(super) fn dec_status(code: u16, id: u16) -> Option<String> {
	Some(match code {
		// No printer.
		15 => "\x1b[?13n".into(),
		// User-defined keys are locked: DECUDK is not supported.
		25 => "\x1b[?21n".into(),
		// No DEC locator, so its type is unknown.
		53 | 55 => "\x1b[?50n".into(),
		56 => "\x1b[?57;0n".into(),
		// DECMSR: no space for macros.
		62 => "\x1b[0*{".into(),
		// DECCKSR: the checksum of the empty macro memory, tagged with `id`.
		63 => format!("\x1bP{id}!~0000\x1b\\"),
		// No communication errors.
		75 => "\x1b[?70n".into(),
		// Not set up for multiple sessions.
		85 => "\x1b[?83n".into(),
		_ => return None,
	})
}
