//! Selective erase: DECSCA (`CSI Ps " q`) marks the characters printed next
//! as protected, and DECSED (`CSI ? Ps J`) and DECSEL (`CSI ? Ps K`) erase
//! all but those.
use crate::{
	parser::Parser,
	term::{Term, attr::*},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(5, 3, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn feed(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The screen as text, one string per row.
fn screen(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|y| {
			term.grid[y]
				.iter()
				.filter(|cell| cell.width != 0)
				.map(|cell| cell.ch)
				.collect()
		})
		.collect()
}

/// Rows `abcde`, `fghij` and `klmno`, with `c`, `h` and `m` protected, and
/// the cursor on the `h`.
fn columns() -> Term {
	let mut t = terminal();
	let protect = |c: char| format!("\x1b[1\"q{c}\x1b[0\"q");
	let rows = [("ab", 'c', "de"), ("fg", 'h', "ij"), ("kl", 'm', "no")];
	for (before, middle, after) in rows {
		feed(&mut t, &format!("{before}{}{after}", protect(middle)));
	}
	feed(&mut t, "\x1b[2;3H");
	t
}

#[test]
fn the_fixture_protects_the_middle_column() {
	let t = columns();
	assert_eq!(screen(&t), ["abcde", "fghij", "klmno"]);
	assert_eq!((t.cx, t.cy), (2, 1));
}

#[test]
fn decsed_erases_below_above_or_all_but_protected_characters() {
	for (mode, expected) in [
		("", ["abcde", "fgh  ", "  m  "]),
		("0", ["abcde", "fgh  ", "  m  "]),
		("1", ["  c  ", "  hij", "klmno"]),
		("2", ["  c  ", "  h  ", "  m  "]),
	] {
		let mut t = columns();
		feed(&mut t, &format!("\x1b[?{mode}J"));
		assert_eq!(screen(&t), expected, "DECSED {mode:?}");
		assert_eq!((t.cx, t.cy), (2, 1), "the cursor stays");
	}
}

#[test]
fn decsel_erases_only_the_cursor_line() {
	for (mode, expected) in [
		("", "fgh  "),
		("0", "fgh  "),
		("1", "  hij"),
		("2", "  h  "),
	] {
		let mut t = columns();
		feed(&mut t, &format!("\x1b[?{mode}K"));
		assert_eq!(screen(&t), ["abcde", expected, "klmno"], "DECSEL {mode:?}");
	}
}

#[test]
fn unknown_modes_erase_nothing() {
	let mut t = columns();
	feed(&mut t, "\x1b[?4J\x1b[?3K\x1b[?99J");
	assert_eq!(screen(&t), ["abcde", "fghij", "klmno"]);
}

#[test]
fn decsed_3_clears_the_scrollback_and_leaves_the_screen() {
	let mut t = columns();
	feed(&mut t, "\x1b[3;1H\n\n");
	assert!(!t.scrollback.is_empty());
	let before = screen(&t);
	feed(&mut t, "\x1b[?3J");
	assert!(t.scrollback.is_empty());
	assert_eq!(screen(&t), before);
}

#[test]
fn the_scroll_region_does_not_limit_decsed() {
	let mut t = columns();
	feed(&mut t, "\x1b[2;2r\x1b[2;3H\x1b[?0J");
	assert_eq!(screen(&t), ["abcde", "fgh  ", "  m  "]);
}

#[test]
fn decsca_2_and_0_are_both_erasable_and_others_change_nothing() {
	for (mode, protected) in [("0", false), ("2", false), ("", false), ("1", true)] {
		let mut t = terminal();
		feed(&mut t, &format!("\x1b[1\"q\x1b[{mode}\"qx\x1b[?2J"));
		assert_eq!(screen(&t)[0].starts_with('x'), protected, "DECSCA {mode:?}");
	}
	let mut t = terminal();
	feed(&mut t, "\x1b[1\"q\x1b[3\"qx\x1b[?2J");
	assert!(screen(&t)[0].starts_with('x'), "DECSCA 3 is not a value");
}

#[test]
fn plain_erases_ignore_and_drop_the_protection() {
	for erase in ["\x1b[2J", "\x1b[2K", "\x1b[1X", "\x1b[1P"] {
		let mut t = terminal();
		feed(&mut t, &format!("\x1b[1\"qx\x1b[0\"q\x1b[H{erase}"));
		assert_eq!(screen(&t)[0], "     ", "{erase:?} erased it");
		feed(&mut t, "\x1b[Hy\x1b[?2J");
		assert_eq!(
			screen(&t)[0],
			"     ",
			"after {erase:?} the cell is erasable"
		);
	}
}

#[test]
fn overwriting_a_protected_character_follows_the_current_protection() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1\"qab\x1b[0\"q\x1b[Hx\x1b[?2J");
	assert_eq!(screen(&t)[0], " b   ");
	feed(&mut t, "\x1b[1\"q\x1b[Hy\x1b[0\"q\x1b[?2J");
	assert_eq!(screen(&t)[0], "yb   ");
}

