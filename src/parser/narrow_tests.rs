//! Tests of the narrow-character fast path: chunks alone, and whole streams
//! against per-character printing.
use std::simd::u32x4;

use super::{CHUNK, NARROW_BLOCKS, in_narrow_block, print_chunk};
use crate::{
	parser::{Parser, PerCharacter},
	term::{Cell, Term},
	theme::{Theme, ThemeKind},
};

fn style() -> Cell {
	Cell {
		ch: '\0',
		fg: 0x112233,
		bg: 0x445566,
		attrs: 0x5a,
		width: 0,
		extra: 0x1234,
	}
}

/// The narrow cell `print_chunk` takes: `style` with width 1 and no character.
fn narrow() -> u32x4 {
	Cell::with_width(Cell::without_char(style().words()), 1)
}

fn chunk(text: &str) -> [u8; CHUNK] {
	text.as_bytes()[..CHUNK].try_into().unwrap()
}

/// A row of `len` cells that no chunk writes, as words.
fn row(len: usize) -> Vec<u32x4> {
	vec![Cell::default().words(); len]
}

fn cells(row: &[u32x4]) -> Vec<Cell> {
	// SAFETY: every word in these rows holds a valid cell.
	row
		.iter()
		.map(|w| unsafe { std::mem::transmute(*w) })
		.collect()
}

fn text(row: &[u32x4]) -> String {
	cells(row).iter().map(|c| c.ch).collect()
}

#[test]
fn ascii_fills_one_cell_per_byte_in_the_style() {
	let mut row = row(20);
	assert_eq!(
		print_chunk(&chunk("the quick brown fox"), &mut row, narrow()),
		Some((16, 16, ' '))
	);
	assert_eq!(text(&row[..16]), "the quick brown ");
	let expected = Cell {
		ch: 't',
		width: 1,
		..style()
	};
	assert_eq!(cells(&row)[0], expected);
	assert_eq!(cells(&row)[16], Cell::default(), "past the chunk");
}

#[test]
fn two_and_three_byte_characters_take_one_cell_each() {
	for (input, len, written) in [
		("naïve café über ", 16, "naïve café ü"),
		("привет мир", 15, "привет м"),
		("αβγδεζηθικλ", 16, "αβγδεζηθ"),
		("a→b—c─d…e€f", 16, "a→b—c─d"),
		(
			"\u{800}\u{1000}\u{20ac}x\u{7ff}",
			16,
			"\u{800}\u{1000}\u{20ac}x\u{7ff}",
		),
	] {
		let mut row = row(CHUNK);
		let bytes = chunk(&format!("{input}{}", " ".repeat(CHUNK)));
		let (used, cells_used, last) = print_chunk(&bytes, &mut row, narrow()).expect(input);
		let decoded = std::str::from_utf8(&bytes[..used]).unwrap();
		assert_eq!(used, len, "{input}");
		assert!(decoded.starts_with(written), "{input}: {decoded:?}");
		assert_eq!(cells_used, decoded.chars().count(), "{input}");
		assert_eq!(text(&row[..cells_used]), decoded, "{input}");
		assert_eq!(Some(last), decoded.chars().last(), "{input}");
		assert!(
			cells(&row[..cells_used])
				.iter()
				.all(|c| c.width == 1 && c.fg == 0x112233)
		);
	}
}

#[test]
fn characters_cut_by_the_chunk_end_are_left_for_later() {
	for (input, len) in [
		("aaaaaaaaaaaaaaaé", 15),
		("aaaaaaaaaaaaaa€", 14),
		("aaaaaaaaaaaaaaa€", 15),
		("aaaaaaaaaaaaa€", 16),
		("aaaaaaaaaaaaaaé", 16),
	] {
		let mut row = row(CHUNK);
		let bytes = chunk(&format!("{input}{}", " ".repeat(CHUNK)));
		let (used, cells_used, _) = print_chunk(&bytes, &mut row, narrow()).expect(input);
		assert_eq!(used, len, "{input}");
		let printed = std::str::from_utf8(&bytes[..used]).expect("ends at a character");
		assert_eq!(cells_used, printed.chars().count(), "{input}");
		assert_eq!(text(&row[..cells_used]), printed, "{input}");
		assert_eq!(
			cells(&row)[cells_used],
			Cell::default(),
			"{input}: nothing past it"
		);
	}
}

