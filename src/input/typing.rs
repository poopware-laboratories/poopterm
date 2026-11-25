//! What the on-screen keyboard may do to text typed into the terminal:
//! nothing. Smart quotes and dashes would turn a shell's `'` and `--` into
//! `‘’` and `—`, and correction or capitals would rewrite commands, so the
//! view answers each `UITextInputTraits` question with "no" (iOS).
use std::ffi::CStr;

/// `UITextAutocapitalizationTypeNone`.
pub(crate) const NONE: isize = 0;
/// The `…TypeNo` value of every other trait below.
pub(crate) const NO: isize = 1;

/// Each trait's getter and the value that turns it off, as UIKit's
/// `UITextInputTraits` documents them.
pub(crate) const TRAITS: [(&CStr, isize); 7] = [
	(c"autocapitalizationType", NONE),
	(c"autocorrectionType", NO),
	(c"spellCheckingType", NO),
	(c"smartQuotesType", NO),
	(c"smartDashesType", NO),
	(c"smartInsertDeleteType", NO),
	(c"inlinePredictionType", NO),
];

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn quotes_and_dashes_stay_as_typed() {
		for getter in [c"smartQuotesType", c"smartDashesType"] {
			assert!(TRAITS.contains(&(getter, NO)), "{getter:?}");
		}
	}

	#[test]
	fn commands_are_neither_corrected_nor_capitalized() {
		assert!(TRAITS.contains(&(c"autocorrectionType", NO)));
		assert!(TRAITS.contains(&(c"autocapitalizationType", NONE)));
		assert!(TRAITS.contains(&(c"inlinePredictionType", NO)));
	}

	#[test]
	fn every_value_is_one_the_glue_has_an_answer_for() {
		// ios_keyboard has one method returning NONE and one returning NO.
		assert!(
			TRAITS
				.iter()
				.all(|&(_, value)| value == NONE || value == NO)
		);
		assert_ne!(NONE, NO);
	}

	#[test]
	fn each_getter_is_listed_once_and_is_a_plain_getter() {
		for (index, (getter, _)) in TRAITS.iter().enumerate() {
			let name = getter.to_str().unwrap();
			assert!(name.ends_with("Type"), "{name}");
			assert!(!name.contains(':'), "takes no arguments: {name}");
			assert!(
				TRAITS[index + 1..].iter().all(|(other, _)| other != getter),
				"{name} twice"
			);
		}
	}
}
