use super::*;

#[test]
fn a_return_from_the_keyboard_is_a_carriage_return() {
	assert_eq!(commit_bytes("\n"), b"\r");
}

#[test]
fn plain_text_is_unchanged_including_non_ascii() {
	assert_eq!(commit_bytes("hello"), b"hello");
	assert_eq!(commit_bytes("é漢😀"), "é漢😀".as_bytes());
	assert_eq!(commit_bytes(""), b"");
}

#[test]
fn text_with_line_breaks_keeps_each_one_as_a_single_carriage_return() {
	assert_eq!(commit_bytes("a\nb\n"), b"a\rb\r");
	assert_eq!(commit_bytes("a\r\nb"), b"a\rb", "a CR LF pair is one break");
	assert_eq!(commit_bytes("\n\n"), b"\r\r", "empty lines stay");
	assert_eq!(commit_bytes("a\rb"), b"a\rb", "a lone CR is already right");
}

#[test]
fn a_carriage_return_before_a_line_feed_only_collapses_with_that_one() {
	assert_eq!(commit_bytes("\r\r\n"), b"\r\r");
	assert_eq!(commit_bytes("\r\n\n"), b"\r\r");
}