#[test]
fn characters_that_are_not_narrow_or_printable_reject_the_chunk_untouched() {
	for input in [
		"abcdefghijklmno\u{1}",
		"abcdefghij\u{1b}[0mxyz",
		"abcdefghijklmn\u{7f}o",
		"abcdefghij\tklmnopq",
		"abcdefghijk日本語",
		"abcdefghijk한국어",
		"abcdefghijkl🙂xy",
		"abcdefghijkl\u{231a}xy",
		"abcdefghijkl\u{2e80}xy",
		"abcdefghijke\u{301}xyz",
		"abcdefghijk\u{200b}xyz",
		"abcdefghij\u{ad}xyzw",
		"\u{ff21}bcdefghijklmnopq",
	] {
		let mut row = row(CHUNK);
		let bytes = chunk(&format!("{input}{}", " ".repeat(CHUNK)));
		assert_eq!(print_chunk(&bytes, &mut row, narrow()), None, "{input:?}");
		assert!(
			cells(&row).iter().all(|c| *c == Cell::default()),
			"{input:?}"
		);
	}
}

#[test]
fn chunks_that_do_not_fit_the_row_are_rejected_untouched() {
	let bytes = chunk("ééééééééé");
	for (len, fits) in [(0, false), (7, false), (8, true), (9, true)] {
		let mut row = row(len);
		let printed = print_chunk(&bytes, &mut row, narrow());
		assert_eq!(printed.is_some(), fits, "row of {len}");
		if fits {
			assert_eq!(text(&row[..8]), "éééééééé");
		} else {
			assert!(
				cells(&row).iter().all(|c| *c == Cell::default()),
				"row of {len}"
			);
		}
	}
}

#[test]
fn narrow_blocks_are_exactly_the_blocks_of_narrow_characters() {
	for block in 0..0x3000 >> 5 {
		let narrow = (block << 5..=block << 5 | 31).all(|c| {
			(0x80..0xa0).contains(&c)
				|| char::from_u32(c.into()).is_some_and(|c| crate::width::width(c) == 1)
		});
		assert_eq!(in_narrow_block(block << 5), narrow, "U+{:04X}", block << 5);
		assert_eq!(
			in_narrow_block(block << 5 | 31),
			narrow,
			"U+{:04X}",
			block << 5 | 31
		);
	}
	assert_eq!(
		NARROW_BLOCKS[6..],
		[0; 26],
		"nothing from U+3000 on is decoded"
	);
}

#[test]
fn cell_words_hold_each_field_in_place() {
	let words = style().words();
	assert_eq!(words.to_array(), [0, 0x112233, 0x445566, 0x1234_005a]);
	assert_eq!(
		Cell::without_char(Cell { ch: 'x', ..style() }.words()),
		words
	);
	for width in 0..=2 {
		let built = Cell::with_char(Cell::with_width(words, width), 'é' as u32);
		let expected = Cell {
			ch: 'é',
			width: width as u8,
			..style()
		};
		assert_eq!(
			// SAFETY: lane 0 holds a valid char.
			unsafe { std::mem::transmute::<u32x4, Cell>(built) },
			expected
		);
	}
	let mut row = [Cell::default(); 2];
	Cell::words_mut(&mut row)[1] = words;
	assert_eq!(row, [Cell::default(), style()]);
}

/// Feeds `stream` whole and through vte one character at a time and asserts
/// the screens, history and cursors match.
fn assert_matches_per_character(stream: &[u8], cols: usize, rows: usize) {
	let mut batched = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	batched.scrollback_limit = 50;
	let mut reference = batched.clone();
	Parser::new().advance(&mut batched, stream);
	vte::Parser::new().advance(&mut PerCharacter(&mut reference), stream);
	let context = String::from_utf8_lossy(stream);
	let cursor = |t: &Term| (t.cx, t.cy, t.wrap_pending, t.last_printed);
	assert_eq!(
		cursor(&batched),
		cursor(&reference),
		"{cols} columns: {context:?}"
	);
	// Marks are interned in the same order on both sides, so ids agree.
	for y in 0..rows {
		assert_eq!(
			batched.grid[y], reference.grid[y],
			"row {y}, {cols} columns: {context:?}"
		);
	}
	let history = |t: &Term| {
		t.scrollback
			.iter()
			.map(<[Cell]>::to_vec)
			.collect::<Vec<_>>()
	};
	assert_eq!(
		history(&batched),
		history(&reference),
		"{cols} columns: {context:?}"
	);
}

