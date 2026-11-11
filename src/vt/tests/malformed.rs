//! Malformed UTF-8 in text: every kind of defect, in every context the text
//! loop meets it (alone, between characters, before controls and escapes, at
//! the end of a read), must print what vte prints.
use super::{check, check_with, ours, printed_and_executed};

/// One of each way a byte sequence fails to be a character.
const DEFECTS: &[&[u8]] = &[
	b"\x80",             // a lone continuation byte
	b"\xbf",             // the last continuation byte
	b"\xc0\xaf",         // an overlong two-byte slash
	b"\xc1\xbf",         // the other overlong two-byte lead
	b"\xe0\x80\xaf",     // an overlong three-byte form
	b"\xed\xa0\x80",     // a surrogate
	b"\xf0\x80\x80\xaf", // an overlong four-byte form
	b"\xf4\x90\x80\x80", // above U+10FFFF
	b"\xf5\x80",         // a lead byte no character uses
	b"\xff",
	b"\xc3",                 // two-byte character missing its end
	b"\xe6\x97",             // three-byte character missing its end
	b"\xf0\x9f\x98",         // four-byte character missing its end
	b"\xe6\x41",             // a lead interrupted by ASCII
	b"\xf0\x9f\xe6\x97\xa5", // a lead interrupted by a whole character
];

/// What may surround a defect.
const CONTEXTS: &[(&[u8], &[u8])] = &[
	(b"", b""),
	(b"a", b"b"),
	("日".as_bytes(), "本".as_bytes()),
	(b"x", b"\x1b[1m"),
	(b"x", b"\x07"),
	(b"x", b"\x1b]0;t\x07"),
	("é".as_bytes(), b"\xc2\x85"),
];

#[test]
fn every_defect_in_every_context_matches_vte_read_whole() {
	for defect in DEFECTS {
		for (before, after) in CONTEXTS {
			check(&[before, *defect, after].concat(), &[]);
		}
	}
}

#[test]
fn every_defect_in_every_context_matches_vte_read_byte_by_byte() {
	for defect in DEFECTS {
		for (before, after) in CONTEXTS {
			check_with(&[before, *defect, after].concat(), &[], true);
		}
	}
}

#[test]
fn two_defects_in_a_row_match_vte_at_every_split() {
	for first in DEFECTS {
		for second in DEFECTS {
			let stream = [b"a", *first, *second, b"z"].concat();
			for split in 0..=stream.len() {
				check(&stream, &[split]);
			}
		}
	}
}

#[test]
fn a_character_split_across_reads_prints_once_whole() {
	for ch in ['é', '日', '😀', '\u{10ffff}'] {
		let stream = format!("a{ch}b");
		for split in 1..stream.len() {
			let (text, controls) = printed_and_executed(&ours(stream.as_bytes(), &[split]));
			assert_eq!(text, stream, "{ch:?} split at {split}");
			assert!(controls.is_empty());
		}
	}
}

#[test]
fn a_character_cut_off_before_an_escape_prints_one_replacement() {
	// The read ends in the middle of the run, so the cut is not carried over.
	for cut in [&b"\xc3"[..], b"\xe6\x97", b"\xf0\x9f\x98"] {
		let stream = [b"a", cut, b"\x07b"].concat();
		check(&stream, &[]);
		let (text, controls) = printed_and_executed(&ours(&stream, &[]));
		assert_eq!(text, "a\u{fffd}b", "{cut:x?}");
		assert_eq!(controls, [0x07]);
	}
}

#[test]
fn a_cut_character_whose_next_read_is_not_its_end_prints_one_replacement() {
	for cut in [&b"\xc3"[..], b"\xe6\x97", b"\xf0\x9f\x98"] {
		for next in [&b"b"[..], b"\x07", "日".as_bytes(), b"\xff"] {
			let stream = [b"a", cut, next].concat();
			check(&stream, &[1 + cut.len()]);
			let (text, _) = printed_and_executed(&ours(&stream, &[1 + cut.len()]));
			assert!(
				text.starts_with("a\u{fffd}"),
				"{cut:x?} then {next:x?}: {text:?}"
			);
		}
	}
}

#[test]
fn a_raw_c1_byte_executes_but_one_continuing_a_character_does_not() {
	for control in 0x80..=0x9fu8 {
		let stream = [b'a', control, b'b'];
		check(&stream, &[]);
		let (text, controls) = printed_and_executed(&ours(&stream, &[]));
		assert_eq!((text.as_str(), controls.as_slice()), ("ab", &[control][..]));
		// After a three-byte lead the same byte continues the character,
		// which `b` then cuts off: one replacement, no control.
		let stream = [b'a', 0xe6, control, b'b'];
		check(&stream, &[]);
		let (text, controls) = printed_and_executed(&ours(&stream, &[]));
		assert_eq!(
			(text.as_str(), controls.as_slice()),
			("a\u{fffd}b", &[][..])
		);
	}
	// Encoded as a character, the control executes too.
	let stream = b"a\xc2\x85b";
	check(stream, &[]);
	assert_eq!(
		printed_and_executed(&ours(stream, &[])),
		("ab".to_owned(), vec![0x85])
	);
}
