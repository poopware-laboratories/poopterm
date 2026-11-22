//! Tests of quick select while it is shown: starting it, the keys that
//! choose, copy, open and leave, the hints left as letters are typed, and
//! where their labels are drawn.
use super::{
	hint_mode::{HintMode, Outcome, chip, open_action},
	hints::{Hint, Target},
	links::LinkAction,
};

fn copy(text: &str) -> Outcome {
	Outcome::Use(LinkAction::CopyPath(text.into()))
}
use winit::keyboard::{Key, NamedKey};

fn rows(lines: &[&str]) -> Vec<Vec<(char, usize)>> {
	lines
		.iter()
		.map(|line| line.chars().zip(0..).collect())
		.collect()
}

fn letter(text: &str) -> Key {
	Key::Character(text.into())
}

/// Two targets: `/bb` at the bottom is `a`, `/aa` above it `s`.
fn two() -> HintMode {
	HintMode::new(&rows(&["open /aa", "or /bb"])).unwrap()
}

/// Thirty targets, so every label has two letters.
fn thirty() -> HintMode {
	let lines: Vec<String> = (0..30).map(|i| format!("/t{i}")).collect();
	let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
	HintMode::new(&rows(&lines)).unwrap()
}

#[test]
fn a_screen_with_nothing_to_select_does_not_start() {
	assert_eq!(HintMode::new(&rows(&["just words", ""])), None);
	assert_eq!(HintMode::new(&[]), None);
}

#[test]
fn a_new_mode_shows_every_hint_with_nothing_typed() {
	let mode = two();
	assert_eq!(mode.typed, "");
	let shown: Vec<(&str, &str)> = (mode.shown().into_iter())
		.map(|(hint, rest)| (hint.target.text.as_str(), rest))
		.collect();
	assert_eq!(shown, [("/aa", "s"), ("/bb", "a")]);
}

#[test]
fn a_lower_case_label_copies_and_a_capital_opens() {
	assert_eq!(two().press(&letter("a")), copy("/bb"));
	assert_eq!(two().press(&letter("s")), copy("/aa"));
	assert_eq!(two().press(&letter("S")), Outcome::Use(open_action("/aa")));
}

#[test]
fn two_letter_labels_take_two_presses_and_the_last_decides() {
	let mut mode = thirty();
	assert_eq!(mode.press(&letter("a")), Outcome::Stay);
	assert_eq!(mode.typed, "a");
	assert_eq!(mode.shown().len(), 26, "every label starting with a");
	assert!(mode.shown().iter().all(|(_, rest)| rest.len() == 1));
	assert_eq!(mode.press(&letter("S")), Outcome::Use(open_action("/t28")));
	let mut mode = thirty();
	mode.press(&letter("A"));
	assert_eq!(
		mode.press(&letter("a")),
		copy("/t29"),
		"only the last letter's case counts"
	);
}

#[test]
fn letters_no_label_goes_on_with_are_ignored() {
	let mut mode = thirty();
	assert_eq!(mode.press(&letter("z")), Outcome::Stay);
	assert_eq!(mode.typed, "", "no label starts with z");
	mode.press(&letter("s"));
	assert_eq!(mode.press(&letter("z")), Outcome::Stay);
	assert_eq!(mode.typed, "s", "nor with sz");
	assert_eq!(mode.shown().len(), 4);
}

#[test]
fn backspace_takes_back_a_letter_and_escape_leaves() {
	let mut mode = thirty();
	mode.press(&letter("s"));
	assert_eq!(mode.press(&Key::Named(NamedKey::Backspace)), Outcome::Stay);
	assert_eq!((mode.typed.as_str(), mode.shown().len()), ("", 30));
	assert_eq!(
		mode.press(&Key::Named(NamedKey::Backspace)),
		Outcome::Stay,
		"nothing to take back"
	);
	assert_eq!(mode.press(&Key::Named(NamedKey::Escape)), Outcome::Exit);
}

