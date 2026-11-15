//! Title modes, set by XTSMTITLE (`CSI > Pm t`) and reset by XTRMTITLE
//! (`CSI > Pm T`): feature 0 takes OSC 0, 1 and 2 titles as hexadecimal,
//! 2 takes their bytes as UTF-8 rather than ISO-8859-1. Features 1 and 3
//! choose how a title query is answered; titles are never reported, so
//! they are only kept.
use super::{osc::clean_title, params::ParamList};
use crate::term::Term;

const HEX: u8 = 1 << 0;
const UTF8: u8 = 1 << 2;
/// The initial modes: UTF-8 set and query, as titles always were here.
pub(crate) const DEFAULT_TITLE_MODES: u8 = UTF8 | 1 << 3;

impl Term {
	/// XTSMTITLE (`set`) or XTRMTITLE: turns the listed features on or
	/// off; a reset without parameters restores the defaults.
	pub(super) fn set_title_modes(&mut self, params: &(impl ParamList + ?Sized), set: bool) {
		if params.iter().any(|param| param.len() > 1) {
			return;
		}
		if !set && params.is_default() {
			self.title_modes = DEFAULT_TITLE_MODES;
			return;
		}
		for feature in params
			.iter()
			.map(|param| param.first().copied().unwrap_or(0))
		{
			if feature <= 3 {
				let bit = 1 << feature;
				if set {
					self.title_modes |= bit;
				} else {
					self.title_modes &= !bit;
				}
			}
		}
	}

	/// An OSC title as the title modes read it, or `None` when hex mode is
	/// on and the text is not hexadecimal.
	pub(super) fn decode_title(&self, text: &[u8]) -> Option<String> {
		let hex;
		let bytes = if self.title_modes & HEX != 0 {
			hex = from_hex(text)?;
			&hex[..]
		} else {
			text
		};
		Some(if self.title_modes & UTF8 != 0 {
			clean_title(bytes)
		} else {
			let latin1: String = bytes.iter().map(|&byte| char::from(byte)).collect();
			clean_title(latin1.as_bytes())
		})
	}
}

/// Pairs of hexadecimal digits, in either case, as bytes.
fn from_hex(text: &[u8]) -> Option<Vec<u8>> {
	let (pairs, odd) = text.as_chunks::<2>();
	if !odd.is_empty() {
		return None;
	}
	let digit = |byte: u8| char::from(byte).to_digit(16);
	(pairs.iter())
		.map(|&[high, low]| u8::try_from(digit(high)? << 4 | digit(low)?).ok())
		.collect()
}

#[cfg(test)]
mod tests {
	use super::{DEFAULT_TITLE_MODES, from_hex};
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};

	#[test]
	fn every_byte_decodes_from_either_case() {
		for byte in 0..=255u8 {
			for text in [format!("{byte:02x}"), format!("{byte:02X}")] {
				assert_eq!(from_hex(text.as_bytes()), Some(vec![byte]), "{text}");
			}
		}
		assert_eq!(from_hex(b""), Some(Vec::new()));
		assert_eq!(from_hex(b"00ff7f"), Some(vec![0, 255, 127]));
	}

	#[test]
	fn anything_but_pairs_of_hex_digits_is_rejected() {
		let bad: [&[u8]; 9] = [
			b"0",
			b"abc",
			b"0g",
			b"g0",
			b"0x41",
			b"-1",
			b"4 1",
			b"41\n",
			"é9".as_bytes(),
		];
		for text in bad {
			assert_eq!(from_hex(text), None, "{:?}", String::from_utf8_lossy(text));
		}
	}

	#[test]
	fn each_feature_turns_on_and_off_alone() {
		let mut t = Term::new(10, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
		assert_eq!(t.title_modes, 0b1100, "UTF-8 set and query");
		for feature in 0..4u8 {
			Parser::new().advance(&mut t, format!("\x1b[>{feature}t").as_bytes());
			assert_eq!(
				t.title_modes,
				DEFAULT_TITLE_MODES | 1 << feature,
				"set {feature}"
			);
			Parser::new().advance(&mut t, format!("\x1b[>{feature}T").as_bytes());
			// A lone 0 cannot be told from no parameters: all defaults.
			let off = if feature == 0 {
				DEFAULT_TITLE_MODES
			} else {
				DEFAULT_TITLE_MODES & !(1 << feature)
			};
			assert_eq!(t.title_modes, off, "reset {feature}");
			Parser::new().advance(&mut t, b"\x1b[>T");
		}
		// Features past 3 do not exist and leave the bits alone.
		Parser::new().advance(&mut t, b"\x1b[>4;7;8;65535t\x1b[>4;65535T");
		assert_eq!(t.title_modes, DEFAULT_TITLE_MODES);
	}
}