#[test]
fn erased_cells_keep_their_colors_and_attributes() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;3;41;58;5;2mx\x1b[0m\x1b[?2K");
	let cell = t.grid[0][0];
	assert_eq!(cell.ch, ' ');
	assert_eq!(cell.bg, t.indexed_color(1));
	assert_ne!(cell.attrs & ITALIC, 0);
	assert_eq!(
		t.extras.underline_color(cell.extra),
		Some(t.indexed_color(2))
	);
}

#[test]
fn erased_cells_lose_their_marks_and_links() {
	let mut t = terminal();
	feed(
		&mut t,
		"e\u{301}\x1b]8;;https://x\x1b\\y\x1b]8;;\x1b\\\x1b[?2K",
	);
	assert!(t.marks(&t.grid[0][0]).is_empty());
	assert_eq!(t.hyperlink_uri(&t.grid[0][1]), None);
}

#[test]
fn protection_survives_marks_links_and_underline_colors() {
	let mut t = terminal();
	feed(
		&mut t,
		"\x1b[1\"q\x1b[58;5;3me\u{301}\x1b]8;;https://x\x1b\\y\x1b]8;;\x1b\\\x1b[59mz\x1b[0\"q\x1b[?2J",
	);
	assert_eq!(screen(&t)[0], "eyz  ");
	assert_eq!(t.marks(&t.grid[0][0]), ['\u{301}']);
	assert_eq!(t.hyperlink_uri(&t.grid[0][1]), Some("https://x"));
}

#[test]
fn wide_characters_go_or_stay_whole() {
	let mut t = terminal();
	feed(&mut t, "猫\x1b[1\"q犬\x1b[0\"q\x1b[?2K");
	assert_eq!(screen(&t)[0], "  犬 ");
	assert_eq!((t.grid[0][0].width, t.grid[0][1].width), (1, 1));
	assert_eq!((t.grid[0][2].width, t.grid[0][3].width), (2, 0));
	// The cursor inside a wide character erases all of it, as EL does.
	let mut t = terminal();
	feed(&mut t, "ab猫\x1b[1;4H\x1b[?0K");
	assert_eq!(screen(&t)[0], "ab   ");
}

#[test]
fn the_protection_is_saved_with_the_cursor_and_reset_softly() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1\"q\x1b7\x1b[0\"q\x1b8x\x1b[?2J");
	assert_eq!(screen(&t)[0], "x    ", "DECRC restored it");
	feed(&mut t, "\x1b[1\"q\x1b[!p\x1b[Hy\x1b[?2J");
	assert_eq!(screen(&t)[0], "     ", "DECSTR cleared it");
	feed(&mut t, "\x1b[1\"q\x1bc\x1b[Hz\x1b[?2J");
	assert_eq!(screen(&t)[0], "     ", "RIS cleared it");
}

#[test]
fn decrqss_reports_the_protection() {
	let mut t = terminal();
	feed(&mut t, "\x1bP$q\"q\x1b\\\x1b[1\"q\x1bP$q\"q\x1b\\");
	assert_eq!(
		t.responses,
		[
			b"\x1bP1$r0\"q\x1b\\".to_vec(),
			b"\x1bP1$r1\"q\x1b\\".to_vec()
		]
	);
}

#[test]
fn deccir_reports_and_restores_the_protection() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1\"q\x1b[1$w");
	let report = String::from_utf8(t.responses.pop().unwrap()).unwrap();
	assert_eq!(report.split(';').nth(4), Some("A"), "{report:?}");
	feed(&mut t, "\x1b[0\"q\x1b[1$w");
	let plain = String::from_utf8(t.responses.pop().unwrap()).unwrap();
	assert_eq!(plain.split(';').nth(4), Some("@"));
	// Restoring the first report turns the protection back on.
	let restore = report.replacen("$u", "$t", 1);
	feed(&mut t, &format!("{restore}x\x1b[?2J"));
	assert!(screen(&t).concat().contains('x'));
}

#[test]
fn primary_da_lists_selective_erase() {
	let mut t = terminal();
	feed(&mut t, "\x1b[c");
	let reply = String::from_utf8(t.responses.remove(0)).unwrap();
	let codes: Vec<&str> = reply[3..reply.len() - 1].split(';').collect();
	assert!(codes.contains(&"6"), "{reply:?}");
}

#[test]
fn selective_erase_split_across_reads() {
	let mut t = columns();
	let mut parser = Parser::new();
	for chunk in ["\x1b", "[?", "2", "J"] {
		parser.advance(&mut t, chunk.as_bytes());
	}
	assert_eq!(screen(&t), ["  c  ", "  h  ", "  m  "]);
}