#[test]
fn other_keys_change_nothing() {
	let mut mode = thirty();
	mode.press(&letter("s"));
	for key in [
		letter("1"),
		letter("é"),
		letter("ab"),
		letter(";"),
		Key::Named(NamedKey::Enter),
		Key::Named(NamedKey::ArrowUp),
		Key::Named(NamedKey::Space),
	] {
		assert_eq!(mode.press(&key), Outcome::Stay, "{key:?}");
		assert_eq!(mode.typed, "s", "{key:?}");
	}
}

fn hint(row: usize, cols: (usize, usize)) -> Hint {
	let text = "x".repeat(cols.1 - cols.0 + 1);
	Hint {
		label: "as".into(),
		target: Target { row, cols, text },
	}
}

#[test]
fn a_label_covers_one_cell_a_letter_from_the_targets_first_cell() {
	let grid = ((10, 40), (8, 16));
	assert_eq!(chip(&hint(0, (0, 9)), 2, grid, 80), (10, 40, 16, 16));
	assert_eq!(
		chip(&hint(3, (5, 9)), 1, grid, 80),
		(10 + 40, 40 + 48, 8, 16)
	);
}

#[test]
fn a_label_near_the_right_edge_moves_left_to_stay_on_the_grid() {
	let grid = ((0, 0), (8, 16));
	// A target in the last column has room for one letter only.
	assert_eq!(chip(&hint(0, (79, 79)), 2, grid, 80), (78 * 8, 0, 16, 16));
	assert_eq!(chip(&hint(0, (78, 79)), 2, grid, 80), (78 * 8, 0, 16, 16));
	// A grid narrower than the label shows what fits, from its left.
	assert_eq!(chip(&hint(0, (0, 0)), 2, grid, 1), (0, 0, 8, 16));
	assert_eq!(
		chip(&hint(0, (0, 0)), 0, grid, 80),
		(0, 0, 8, 16),
		"at least one cell"
	);
}

#[test]
fn opening_follows_links_and_copies_what_is_none() {
	assert_eq!(
		open_action("https://a.b/c"),
		LinkAction::OpenUrl("https://a.b/c".into())
	);
	assert_eq!(
		open_action("http://10.0.0.1:80/"),
		LinkAction::OpenUrl("http://10.0.0.1:80/".into())
	);
	// Paths on the screen may be relative to a shell's directory, which
	// the app does not know, so they are copied as a click does.
	assert_eq!(
		open_action("src/app/mod.rs:3"),
		LinkAction::CopyPath("src/app/mod.rs:3".into())
	);
	// No link: hashes and addresses are copied.
	for text in ["9b9e490", "10.0.0.1", "192.168.1.1:22"] {
		assert_eq!(
			open_action(text),
			LinkAction::CopyPath(text.into()),
			"{text}"
		);
	}
	// A file:// link to a directory opens it; one to a program never runs.
	let dir = std::env::temp_dir();
	let url = format!("file://{}", dir.display());
	assert_eq!(
		open_action(&url),
		LinkAction::OpenPath(dir.canonicalize().unwrap())
	);
	assert!(matches!(
		open_action("file:///bin/sh"),
		LinkAction::CopyPath(_)
	));
}

#[test]
fn chips_follow_the_letters_left_to_type() {
	let grid = ((4, 30), (10, 20));
	let mut mode = thirty();
	let chips = mode.chips(grid, 80);
	assert_eq!(chips.len(), 30);
	// Row 29 holds `/t29`, labelled `aa`: two cells from its first column.
	assert_eq!(chips[29], ((4, 30 + 29 * 20, 20, 20), "aa"));
	assert_eq!(chips[0], ((4, 30, 20, 20), "sf"));
	mode.press(&letter("s"));
	let chips = mode.chips(grid, 80);
	assert_eq!(chips.len(), 4, "only the labels starting with s");
	assert!(
		chips
			.iter()
			.all(|((_, _, w, _), rest)| *w == 10 && rest.len() == 1)
	);
	assert_eq!(chips[0], ((4, 30, 10, 20), "f"), "the typed letter is gone");
	mode.press(&Key::Named(NamedKey::Backspace));
	assert_eq!(mode.chips(grid, 80).len(), 30);
}

