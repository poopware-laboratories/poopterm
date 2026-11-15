//! ANSI modes (SM, RM), DECRQM answers, and saving DEC private modes with
//! XTSAVE / XTRESTORE.
use crate::term::Term;

/// DEC private modes kept only so programs read back what they set: DECSCLM
/// (smooth scrolling, ?4), DECPFF and DECPEX (printer form feed and extent,
/// ?18, ?19), DECHEBM (Hebrew keyboard, ?35) and DECNRCM (national
/// replacement characters, ?42). All start reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredModes(u8);

const STORED: [u16; 5] = [4, 18, 19, 35, 42];

impl StoredModes {
	fn bit(mode: u16) -> Option<u8> {
		STORED.iter().position(|&m| m == mode).map(|i| 1 << i)
	}

	pub(crate) fn get(self, mode: u16) -> Option<bool> {
		Self::bit(mode).map(|bit| self.0 & bit != 0)
	}

	/// Sets or resets `mode`; false if it is not a stored mode.
	pub(crate) fn set(&mut self, mode: u16, enabled: bool) -> bool {
		let Some(bit) = Self::bit(mode) else {
			return false;
		};
		self.0 = if enabled { self.0 | bit } else { self.0 & !bit };
		true
	}
}

/// ANSI modes with no effect here, which DECRQM reports as permanently
/// reset: GATM, SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM, EBM.
const PERMANENT_ANSI: [u16; 12] = [1, 5, 7, 10, 11, 13, 14, 15, 16, 17, 18, 19];

/// DEC private modes DECRQM reports as permanently reset: DECHCCM (?60),
/// which only matters with horizontal scrolling.
const PERMANENT_DEC: [u16; 1] = [60];

/// A DECRQM status: 0 unrecognized, 1 set, 2 reset, 4 permanently reset.
pub(super) fn mode_status(state: Option<bool>, permanent: bool) -> u8 {
	match state {
		_ if permanent => 4,
		Some(true) => 1,
		Some(false) => 2,
		None => 0,
	}
}

/// Whether DECRQM reports `mode` as permanently reset.
pub(super) fn permanently_reset(mode: u16, private: bool) -> bool {
	if private {
		PERMANENT_DEC.contains(&mode)
	} else {
		PERMANENT_ANSI.contains(&mode)
	}
}

impl Term {
	pub(super) fn set_ansi_mode(&mut self, mode: u16, enabled: bool) {
		match mode {
			2 => self.keyboard_locked = enabled,
			4 => self.insert_mode = enabled,
			// SRM set means no local echo.
			12 => self.local_echo = !enabled,
			20 => self.newline_mode = enabled,
			_ => {}
		}
	}

	/// The state of an ANSI mode for DECRQM, or `None` if unsupported.
	pub(crate) fn ansi_mode(&self, mode: u16) -> Option<bool> {
		Some(match mode {
			2 => self.keyboard_locked,
			4 => self.insert_mode,
			12 => !self.local_echo,
			20 => self.newline_mode,
			_ => return None,
		})
	}

	/// XTSAVE: remembers the current value of each known private mode.
	pub(super) fn save_private_modes(&mut self, modes: impl Iterator<Item = u16>) {
		for mode in modes {
			let Some(value) = self.private_mode(mode) else {
				continue;
			};
			match self
				.saved_modes
				.iter_mut()
				.find(|(saved, _)| *saved == mode)
			{
				Some(entry) => entry.1 = value,
				None => self.saved_modes.push((mode, value)),
			}
		}
	}

	/// XTRESTORE: sets each saved mode back to its saved value. Resets go
	/// first, so a reset mouse mode cannot turn off one restored as set.
	pub(super) fn restore_private_modes(&mut self, modes: impl Iterator<Item = u16>) {
		let mut restored: Vec<(u16, bool)> = modes
			.filter_map(|mode| self.saved_modes.iter().find(|(saved, _)| *saved == mode))
			.copied()
			.collect();
		restored.sort_by_key(|&(_, set)| set);
		for (mode, set) in restored {
			self.apply_csi_dispatch(&[mode][..], b"?", false, if set { 'h' } else { 'l' });
		}
	}
}