#[test]
fn long_narrow_runs_match_per_character_printing() {
	let lines = [
		"Ünïcödé façade naïveté, crème brûlée — “quoted” ‘text’ … ±½° and so on",
		"Съешь же ещё этих мягких французских булок, да выпей чаю. Ещё раз!",
		"Τάχιστη αλώπηξ βαφής ψημένη γη, δρασκελίζει υπέρ νωθρού κυνός",
		"→ ← ↑ ↓ ↔ ⇒ ─│┌┐└┘├┤┬┴┼ ═║╔╗ ▀▄█▌▐░▒▓ • ‣ ※ ‰ € £ ¥ © ® ™",
		"mixed ascii and é every few letters é and more ascii é the end",
	];
	for cols in [80, 33, 17, 16, 15, 2] {
		for line in lines {
			let stream = format!("{line}\r\n{line}{line}\r\n\x1b[5G{line}");
			assert_matches_per_character(stream.as_bytes(), cols, 4);
		}
	}
}

#[test]
fn narrow_runs_next_to_other_text_match_per_character_printing() {
	let run = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя";
	let fragments = [
		run,
		"plain ascii words go here ",
		"日本語のテキスト",
		"🙂👍🏽👨\u{200d}👩",
		"e\u{301}\u{302}",
		"\u{200d}é",
		"\u{ad}\u{200b}\u{2028}",
		"\u{231a}\u{2e80}\u{1100}\u{1160}",
		"\u{85}\u{9b}31m",
		"\x1b[1;31m",
		"\x1b[0m",
		"\x1b]8;;https://example.com\x1b\\",
		"\x1b]8;;\x1b\\",
		"\r\n",
		"\x1b[?7l",
		"\x1b[?7h",
		"\x1b[4h",
		"\x1b[4l",
		"\x1b(0lqqk\x1b(B",
		"\x1b[3G",
		"\u{10eeee}\u{305}",
	];
	let mut seed = 0x2545_f491_u32;
	for _ in 0..300 {
		let mut stream = String::new();
		for _ in 0..12 {
			seed ^= seed << 13;
			seed ^= seed >> 17;
			seed ^= seed << 5;
			stream.push_str(fragments[seed as usize % fragments.len()]);
		}
		for cols in [80, 21] {
			assert_matches_per_character(stream.as_bytes(), cols, 3);
		}
	}
}

#[test]
fn wide_characters_cut_by_a_narrow_run_are_repaired() {
	let run = "ééééééééééééééééééééééé";
	for start in 1..6 {
		let stream = format!("日本語日本語日本語日本語\x1b[1;{start}H{run}");
		assert_matches_per_character(stream.as_bytes(), 24, 2);
		let stream = format!("{run}\x1b[1;{start}H日本語日本語日本語日本語");
		assert_matches_per_character(stream.as_bytes(), 24, 2);
	}
	let stream = format!("\x1b[1;20H日本語日本語\x1b[1;1H{run}");
	assert_matches_per_character(stream.as_bytes(), 24, 2);
}

#[test]
fn narrow_characters_in_blocks_with_marks_are_looked_up_and_taken() {
	// Each block also holds combining marks or wide characters, so these take
	// the width lookup instead of the bitmap, and still print here.
	for c in [
		'\u{48a}', '\u{370}', '\u{a8}', '\u{5d0}', '\u{2010}', '\u{2300}',
	] {
		assert!(
			!in_narrow_block(c as u16),
			"U+{:04X} is in a mixed block",
			c as u32
		);
		let input = format!("{c}abcdefghijklmnopq");
		let mut row = row(CHUNK);
		let (used, cells_used, _) =
			print_chunk(&chunk(&input), &mut row, narrow()).unwrap_or_else(|| panic!("{input}"));
		let printed = &input[..used];
		assert_eq!(cells_used, printed.chars().count(), "{input}");
		assert_eq!(text(&row[..cells_used]), printed, "{input}");
	}
}

/// Feeds `parts` one `advance` call at a time.
fn fed(parts: &[&[u8]], cols: usize, rows: usize) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = Parser::new();
	for part in parts {
		parser.advance(&mut term, part);
	}
	term
}

#[test]
fn streams_split_anywhere_print_like_whole_streams() {
	let stream = "ascii, then éèêë and Привет мир — “ok” ─┼─ ±½ done, ascii 日本".as_bytes();
	let whole = fed(&[stream], 20, 4);
	for split in 1..stream.len() {
		let parts = fed(&[&stream[..split], &stream[split..]], 20, 4);
		assert_eq!(
			(parts.cx, parts.cy, parts.last_printed),
			(whole.cx, whole.cy, whole.last_printed),
			"split at byte {split}"
		);
		for y in 0..4 {
			assert_eq!(
				parts.grid[y], whole.grid[y],
				"row {y}, split at byte {split}"
			);
		}
	}
}

