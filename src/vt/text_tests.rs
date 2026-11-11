use super::{Chars, Text};
use crate::{
	bench::corpus::Rng,
	vt::{Handler, Params, Parser},
};

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
fn every_character_decodes_to_itself_before_any_tail() {
	// Up to three bytes after a character pick each of the decoder's length
	// checks: the end of the text, and room for a whole four-byte character.
	for tail in ["", "z", "zy", "zyx"] {
		for c in (0..=0x10ffff).filter_map(char::from_u32) {
			let text = format!("{c}{tail}");
			let mut chars = Text::from(text.as_str()).chars();
			assert_eq!(chars.next(), Some(c), "{:x} before {tail:?}", u32::from(c));
			assert_eq!(chars.as_bytes(), tail.as_bytes(), "{:x}", u32::from(c));
			assert!(chars.eq(tail.chars()), "{:x} before {tail:?}", u32::from(c));
		}
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

/// Random text over every encoded length, the edges, and marks.
fn random_text(rng: &mut Rng) -> String {
	(0..rng.below(40))
		.map(|_| match rng.below(4) {
			0 => *rng.pick(&EDGES),
			1 => char::from_u32(rng.below(0x110000) as u32).unwrap_or('\u{fffd}'),
			2 => char::from(b' ' + rng.below(95) as u8),
			_ => *rng.pick(&['\u{301}', '\u{200d}', '猫', '\u{1f600}', '\u{e9}']),
		})
		.collect()
}

#[test]
fn random_text_decodes_like_std_with_the_same_bytes_left() {
	let mut rng = Rng::new(0x7e47);
	for _ in 0..2000 {
		let text = random_text(&mut rng);
		let mut chars = Text::from(text.as_str()).chars();
		for (at, c) in text.char_indices() {
			assert_eq!(chars.as_bytes().len(), text.len() - at, "{text:?}");
			assert_eq!(chars.next(), Some(c), "{text:?} at {at}");
		}
		assert_eq!(chars.next(), None, "{text:?}");
	}
}

#[test]
fn every_cut_of_random_text_is_text_exactly_when_std_accepts_it() {
	let mut rng = Rng::new(0xc07);
	for _ in 0..300 {
		let text = random_text(&mut rng);
		let bytes = text.as_bytes();
		for at in 0..=bytes.len() {
			for part in [&bytes[..at], &bytes[at..]] {
				let std = std::str::from_utf8(part).ok();
				assert_eq!(Text::new(part), std.map(Text::from), "{part:x?}");
			}
		}
	}
}

#[test]
fn a_clone_peeks_without_moving_the_original() {
	let mut chars = Text::from("\u{732b}a").chars();
	let mut peek = chars.clone();
	assert_eq!(peek.next(), Some('\u{732b}'));
	assert_eq!(peek.as_bytes(), b"a");
	assert_eq!(chars.as_bytes(), "\u{732b}a".as_bytes());
	assert_eq!(chars.next(), Some('\u{732b}'));
	assert_eq!(chars.as_text(), peek.as_text());
}

#[test]
fn advancing_past_a_chunk_resumes_where_decoding_all_would() {
	// The narrow fast path consumes whole ASCII chunks, then decodes on.
	let text = "abcdefgh\u{732b}\u{301}ij";
	let mut chars = Text::from(text).chars();
	chars.advance(8);
	assert!(chars.eq(text.chars().skip(8)));
}

/// Collects printed text; ASCII takes the default `print_ascii`.
#[derive(Default)]
struct Printed(String);

impl Handler for Printed {
	fn print(&mut self, text: Text<'_>) {
		self.0.extend(text.chars());
	}
	fn execute(&mut self, byte: u8) {
		self.0.push(char::from(byte));
	}
	fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
	fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
	fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn put(&mut self, _: &[u8]) {}
	fn unhook(&mut self) {}
}

#[test]
fn handlers_without_an_ascii_path_get_ascii_as_text() {
	let mut printed = Printed::default();
	Parser::new().advance(&mut printed, "hi\x1b[1m th\u{e9}re\r".as_bytes());
	assert_eq!(printed.0, "hi th\u{e9}re\r");
}

#[test]
fn malformed_runs_print_replacements_and_c1_controls_execute() {
	let mut printed = Printed::default();
	Parser::new().advance(&mut printed, b"a\xffb\xc2\x85c\xe2\x82");
	// The cut-off character at the end waits for the next read.
	assert_eq!(printed.0, "a\u{fffd}b\u{85}c");
}