#[test]
fn the_scrollback_is_left_alone_by_modes_0_to_2() {
	let mut t = terminal();
	feed(&mut t, "old\r\n\r\n\r\nnew");
	let history = t.scrollback.len();
	assert!(history > 0);
	for mode in 0..=2 {
		feed(&mut t, &format!("\x1b[?{mode}J"));
		assert_eq!(t.scrollback.len(), history, "DECSED {mode}");
	}
	let first = t.scrollback.get(0).unwrap();
	assert_eq!(
		first.iter().take(3).map(|cell| cell.ch).collect::<String>(),
		"old"
	);
}

#[test]
fn the_alternate_screen_erases_only_itself() {
	let mut t = terminal();
	feed(&mut t, "main\x1b[?1049h\x1b[1\"qk\x1b[0\"qx\x1b[?2J");
	assert_eq!(screen(&t)[0], "k    ");
	feed(&mut t, "\x1b[?1049l");
	assert_eq!(screen(&t)[0], "main ");
}

#[test]
fn margins_do_not_limit_selective_erase() {
	let mut t = terminal();
	feed(&mut t, "abcde\x1b[?69h\x1b[2;3s\x1b[1;3H\x1b[?2K");
	assert_eq!(screen(&t)[0], "     ", "DECSEL");
	let mut t = terminal();
	feed(
		&mut t,
		"abcdefghij\x1b[?69h\x1b[2;3s\x1b[?6h\x1b[1;1H\x1b[?0J",
	);
	assert_eq!(
		screen(&t)[..2],
		["a    ", "     "],
		"DECSED from the origin"
	);
}

#[test]
fn a_pending_wrap_erases_from_the_last_column() {
	let mut t = terminal();
	feed(&mut t, "abcde");
	assert!(t.wrap_pending);
	feed(&mut t, "\x1b[?0K");
	assert_eq!(screen(&t)[0], "abcd ");
	let mut t = terminal();
	feed(&mut t, "abcde\x1b[?1K");
	assert_eq!(screen(&t)[0], "     ");
}

#[test]
fn erased_blanks_do_not_take_the_current_background() {
	// Unlike EL, which fills with the current colors.
	let mut t = terminal();
	feed(&mut t, "ab\x1b[44m\x1b[?2K");
	let default_bg = t.default_bg;
	assert!(t.grid[0].iter().all(|cell| cell.bg == default_bg));
	assert_eq!(t.grid.used(0), 2, "the padding stays padding");
	feed(&mut t, "\x1b[2K");
	assert!(t.grid[0].iter().all(|cell| cell.bg == t.indexed_color(4)));
}

#[test]
fn scaled_text_is_erased_whole_or_kept_whole() {
	let mut t = Term::new(8, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	feed(
		&mut t,
		"\x1b]66;s=2;a\x07\x1b[1\"q\x1b]66;s=2;b\x07\x1b[0\"q\x1b[?2J",
	);
	let multicell = |x: usize, y: usize| t.extras.multicell(t.grid[y][x].extra).is_some();
	for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
		assert!(!multicell(x, y), "the plain block is gone at ({x}, {y})");
		assert_eq!(t.grid[y][x].ch, ' ');
	}
	for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
		assert!(multicell(x, y), "the protected block stays at ({x}, {y})");
	}
	// Erasing the top row of a block erases its lower half too.
	let mut t = Term::new(8, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	feed(&mut t, "\x1b]66;s=2;a\x07\x1b[1;1H\x1b[?2K");
	let multicell = |x: usize, y: usize| t.extras.multicell(t.grid[y][x].extra).is_some();
	assert!(
		!multicell(0, 1) && !multicell(1, 1),
		"no half block is left"
	);
}

#[test]
fn only_the_first_parameter_counts() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;0\"qp\x1b[0\"qx\x1b[?2;0J");
	assert_eq!(
		screen(&t)[0],
		"p    ",
		"DECSCA 1;0 protects, DECSED 2;0 erases all"
	);
	let mut t = terminal();
	feed(&mut t, "\x1b[65535\"qx\x1b[?65535J\x1b[?65535K");
	assert_eq!(screen(&t)[0], "x    ", "out of range values do nothing");
	assert!(!t.protected);
}

#[test]
fn the_cursor_stays_and_its_pending_wrap_ends_as_with_ed() {
	for erase in ["\x1b[?2J", "\x1b[2J", "\x1b[?2K", "\x1b[2K"] {
		let mut t = terminal();
		feed(&mut t, &format!("\x1b[1\"qabcde{erase}"));
		assert_eq!((t.cx, t.cy, t.wrap_pending), (4, 0, false), "{erase:?}");
		feed(&mut t, "f");
		assert_eq!(
			t.grid[0][4].ch, 'f',
			"{erase:?}: the next character overwrites"
		);
	}
}
