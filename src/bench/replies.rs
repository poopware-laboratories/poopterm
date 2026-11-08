//! Reading a terminal's answers to the queries `--terminal` sends: the
//! Primary Device Attributes reply that marks all earlier output as parsed,
//! and the XTVERSION reply that names the terminal.

/// The longest terminal name taken from an XTVERSION reply.
const NAME_LIMIT: usize = 256;

/// Where the first complete Primary Device Attributes reply in `input`
/// (`CSI ? Ps ; … c`) is. Other replies and key presses before it are
/// skipped; the secondary reply (`CSI > … c`) does not count.
pub(crate) fn da1(input: &[u8]) -> Option<std::ops::Range<usize>> {
	let mut at = 0;
	while let Some(found) = find(&input[at..], b"\x1b[?") {
		let start = at + found + 3;
		let params = input[start..]
			.iter()
			.take_while(|byte| matches!(byte, b'0'..=b'9' | b';'));
		let end = start + params.count();
		if input.get(end) == Some(&b'c') {
			return Some(start - 3..end + 1);
		}
		at = start;
	}
	None
}

/// The name in the first XTVERSION reply (`DCS > | name ST`) in `input`,
/// if it is complete, not empty and at most [`NAME_LIMIT`] bytes.
pub(crate) fn xtversion(input: &[u8]) -> Option<String> {
	// Only the first opener: searching again from each one would be
	// quadratic on a reply full of openers and no terminator.
	let start = find(input, b"\x1bP>|")? + 4;
	let name = &input[start..][..find(&input[start..], b"\x1b\\")?];
	(1..=NAME_LIMIT)
		.contains(&name.len())
		.then(|| String::from_utf8_lossy(name).into_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
	haystack
		.windows(needle.len())
		.position(|window| window == needle)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_primary_reply_ends_after_its_final_c() {
		assert_eq!(da1(b"\x1b[?62;4;6c"), Some(0..10));
		assert_eq!(da1(b"\x1b[?62;4;6cmore"), Some(0..10));
		assert_eq!(da1(b"\x1b[?1c"), Some(0..5));
		// Parameters may be left out, as everywhere in a CSI sequence.
		assert_eq!(da1(b"\x1b[?c"), Some(0..4));
	}

	#[test]
	fn what_comes_before_the_reply_is_skipped() {
		let input = b"typed\x1bP>|kitty(0.36)\x1b\\\x1b[?62;22c";
		assert_eq!(da1(input), Some(input.len() - 9..input.len()));
	}

	#[test]
	fn an_unfinished_reply_is_not_one_yet() {
		assert_eq!(da1(b""), None);
		assert_eq!(da1(b"\x1b"), None);
		assert_eq!(da1(b"\x1b[?62;4"), None);
	}

	#[test]
	fn other_private_replies_are_not_the_primary_one() {
		// A mode report ends in `y`, the secondary attributes start `>`.
		assert_eq!(da1(b"\x1b[?1;2$y"), None);
		assert_eq!(da1(b"\x1b[>0;10;1c"), None);
		let input = b"\x1b[?1;2$y\x1b[>1c\x1b[?6c";
		assert_eq!(da1(input), Some(input.len() - 5..input.len()));
	}

	#[test]
	fn a_c_inside_some_other_reply_is_not_the_end() {
		assert_eq!(da1(b"\x1bP>|vc 1.0\x1b\\"), None);
		assert_eq!(da1(b"\x1b[?6;c"), Some(0..6));
		assert_eq!(da1(b"\x1b[?6xc"), None);
	}

	#[test]
	fn the_name_sits_between_the_opener_and_the_terminator() {
		assert_eq!(
			xtversion(b"\x1bP>|kitty(0.36)\x1b\\").as_deref(),
			Some("kitty(0.36)")
		);
		let input = b"\x1b[?62c\x1bP>|poopterm 26.11.36\x1b\\\x1b[?62c";
		assert_eq!(xtversion(input).as_deref(), Some("poopterm 26.11.36"));
	}

	#[test]
	fn no_reply_or_an_unfinished_one_has_no_name() {
		assert_eq!(xtversion(b"\x1b[?62;4c"), None);
		assert_eq!(xtversion(b"\x1bP>|kitty"), None);
		assert_eq!(xtversion(b"\x1bP>kitty\x1b\\"), None);
	}

	#[test]
	fn a_flood_of_openers_without_a_terminator_has_no_name() {
		let input = b"\x1bP>|".repeat(100_000);
		assert_eq!(xtversion(&input), None);
	}

	#[test]
	fn names_are_bounded_and_need_not_be_utf8() {
		let longest = [b"\x1bP>|", &[b'a'; NAME_LIMIT][..], b"\x1b\\"].concat();
		assert_eq!(xtversion(&longest).map(|name| name.len()), Some(NAME_LIMIT));
		let longer = [b"\x1bP>|", &[b'a'; NAME_LIMIT + 1][..], b"\x1b\\"].concat();
		assert_eq!(xtversion(&longer), None);
		assert_eq!(
			xtversion(b"\x1bP>|t\xffx\x1b\\").as_deref(),
			Some("t\u{fffd}x")
		);
	}

	#[test]
	fn an_opener_cut_short_by_another_still_finds_the_reply_inside() {
		assert_eq!(da1(b"\x1b[?\x1b[?6c"), Some(3..8));
		assert_eq!(da1(b"\x1b[?62\x1b[?6c"), Some(5..10));
	}

	#[test]
	fn a_vt525_reply_with_many_parameters_is_whole() {
		let reply = b"\x1b[?65;1;2;6;9;15;16;17;18;21;22;28;29c";
		assert_eq!(da1(reply), Some(0..reply.len()));
	}

	#[test]
	fn a_flood_of_unfinished_replies_has_none() {
		let input = b"\x1b[?62;".repeat(200_000);
		assert_eq!(da1(&input), None);
		let mut finished = input;
		finished.extend_from_slice(b"1c");
		assert_eq!(da1(&finished).map(|reply| reply.end), Some(finished.len()));
	}

	#[test]
	fn an_empty_name_is_no_name() {
		assert_eq!(xtversion(b"\x1bP>|\x1b\\"), None);
		assert_eq!(xtversion(b"\x1b\\\x1bP>|x\x1b\\").as_deref(), Some("x"));
	}
}
