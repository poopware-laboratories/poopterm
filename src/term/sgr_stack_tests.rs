//! XTPUSHSGR (`CSI Pm # {`) and XTPOPSGR (`CSI # }`): a stack of up to 10
//! saved renditions, each restoring all attributes or only those chosen.
use crate::{
	parser::Parser,
	term::{Term, attr::*},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 4, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn feed(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// Everything SGR sets, to compare before and after.
#[derive(Debug, PartialEq)]
struct Style {
	bold: bool,
	reverse: bool,
	attrs: u8,
	fg: (u32, u32),
	bg: u32,
	underline_color: Option<u32>,
}

fn style(t: &Term) -> Style {
	Style {
		bold: t.bold,
		reverse: t.reverse,
		attrs: t.attrs,
		fg: (t.fg, t.fg_id),
		bg: t.bg,
		underline_color: t.underline_color,
	}
}

/// Sets every attribute, with colors from `base`.
fn everything(base: u8) -> String {
	format!(
		"\x1b[1;2;3;5;7;8;9;21;53;38;5;{base};48;5;{};58;5;{}m",
		base + 1,
		base + 2
	)
}

#[test]
fn pushing_without_parameters_saves_and_restores_everything() {
	let mut t = terminal();
	feed(&mut t, &everything(10));
	let saved = style(&t);
	feed(&mut t, "\x1b[#{\x1b[0m\x1b[4;38;5;200;48;2;1;2;3m\x1b[#}");
	assert_eq!(style(&t), saved);
	// And back to plain the same way.
	feed(&mut t, "\x1b[0m");
	let plain = style(&t);
	feed(&mut t, &format!("\x1b[0#{{{}\x1b[#}}", everything(20)));
	assert_eq!(style(&t), plain, "a lone 0 also saves everything");
}

#[test]
fn popping_an_empty_stack_changes_nothing() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;31m");
	let before = style(&t);
	feed(&mut t, "\x1b[#}\x1b[#}");
	assert_eq!(style(&t), before);
}

#[test]
fn chosen_attributes_are_the_only_ones_restored() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;31;42m\x1b[1;31#{\x1b[22;3;33;44m\x1b[#}");
	assert!(t.bold, "bold (1) restored");
	assert_eq!(t.bg, t.indexed_color(2), "background (31) restored");
	assert_eq!(t.fg, t.indexed_color(3), "foreground kept");
	assert_ne!(t.attrs & ITALIC, 0, "italic kept");
}

/// Each selector, an SGR that changes what it covers, and a check of it.
type Check = fn(&Term) -> bool;
const SELECTORS: &[(u16, &str, Check)] = &[
	(1, "1", |t| t.bold),
	(2, "2", |t| t.attrs & DIM != 0),
	(3, "3", |t| t.attrs & ITALIC != 0),
	(4, "4", |t| t.attrs & UNDERLINE != 0),
	(5, "5", |t| t.attrs & BLINK != 0),
	(7, "7", |t| t.reverse),
	(8, "8", |t| t.attrs & CONCEAL != 0),
	(9, "9", |t| t.attrs & STRIKE != 0),
	(21, "21", |t| t.attrs & DOUBLE_UNDERLINE != 0),
	(30, "35", |t| t.fg == t.indexed_color(5)),
	(31, "45", |t| t.bg == t.indexed_color(5)),
];

#[test]
fn each_selector_restores_its_attribute_and_no_other() {
	for &(selector, sgr, is_set) in SELECTORS {
		let mut t = terminal();
		// Saved plain, then everything set: only the selector's part returns.
		feed(
			&mut t,
			&format!("\x1b[{selector}#{{\x1b[1;2;3;5;7;8;9;21;35;45m\x1b[#}}"),
		);
		assert!(!is_set(&t), "{selector} restored");
		for &(other, _, other_set) in SELECTORS {
			let shared = matches!((selector, other), (4, 21) | (21, 4));
			if other != selector && !shared {
				assert!(other_set(&t), "{selector} left {other} alone");
			}
		}
		// Saved set, then cleared: the selector's part comes back set.
		let mut t = terminal();
		feed(
			&mut t,
			&format!("\x1b[{sgr}m\x1b[{selector}#{{\x1b[0m\x1b[#}}"),
		);
		assert!(is_set(&t), "{selector} restored when set");
	}
}

#[test]
fn single_and_double_underline_are_one_attribute_here() {
	for selector in [4, 21] {
		let mut t = terminal();
		feed(
			&mut t,
			&format!("\x1b[21m\x1b[{selector}#{{\x1b[24m\x1b[#}}"),
		);
		assert_eq!(
			t.attrs & (UNDERLINE | DOUBLE_UNDERLINE),
			UNDERLINE | DOUBLE_UNDERLINE
		);
	}
}

#[test]
fn unknown_selectors_still_push_a_level_that_restores_nothing() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1m\x1b[#{\x1b[22m\x1b[99;6;58#{\x1b[3m\x1b[#}");
	assert!(
		!t.bold && t.attrs & ITALIC != 0,
		"the empty level restored nothing"
	);
	feed(&mut t, "\x1b[#}");
	assert!(t.bold && t.attrs & ITALIC == 0, "the level below it did");
}

#[test]
fn levels_pop_in_reverse_order() {
	let mut t = terminal();
	feed(&mut t, "\x1b[31m\x1b[#{\x1b[32m\x1b[#{\x1b[33m");
	feed(&mut t, "\x1b[#}");
	assert_eq!(t.fg, t.indexed_color(2));
	feed(&mut t, "\x1b[#}");
	assert_eq!(t.fg, t.indexed_color(1));
}

#[test]
fn the_stack_holds_ten_levels_and_ignores_more() {
	let mut t = terminal();
	for index in 0..12 {
		feed(&mut t, &format!("\x1b[38;5;{index}m\x1b[#{{"));
	}
	feed(&mut t, "\x1b[38;5;99m");
	for index in (0..10).rev() {
		feed(&mut t, "\x1b[#}");
		assert_eq!(t.fg, t.indexed_color(index), "level {index}");
	}
	feed(&mut t, "\x1b[38;5;50m\x1b[#}");
	assert_eq!(t.fg, t.indexed_color(50), "the stack is empty");
}

#[test]
fn restored_attributes_print_with_the_next_text() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;4m\x1b[#{\x1b[0mA\x1b[#}B");
	assert_eq!(t.grid[0][0].attrs & UNDERLINE, 0);
	assert_ne!(t.grid[0][1].attrs & UNDERLINE, 0);
	assert!(t.bold);
}

#[test]
fn the_rendition_stack_is_not_the_color_stack() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1m\x1b[#{\x1b[22m\x1b[#Q\x1b[#R");
	assert_eq!(
		t.responses,
		vec![b"\x1b[0;0#Q".to_vec()],
		"no palette saved"
	);
	assert!(!t.bold, "XTPOPCOLORS popped nothing here");
	feed(&mut t, "\x1b[#P\x1b[#}");
	assert!(t.bold, "XTPOPSGR still finds its level");
}

#[test]
fn a_full_reset_empties_the_stack_and_a_soft_one_keeps_it() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1m\x1b[#{\x1b[!p");
	assert!(!t.bold);
	feed(&mut t, "\x1b[#}");
	assert!(t.bold, "DECSTR keeps the saved levels");
	feed(&mut t, "\x1b[3m\x1b[#{\x1bc\x1b[#}");
	assert_eq!(t.attrs & ITALIC, 0, "RIS dropped them");
}

#[test]
fn both_screens_share_the_stack() {
	let mut t = terminal();
	feed(&mut t, "\x1b[7m\x1b[#{\x1b[27m\x1b[?1049h\x1b[#}");
	assert!(t.reverse);
}

#[test]
fn pushes_and_pops_split_across_reads() {
	let mut t = terminal();
	let mut parser = Parser::new();
	for chunk in ["\x1b[1m\x1b[", "#", "{\x1b[22m\x1b", "[#", "}"] {
		parser.advance(&mut t, chunk.as_bytes());
	}
	assert!(t.bold);
}

#[test]
fn overline_and_underline_color_return_only_with_everything() {
	// No selector names them, so a chosen push leaves them as they are.
	let mut t = terminal();
	feed(&mut t, "\x1b[53;58;5;9m\x1b[1;4;30;31#{\x1b[55;59m\x1b[#}");
	assert_eq!(t.attrs & OVERLINE, 0);
	assert_eq!(t.underline_color, None);
	let mut t = terminal();
	feed(&mut t, "\x1b[53;58;5;9m\x1b[#{\x1b[55;59m\x1b[#}");
	assert_ne!(t.attrs & OVERLINE, 0);
	assert_eq!(t.underline_color, Some(t.indexed_color(9)));
}

#[test]
fn direct_colors_come_back_exactly() {
	let mut t = terminal();
	feed(&mut t, "\x1b[38;2;1;2;3;48:2::4:5:6;58:2::7:8:9m");
	let saved = style(&t);
	feed(&mut t, "\x1b[#{\x1b[38;5;1;48;5;2;58;5;3m\x1b[#}");
	assert_eq!(style(&t), saved);
}

#[test]
fn a_selector_with_subparameters_counts_by_its_main_number() {
	let mut t = terminal();
	feed(&mut t, "\x1b[32m\x1b[30:2#{\x1b[33m\x1b[#}");
	assert_eq!(t.fg, t.indexed_color(2));
}

#[test]
fn huge_selectors_are_ignored_beside_known_ones() {
	let mut t = terminal();
	feed(&mut t, "\x1b[1;3m\x1b[65535;1;99999#{\x1b[22;23m\x1b[#}");
	assert!(t.bold, "1 restored");
	assert_eq!(t.attrs & ITALIC, 0, "3 was not chosen");
}

#[test]
fn a_pop_frees_a_level_for_the_next_push() {
	let mut t = terminal();
	for _ in 0..10 {
		feed(&mut t, "\x1b[#{");
	}
	feed(&mut t, "\x1b[#}\x1b[1m\x1b[#{\x1b[22m\x1b[#}");
	assert!(t.bold, "the eleventh push found room after a pop");
}

#[test]
fn a_restored_underline_color_reaches_the_cells_and_keeps_the_link() {
	let mut t = terminal();
	feed(
		&mut t,
		"\x1b[58;2;1;2;3m\x1b[#{\x1b[59m\x1b]8;;https://x\x1b\\A\x1b[#}B",
	);
	let underline = |column: usize| t.extras.underline_color(t.grid[0][column].extra);
	assert_eq!((underline(0), underline(1)), (None, Some(0x010203)));
	assert_eq!(t.hyperlink_uri(&t.grid[0][1]), Some("https://x"));
}