#[test]
fn chips_scale_with_the_cells() {
	let mode = two();
	for scale in [1, 2, 3] {
		let cell = (8 * scale, 16 * scale);
		let chips = mode.chips(((0, 0), cell), 80);
		// `/aa` starts at column 5 of row 0; `/bb` at column 3 of row 1.
		assert_eq!(chips[0].0, (5 * cell.0, 0, cell.0, cell.1));
		assert_eq!(chips[1].0, (3 * cell.0, cell.1, cell.0, cell.1));
	}
}

/// Quick select over a terminal of `cols` by `rows` that was sent `bytes`.
fn over_terminal(cols: usize, rows: usize, bytes: &str) -> Option<HintMode> {
	use crate::{
		term::{Frame, Term},
		theme::{Theme, ThemeKind},
		ui::hints::rows_of,
	};
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	crate::parser::Parser::new().advance(&mut term, bytes.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	HintMode::new(&rows_of(&frame))
}

#[test]
fn a_git_log_on_screen_copies_its_newest_hash_first() {
	let log = "d145832 feat(pty): pixels (#365)\r\n22bbb31 feat(input): keys (#364)\r\n\
	           9b9e490 feat(fonts): colr (#363)\r\n";
	let mut mode = over_terminal(60, 6, log).unwrap();
	// The bottom one, nearest the prompt, is `a`.
	assert_eq!(mode.press(&letter("a")), copy("9b9e490"));
	let mut mode = over_terminal(60, 6, log).unwrap();
	assert_eq!(mode.press(&letter("d")), copy("d145832"));
}

#[test]
fn colors_and_wide_characters_on_screen_do_not_move_labels() {
	let line = "\x1b[1;31m漢字\x1b[0m see \x1b[4mhttps://a.b/c\x1b[0m";
	let mode = over_terminal(40, 2, line).unwrap();
	let chips = mode.chips(((0, 0), (10, 20)), 40);
	// Two wide characters take four columns, then " see ".
	assert_eq!(chips, [((90, 0, 10, 20), "a")]);
	assert_eq!(over_terminal(40, 2, "\x1b[31mred words only\x1b[0m"), None);
}

#[test]
fn an_empty_screen_or_one_past_the_cursor_has_nothing() {
	assert_eq!(over_terminal(20, 3, ""), None);
	// Erased output is not selectable.
	assert_eq!(over_terminal(20, 3, "/etc/hosts\r\x1b[2K"), None);
}

#[test]
fn chips_paint_the_accent_and_letters_only_over_their_cells() {
	use crate::{
		fonts::{BUNDLED, Font},
		render::{glyphs::FontSet, painter::Painter, scene::paint},
	};
	let mode = two();
	let (width, height, accent, text) = (200, 60, 0x00ff_8800, 0x0000_0011);
	let chips = mode.chips(((0, 0), (10, 20)), 20);
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![0; width * height];
	let mut painter = Painter::new(&mut pixels, width, height);
	paint(&mut painter, &mut fonts, |scene, _| {
		for ((x, y, w, h), letters) in &chips {
			scene.rect(*x, *y, *w, *h, accent);
			scene.mono_text(*x, *y, letters, 16.0, 14, text);
		}
	});
	let inside = |i: usize| {
		let (col, row) = (i % width, i / width);
		chips
			.iter()
			.any(|((x, y, w, h), _)| (*x..x + w).contains(&col) && (*y..y + h).contains(&row))
	};
	for (i, &pixel) in pixels.iter().enumerate() {
		assert_eq!(
			pixel != 0,
			inside(i),
			"pixel {} of row {}",
			i % width,
			i / width
		);
	}
	for ((x, y, w, h), _) in &chips {
		let chip: Vec<u32> = (*y..y + h)
			.flat_map(|r| pixels[r * width + x..r * width + x + w].to_vec())
			.collect();
		assert!(chip.contains(&accent), "the accent shows");
		assert!(
			chip.iter().any(|&p| p != accent),
			"the letter is drawn on it"
		);
	}
}

#[test]
fn a_capital_opens_a_link_that_a_small_letter_copies() {
	let mode = || HintMode::new(&rows(&["docs at https://a.b/c"])).unwrap();
	assert_eq!(mode().press(&letter("a")), copy("https://a.b/c"));
	let opened = Outcome::Use(LinkAction::OpenUrl("https://a.b/c".into()));
	assert_eq!(mode().press(&letter("A")), opened);
}

#[test]
fn whatever_keys_are_pressed_the_typed_letters_start_a_label() {
	use crate::bench::corpus::Rng;
	let keys: Vec<Key> = "asdfghjklzqXYAS1 é"
		.chars()
		.map(|c| letter(&c.to_string()))
		.chain([Key::Named(NamedKey::Backspace), Key::Named(NamedKey::Enter)])
		.collect();
	let mut rng = Rng::new(99);
	for _ in 0..300 {
		let count = 1 + rng.below(60);
		let lines: Vec<String> = (0..count).map(|i| format!("/t{i}")).collect();
		let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
		let mut mode = HintMode::new(&rows(&lines)).unwrap();
		for _ in 0..8 {
			let before = mode.typed.clone();
			let outcome = mode.press(rng.pick(&keys));
			match outcome {
				Outcome::Stay => {
					assert!(
						mode.hints.iter().any(|h| h.label.starts_with(&mode.typed)),
						"{}",
						mode.typed
					);
					assert!(!mode.shown().is_empty());
				}
				Outcome::Use(LinkAction::CopyPath(text) | LinkAction::OpenUrl(text)) => {
					let hint = mode
						.hints
						.iter()
						.find(|h| h.label == mode.typed)
						.expect("a whole label");
					assert_eq!(hint.target.text, text);
					assert!(mode.typed.starts_with(&before));
					break;
				}
				other => panic!("{other:?} from {before:?}"),
			}
		}
	}
}

#[test]
fn chips_never_overlap_or_leave_the_grid() {
	use crate::bench::corpus::Rng;
	let words = [
		"/aa",
		"~/x/y",
		"9b9e490",
		"1.2.3.4",
		"https://a.b",
		"word",
		"  ",
		"|",
	];
	let mut rng = Rng::new(5);
	for _ in 0..400 {
		let cols = 10 + rng.below(70);
		let lines: Vec<String> = (0..1 + rng.below(8))
			.map(|_| {
				let mut line = String::new();
				while line.chars().count() + 12 < cols {
					line.push_str(rng.pick(&words));
					line.push(' ');
				}
				line
			})
			.collect();
		let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
		let Some(mode) = HintMode::new(&rows(&lines)) else {
			continue;
		};
		let chips = mode.chips(((3, 7), (9, 18)), cols);
		for (i, ((x, y, w, h), letters)) in chips.iter().enumerate() {
			assert_eq!((*w, *h), (letters.len() * 9, 18));
			assert!(
				*x >= 3 && x + w <= 3 + cols * 9,
				"inside the grid: {x} {w} of {cols}"
			);
			for ((x2, y2, w2, _), _) in &chips[i + 1..] {
				let apart = y != y2 || x + w <= *x2 || x2 + w2 <= *x;
				assert!(apart, "{lines:?}");
			}
		}
	}
}

#[test]
fn what_is_shown_is_what_gets_labelled_in_history_and_on_the_alternate_screen() {
	use crate::{
		term::{Frame, Term},
		theme::{Theme, ThemeKind},
		ui::hints::rows_of,
	};
	let mut term = Term::new(20, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	let mut parser = crate::parser::Parser::new();
	parser.advance(&mut term, b"/l0\r\n/l1\r\n/l2\r\n/l3\r\n/l4\r\n/l5");
	let bottom = |term: &Term| {
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		HintMode::new(&rows_of(&frame)).unwrap().press(&letter("a"))
	};
	assert_eq!(bottom(&term), copy("/l5"), "the live screen");
	term.scroll_offset = 3;
	assert_eq!(bottom(&term), copy("/l2"), "scrolled back three lines");
	term.scroll_offset = 0;
	parser.advance(&mut term, b"\x1b[?1049h\x1b[H/vim/buffer.rs");
	assert_eq!(
		bottom(&term),
		copy("/vim/buffer.rs"),
		"a full-screen program's screen"
	);
}