#[test]
fn narrow_runs_without_auto_wrap_overwrite_the_last_column() {
	let run = "абвгдеёжзийклмнопрстуфхцчшщ".repeat(2);
	for cols in [40, 20, 16, 9] {
		let stream = format!("\x1b[?7l{run}\x1b[1;3H{run}é");
		assert_matches_per_character(stream.as_bytes(), cols, 2);
		let term = fed(&[stream.as_bytes()], cols, 2);
		assert_eq!(term.grid[0][cols - 1].ch, 'é', "{cols} columns");
		assert_eq!(term.grid[1][0].ch, ' ', "{cols} columns: nothing wrapped");
	}
}

#[test]
fn every_style_survives_the_fast_path() {
	let run = "Ünïcödé façade naïveté — ±½°";
	for sgr in [
		"1",
		"2",
		"3",
		"4",
		"4:3",
		"5",
		"7",
		"8",
		"9",
		"1;7",
		"2;8",
		"31",
		"1;34",
		"38;5;196",
		"48;5;22",
		"38;2;1;2;3;48;2;4;5;6",
		"4;58;5;3",
		"53",
	] {
		let stream = format!("{run}\x1b[{sgr}m{run}\x1b[0m{run}");
		assert_matches_per_character(stream.as_bytes(), 100, 2);
		let term = fed(&[stream.as_bytes()], 100, 2);
		let n = run.chars().count();
		let (before, styled) = (term.grid[0][n - 1], term.grid[0][2 * n - 1]);
		assert_eq!(styled.ch, '°', "\\e[{sgr}m");
		assert_ne!(
			(styled.fg, styled.bg, styled.attrs, styled.extra),
			(before.fg, before.bg, before.attrs, before.extra),
			"\\e[{sgr}m changes the cell"
		);
		assert_eq!(term.grid[0][3 * n - 1], before, "\\e[{sgr}m is reset");
	}
	let link = format!("\x1b]8;;https://example.com\x1b\\{run}\x1b]8;;\x1b\\{run}");
	assert_matches_per_character(link.as_bytes(), 80, 2);
}

#[test]
fn marks_right_after_a_chunk_join_its_last_character() {
	for lead in 0..20 {
		for mark in ["\u{301}", "\u{20d7}", "\u{200d}é", "\u{fe0f}", "\u{200b}"] {
			let stream = format!("{}{}{mark}xyz", "a".repeat(lead), "éèêëāēī".repeat(3));
			for cols in [80, 24] {
				assert_matches_per_character(stream.as_bytes(), cols, 2);
			}
		}
	}
}

#[test]
fn narrow_runs_over_scaled_text_images_and_inserts_take_the_general_path() {
	let run = "Съешь же ещё этих мягких булок — ±½° éèêë";
	for (setup, after) in [
		("\x1b]66;s=2;ab\x07\x1b[1;1H", ""),
		("\x1b]66;n=1:d=2:w=1;ab\x1b\\\x1b[1;2H", ""),
		("\x1bPq#0;2;0;0;0~~~~~~-~~~~~~\x1b\\\x1b[1;1H", ""),
		("abcdefghij\x1b[1;3H\x1b[4h", "\x1b[4l"),
		("日本語日本語\x1b[1;2H\x1b[4h", "\x1b[4l"),
		("\x1b(0", "\x1b(B"),
	] {
		let stream = format!("{setup}{run}{after}{run}");
		for cols in [80, 30] {
			assert_matches_per_character(stream.as_bytes(), cols, 4);
		}
	}
	// Inserting shifts the earlier text right instead of overwriting it.
	let term = fed(
		&[format!("abcdefghij\x1b[1;3H\x1b[4h{run}").as_bytes()],
		80,
		2,
	);
	let shifted = run.chars().count() + 2;
	let tail: String = term.grid[0][shifted..shifted + 8]
		.iter()
		.map(|c| c.ch)
		.collect();
	assert_eq!(tail, "cdefghij");
	// Scaled text keeps its block; narrow text after it starts past both cells.
	let term = fed(&["\x1b]66;s=2;a\x07éèêëāēīōūабвгд".as_bytes()], 40, 3);
	assert_eq!(term.grid[0][2].ch, 'é');
	assert_eq!(term.grid[0][15].ch, 'д');
}
