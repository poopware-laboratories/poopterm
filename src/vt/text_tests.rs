use super::{Chars, Text};

/// Code points at the edges of each encoded length and of the surrogates.
const EDGES: [char; 12] = [
	'\0',
	'\u{7f}',
	'\u{80}',
	'\u{7ff}',
	'\u{800}',
	'\u{d7ff}',
	'\u{e000}',
	'\u{fffd}',
	'\u{ffff}',
	'\u{10000}',
	'\u{fffff}',
	'\u{10ffff}',
];

#[test]
fn valid_bytes_make_text_and_malformed_ones_do_not() {
	assert_eq!(Text::new(b"").map(Text::as_bytes), Some(&b""[..]));
	assert_eq!(
		Text::new("a\u{e9}\u{732b}\u{1f600}".as_bytes()).map(Text::len),
		Some(10)
	);
	for malformed in [
		&b"\x80"[..],
		b"\xc0\x80",
		b"\xc2",
		b"\xe2\x82",
		b"\xed\xa0\x80",
		b"\xf4\x90\x80\x80",
		b"\xf8\x88\x80\x80\x80",
		b"ok\xff",
	] {
		assert_eq!(Text::new(malformed), None, "{malformed:x?}");
	}
}

#[test]
fn every_character_decodes_to_itself() {
	let mut buffer = [0; 4];
	for c in (0..=0x10ffff).filter_map(char::from_u32) {
		let text = Text::from(&*c.encode_utf8(&mut buffer));
		let mut chars = text.chars();
		assert_eq!(chars.next(), Some(c), "{:x}", u32::from(c));
		assert_eq!(chars.next(), None, "{:x}", u32::from(c));
	}
}

#[test]
fn decoding_matches_std_on_mixed_text() {
	let mixed: String = EDGES
		.iter()
		.chain(&['a', '\u{301}', '猫', '\u{200d}'])
		.collect();
	let ours: Vec<char> = Text::from(mixed.as_str()).chars().collect();
	assert_eq!(ours, mixed.chars().collect::<Vec<_>>());
}

#[test]
fn the_undecoded_bytes_follow_each_character() {
	let text = "a\u{e9}\u{732b}\u{1f600}";
	let mut chars = Text::from(text).chars();
	for (at, c) in text.char_indices() {
		assert_eq!(chars.as_bytes(), &text.as_bytes()[at..]);
		assert_eq!(chars.as_text(), Text::from(&text[at..]));
		assert_eq!(chars.next(), Some(c));
	}
	assert_eq!(chars.as_bytes(), b"");
	assert_eq!(chars.next(), None);
	assert_eq!(chars.next(), None, "stays empty");
}

#[test]
fn advancing_skips_whole_characters() {
	let mut chars = Text::from("ab\u{732b}c").chars();
	chars.advance(2);
	assert_eq!(chars.next(), Some('\u{732b}'));
	chars.advance(0);
	assert_eq!(chars.next(), Some('c'));
	chars.advance(0);
	assert_eq!(chars.next(), None);
}

#[test]
#[should_panic]
fn advancing_past_the_end_panics_like_slicing() {
	Text::from("ab").chars().advance(3);
}

#[test]
fn splitting_keeps_both_halves() {
	let text = Text::from("x\u{732b}y");
	assert_eq!(text.split_at(1), (Text::from("x"), Text::from("\u{732b}y")));
	assert_eq!(text.split_at(0), (Text::default(), text));
	assert_eq!(text.split_at(5), (text, Text::default()));
	assert!(text.split_at(5).1.is_empty());
	assert!(!text.is_empty());
}

#[test]
fn text_reads_back_as_a_str() {
	assert_eq!(Text::from("caf\u{e9}").to_str(), "caf\u{e9}");
	assert_eq!(Text::REPLACEMENT.to_str(), "\u{fffd}");
	assert_eq!(Text::REPLACEMENT.chars().collect::<String>(), "\u{fffd}");
	// Only reachable by breaking the invariant from inside the module.
	assert_eq!(Text(b"\xff").to_str(), "\u{fffd}");
}

#[test]
fn malformed_bytes_decode_to_replacements_without_panicking() {
	let decode = |bytes: &[u8]| Chars(bytes).collect::<String>();
	assert_eq!(decode(b"\x80"), "\u{fffd}");
	// A character cut off at the end is one replacement.
	assert_eq!(decode(b"\xe2\x82"), "\u{fffd}");
	assert_eq!(decode(b"\xc3"), "\u{fffd}");
	assert_eq!(decode(b"\xf0\x9f\x98"), "\u{fffd}");
	assert_eq!(decode(b"\xf0\x9f\x98\x80"), "\u{1f600}");
	// A surrogate and a code point above U+10FFFF fail the final check.
	assert_eq!(decode(b"\xed\xa0\x80"), "\u{fffd}");
	assert_eq!(decode(b"\xf7\xbf\xbf\xbf"), "\u{fffd}");
	for first in 0..=u8::MAX {
		for second in 0..=u8::MAX {
			// One character per byte at most, and every byte consumed.
			let count = Chars(&[first, second, b'z']).count();
			assert!((1..=3).contains(&count), "{first:x} {second:x}");
		}
	}
}

#[test]
fn the_size_hint_bounds_the_count() {
	for text in [
		"",
		"a",
		"abcd",
		"\u{1f600}",
		"\u{1f600}a",
		"\u{e9}\u{e9}\u{e9}",
	] {
		let chars = Text::from(text).chars();
		let (low, high) = chars.size_hint();
		let count = chars.count();
		assert!(low <= count, "{text}");
		assert!(high.is_some_and(|high| count <= high), "{text}");
	}
}
