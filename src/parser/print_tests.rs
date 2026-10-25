//! Tests of printing whole segments: clusters, marks and joins built inside
//! a segment must match printing one character at a time.
use crate::{
	parser::{Parser, PerCharacter},
	term::{Cell, NO_EXTRA, Term},
	theme::{Theme, ThemeKind},
};

/// A cell with its marks and hyperlink resolved, so terminals whose extras
/// tables differ in order still compare equal when they show the same thing.
fn resolved(term: &Term, cell: Cell) -> (Cell, Vec<char>, u16) {
	let (marks, link) = (
		term.extras.marks(cell.extra).to_vec(),
		term.extras.hyperlink(cell.extra),
	);
	(Cell { extra: 0, ..cell }, marks, link)
}

fn screen(term: &Term) -> Vec<Vec<(Cell, Vec<char>, u16)>> {
	let history = term.scrollback.iter().map(<[Cell]>::to_vec);
	let rows = history.chain(term.grid.iter().map(|row| row.to_vec()));
	rows
		.map(|row| row.into_iter().map(|cell| resolved(term, cell)).collect())
		.collect()
}

/// Feeds `stream` whole and through vte one character at a time, at every
/// width from 1 to `widest` columns, and asserts the terminals agree.
fn assert_matches_per_character(stream: &str, widest: usize) {
	for cols in 1..=widest {
		let mut batched = Term::new(cols, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
		batched.scrollback_limit = 40;
		let mut reference = batched.clone();
		Parser::new().advance(&mut batched, stream.as_bytes());
		vte::Parser::new().advance(&mut PerCharacter(&mut reference), stream.as_bytes());
		let cursor = |t: &Term| (t.cx, t.cy, t.wrap_pending, t.last_printed);
		assert_eq!(
			cursor(&batched),
			cursor(&reference),
			"{cols} columns: {stream:?}"
		);
		assert_eq!(
			screen(&batched),
			screen(&reference),
			"{cols} columns: {stream:?}"
		);
	}
}

#[test]
fn emoji_sequences_match_per_character_printing_at_every_width() {
	let words = [
		"🙂",
		"👍🏽",
		"👨\u{200d}👩\u{200d}👧",
		"❤\u{fe0f}",
		"🏳\u{fe0f}\u{200d}🌈",
		"e\u{301}",
		"a\u{308}\u{323}",
		"⚠\u{fe0f}",
		"ok",
	];
	assert_matches_per_character(&words.join(" "), 13);
	assert_matches_per_character(&words.concat(), 13);
}

#[test]
fn marks_on_the_last_column_join_it_while_the_wrap_is_pending() {
	// The mark lands while the wrap is pending, and the next character wraps.
	assert_matches_per_character("abcde\u{301}f", 5);
	assert_matches_per_character("abc猫\u{fe0f}\u{301}d", 5);
	assert_matches_per_character("\x1b[?7labcdef\u{301}g猫\u{301}", 5);
}

#[test]
fn characters_after_a_joiner_join_even_when_they_would_not_fit() {
	// The woman is wide and the row has one column left, but she joins the
	// man's cell instead of wrapping.
	assert_matches_per_character("abc👨\u{200d}👩d", 5);
	assert_matches_per_character("x\u{200d}y\u{200d}\u{200d}z", 4);
	let term = printed("abc👨\u{200d}👩d", 5);
	assert_eq!(term.extras.marks(term.grid[0][3].extra), ['\u{200d}', '👩']);
	assert_eq!(term.grid[1][0].ch, 'd');
}

#[test]
fn marks_after_a_narrow_run_join_its_last_character() {
	// The run is long enough to reach the chunked path before and after.
	let run = "abcdefghijklmnopqrstuvwxyz0123456789";
	assert_matches_per_character(&format!("{run}e\u{301}\u{302}{run}д\u{301}{run}"), 40);
}

#[test]
fn marks_keep_the_hyperlink_of_the_character_they_join() {
	let linked = "\x1b]8;;https://example.com\x1b\\e\u{301}👨\u{200d}👩\x1b]8;;\x1b\\x\u{301}";
	assert_matches_per_character(linked, 8);
	let term = printed(linked, 8);
	let link = term.extras.hyperlink(term.grid[0][0].extra);
	assert_ne!(link, 0);
	assert_eq!(term.extras.hyperlink(term.grid[0][1].extra), link);
	assert_eq!(term.extras.marks(term.grid[0][1].extra), ['\u{200d}', '👩']);
	assert_eq!(term.extras.hyperlink(term.grid[0][3].extra), 0);
	assert_eq!(term.extras.marks(term.grid[0][3].extra), ['\u{301}']);
}

#[test]
fn marks_split_across_writes_still_join() {
	let parts: [&[u8]; 4] = [
		b"ab\xf0\x9f\x91\xa8",
		b"\xe2\x80",
		b"\x8d\xf0\x9f",
		b"\x91\xa9",
	];
	let mut term = Term::new(6, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	for part in parts {
		parser.advance(&mut term, part);
	}
	assert_eq!(term.extras.marks(term.grid[0][2].extra), ['\u{200d}', '👩']);
	assert_eq!((term.cx, term.grid[0][4].ch), (4, ' '));
}

fn printed(stream: &str, cols: usize) -> Term {
	let mut term = Term::new(cols, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, stream.as_bytes());
	term
}

#[test]
fn marks_leave_cells_unchanged_once_the_extras_table_is_full() {
	let mut batched = printed("\x1b]8;;https://example.com\x1b\\ab", 8);
	let extras = std::sync::Arc::make_mut(&mut batched.extras);
	for link in 1..=u16::MAX {
		if extras.with_hyperlink(NO_EXTRA, link).is_none() {
			break;
		}
	}
	let mut reference = batched.clone();
	let stream = "e\u{301}👨\u{200d}👩c\u{302}";
	Parser::new().advance(&mut batched, stream.as_bytes());
	vte::Parser::new().advance(&mut PerCharacter(&mut reference), stream.as_bytes());
	assert_eq!(screen(&batched), screen(&reference));
	// The marks are dropped, and the cells keep their hyperlink.
	let link = batched.grid[0][0].extra;
	assert_ne!(link, NO_EXTRA);
	for x in [2, 3, 5] {
		assert_eq!(batched.grid[0][x].extra, link, "column {x}");
	}
}

#[test]
fn line_drawing_covers_the_whole_dec_special_graphics_set() {
	// The VT100's DEC Special Graphics: 0x5F to 0x7E replace ASCII, the
	// letters b to e, h and i with symbols for format effectors.
	let ascii = "_`abcdefghijklmnopqrstuvwxyz{|}~";
	let drawn = " ◆▒␉␌␍␊°±␤␋┘┐┌└┼⎺⎻─⎼⎽├┤┴┬│≤≥π≠£·";
	assert_eq!(
		ascii.chars().map(super::dec_graphic).collect::<String>(),
		drawn
	);
	// The rest of ASCII is unchanged.
	assert!(('!'..='^').all(|c| super::dec_graphic(c) == c));
}

#[test]
fn segments_store_characters_at_the_edges_of_valid_scalars_intact() {
	// The segment path writes cells as raw words (`Cell::words_mut`), so lane
	// 0 must stay a valid `char`: check the scalars beside the surrogate gap
	// and at the top of Unicode, after a narrow run and next to wide ones.
	let edges = [
		'\u{D7FF}',
		'\u{E000}',
		'\u{FFFD}',
		'\u{10FFFD}',
		'\u{10FFFF}',
	];
	for edge in edges {
		let stream = format!("abcdefgh{edge}中{edge}");
		let term = printed(&stream, 20);
		let row: Vec<(char, u8)> = term.grid.iter().next().unwrap()[..14]
			.iter()
			.map(|c| (c.ch, c.width))
			.collect();
		let width = crate::width::width(edge) as u8;
		let mut expected: Vec<(char, u8)> = "abcdefgh".chars().map(|c| (c, 1)).collect();
		expected.push((edge, width));
		expected.extend([('中', 2), (' ', 0), (edge, width)]);
		assert_eq!(row[..expected.len()], expected[..], "U+{:04X}", edge as u32);
		assert_matches_per_character(&stream, 14);
	}
}
