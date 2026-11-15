//! Title modes: XTSMTITLE (`CSI > Pm t`) and XTRMTITLE (`CSI > Pm T`) choose
//! whether OSC 0, 1 and 2 titles arrive as hexadecimal and as UTF-8.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 4, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}

/// Sets the window title with OSC 2 and returns it.
fn title_from(term: &mut Term, payload: &[u8]) -> String {
	let mut bytes = b"\x1b]2;".to_vec();
	bytes.extend_from_slice(payload);
	bytes.extend_from_slice(b"\x1b\\");
	feed(term, &bytes);
	term.title.clone()
}

#[test]
fn titles_are_utf8_text_by_default() {
	let mut t = terminal();
	assert_eq!(title_from(&mut t, b"616263"), "616263");
	assert_eq!(title_from(&mut t, "café".as_bytes()), "café");
}

#[test]
fn hex_titles_are_decoded_once_set() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t");
	assert_eq!(title_from(&mut t, b"616263"), "abc");
	// Either case; the bytes are then UTF-8.
	assert_eq!(title_from(&mut t, b"636166C3A9"), "café");
	// OSC 1 and OSC 0 decode the icon name too.
	feed(&mut t, b"\x1b]1;69636f6e\x1b\\");
	assert_eq!(t.icon_name, "icon");
	feed(&mut t, b"\x1b]0;626f7468\x07");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("both", "both"));
}

#[test]
fn titles_that_are_not_hex_change_nothing_in_hex_mode() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t");
	assert_eq!(title_from(&mut t, b"6f6b"), "ok");
	for bad in [&b"616"[..], b"6g", b"title", b"61 62", b"+1"] {
		assert_eq!(
			title_from(&mut t, bad),
			"ok",
			"{:?}",
			String::from_utf8_lossy(bad)
		);
	}
	// An empty title is valid hex for nothing.
	assert_eq!(title_from(&mut t, b""), "");
}

#[test]
fn decoded_titles_are_cleaned_like_any_other() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t");
	// BEL and DEL are dropped, invalid UTF-8 becomes U+FFFD.
	assert_eq!(title_from(&mut t, b"6107627f63"), "abc");
	assert_eq!(title_from(&mut t, b"61ff62"), "a\u{fffd}b");
	// At most 64 characters.
	assert_eq!(
		title_from(&mut t, "78".repeat(100).as_bytes()),
		"x".repeat(64)
	);
}

#[test]
fn resetting_hex_mode_takes_titles_as_text_again() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t\x1b[>0T");
	assert_eq!(title_from(&mut t, b"616263"), "616263");
}

#[test]
fn without_utf8_titles_are_iso_8859_1() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>2T");
	// Each byte is one character: UTF-8 `é` reads as two.
	assert_eq!(title_from(&mut t, "é".as_bytes()), "Ã©");
	assert_eq!(title_from(&mut t, b"caf\xe9"), "café");
	// C1 controls are still dropped.
	assert_eq!(title_from(&mut t, b"a\x85b"), "ab");
	// Hex decodes to ISO-8859-1 bytes too.
	feed(&mut t, b"\x1b[>0t");
	assert_eq!(title_from(&mut t, b"636166e9"), "café");
	// And UTF-8 comes back with XTSMTITLE 2.
	feed(&mut t, b"\x1b[>0T\x1b[>2t");
	assert_eq!(title_from(&mut t, "é".as_bytes()), "é");
}

#[test]
fn several_features_change_at_once_and_unknown_ones_are_skipped() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>4;0;99;1;3t");
	assert_eq!(title_from(&mut t, b"6869"), "hi");
	feed(&mut t, b"\x1b[>7;0;2T");
	assert_eq!(title_from(&mut t, b"\xe9"), "é", "ISO-8859-1, not hex");
}

#[test]
fn resetting_without_parameters_restores_the_defaults() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0;1t\x1b[>2;3T");
	feed(&mut t, b"\x1b[>T");
	assert_eq!(title_from(&mut t, "é".as_bytes()), "é");
	assert_eq!(title_from(&mut t, b"6869"), "6869");
}

#[test]
fn a_lone_zero_or_missing_parameter_sets_hex() {
	// The tokenizer cannot tell `CSI > t` from `CSI > 0 t`; both mean 0.
	for set in [&b"\x1b[>t"[..], b"\x1b[>0t"] {
		let mut t = terminal();
		feed(&mut t, set);
		assert_eq!(title_from(&mut t, b"6869"), "hi", "{set:?}");
	}
}

#[test]
fn a_full_reset_restores_the_title_modes() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t\x1b[>2T\x1bc");
	assert_eq!(title_from(&mut t, "é".as_bytes()), "é");
	// A soft reset leaves them, as it leaves the title.
	feed(&mut t, b"\x1b[>0t\x1b[!p");
	assert_eq!(title_from(&mut t, b"6869"), "hi");
}

#[test]
fn query_features_never_report_a_title() {
	let mut t = terminal();
	feed(&mut t, b"\x1b]2;secret\x1b\\\x1b[>1;3t\x1b[21t\x1b[20t");
	assert!(t.responses.is_empty(), "{:?}", t.responses);
}

#[test]
fn title_mode_resets_are_not_scroll_down() {
	let mut t = terminal();
	feed(&mut t, b"top\x1b[>1T\x1b[>2;3T\x1b[>t");
	assert_eq!(t.grid[0][0].ch, 't');
	assert!(t.responses.is_empty());
}

#[test]
fn title_modes_with_subparameters_are_ignored() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0:1t");
	assert_eq!(title_from(&mut t, b"6869"), "6869");
}

#[test]
fn title_modes_and_hex_titles_split_across_reads() {
	let mut t = terminal();
	let mut parser = Parser::new();
	for chunk in [&b"\x1b[>"[..], b"0", b"t\x1b]2;61", b"62", b"\x1b\\"] {
		parser.advance(&mut t, chunk);
	}
	assert_eq!(t.title, "ab");
}

#[test]
fn the_title_stack_keeps_decoded_titles() {
	let mut t = terminal();
	feed(
		&mut t,
		b"\x1b[>0t\x1b]0;6f6e65\x07\x1b[22t\x1b]0;74776f\x07",
	);
	assert_eq!(t.title, "two");
	// Restoring does not decode again, whatever the modes are now.
	feed(&mut t, b"\x1b[>T\x1b[23t");
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("one", "one"));
}

#[test]
fn changing_the_modes_leaves_the_current_title() {
	let mut t = terminal();
	assert_eq!(title_from(&mut t, b"6869"), "6869");
	feed(&mut t, b"\x1b[>0t\x1b[>2T");
	assert_eq!(t.title, "6869");
}

#[test]
fn a_rejected_hex_title_does_not_redraw_the_tab() {
	let mut t = terminal();
	feed(&mut t, b"\x1b[>0t\x1b]0;6869\x07");
	let generation = t.title_generation;
	feed(&mut t, b"\x1b]0;not hex\x07\x1b]1;6\x07\x1b]2;zz\x07");
	assert_eq!(t.title_generation, generation);
	assert_eq!((t.title.as_str(), t.icon_name.as_str()), ("hi", "hi"));
}
