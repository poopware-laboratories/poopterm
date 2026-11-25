//! Resizing the screen without losing what the cursor is on: a screen
//! made shorter than the cursor's row sends the lines above into
//! scrollback, and a taller one takes them back, so turning a phone (or
//! dragging a window) shorter and back leaves the screen as it was.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/tmp".into())
}

fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}

fn screen(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|y| {
			term.grid[y]
				.iter()
				.map(|c| c.ch)
				.collect::<String>()
				.trim_end()
				.to_owned()
		})
		.collect()
}

fn history(term: &Term) -> Vec<String> {
	(0..term.scrollback.len())
		.map(|y| {
			term
				.scrollback
				.get(y)
				.unwrap()
				.iter()
				.map(|c| c.ch)
				.collect::<String>()
				.trim_end()
				.to_owned()
		})
		.collect()
}

/// Ten numbered lines and a prompt on a 20×12 screen.
fn ten_lines_and_a_prompt() -> Term {
	let mut t = terminal(20, 12);
	for n in 0..10 {
		feed(&mut t, format!("line {n}\r\n").as_bytes());
	}
	feed(&mut t, b"$ ");
	t
}

#[test]
fn a_shorter_screen_keeps_the_cursors_line_and_sends_the_top_to_history() {
	let mut t = ten_lines_and_a_prompt();
	assert_eq!((t.cx, t.cy), (2, 10));
	t.resize(20, 4);
	assert_eq!(screen(&t), ["line 7", "line 8", "line 9", "$"]);
	assert_eq!(
		history(&t),
		(0..7).map(|n| format!("line {n}")).collect::<Vec<_>>()
	);
	assert_eq!((t.cx, t.cy), (2, 3), "still after the prompt");
	feed(&mut t, b"ls");
	assert_eq!(screen(&t)[3], "$ ls", "typing goes where it went before");
}

#[test]
fn a_shorter_screen_with_the_cursor_near_the_top_drops_only_blank_rows() {
	let mut t = terminal(20, 12);
	feed(&mut t, b"one\r\ntwo");
	t.resize(20, 3);
	assert_eq!(screen(&t), ["one", "two", ""]);
	assert!(history(&t).is_empty());
	assert_eq!((t.cx, t.cy), (3, 1));
}

#[test]
fn a_taller_screen_takes_lines_back_from_history() {
	let mut t = ten_lines_and_a_prompt();
	t.resize(20, 4);
	t.resize(20, 6);
	assert_eq!(
		screen(&t),
		["line 5", "line 6", "line 7", "line 8", "line 9", "$"]
	);
	assert_eq!(history(&t).len(), 5);
	assert_eq!((t.cx, t.cy), (2, 5));
}

#[test]
fn shrinking_and_growing_back_leaves_the_screen_as_it_was() {
	let mut t = ten_lines_and_a_prompt();
	let (before, cursor) = (screen(&t), (t.cx, t.cy));
	// A turn of the phone passes through sizes like these.
	for rows in [1, 2, 7, 3, 12] {
		t.resize(20, rows);
	}
	assert_eq!(screen(&t), before);
	assert_eq!((t.cx, t.cy), cursor);
	assert!(history(&t).is_empty());
}

#[test]
fn a_taller_screen_takes_back_only_what_history_holds() {
	let mut t = terminal(20, 3);
	feed(&mut t, b"a\r\nb\r\nc\r\nd");
	assert_eq!(history(&t), ["a"]);
	t.resize(20, 6);
	assert_eq!(screen(&t), ["a", "b", "c", "d", "", ""]);
	assert_eq!(t.cy, 3);
	assert!(history(&t).is_empty());
}

#[test]
fn behind_a_full_screen_program_the_shells_screen_keeps_its_lines() {
	let mut t = ten_lines_and_a_prompt();
	feed(&mut t, b"\x1b[?1049h\x1b[Hfull screen\x1b[6;1Hbottom");
	t.resize(20, 3);
	let pushed = history(&t);
	assert_eq!(pushed.len(), 8, "the shell's lines above its prompt");
	assert!(
		pushed.iter().all(|line| line.starts_with("line")),
		"never the program's rows: {pushed:?}"
	);
	t.resize(20, 12);
	assert!(history(&t).is_empty(), "taken back as the screen grew");
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(screen(&t)[10], "$", "the shell's screen is back as it was");
	assert_eq!((t.cx, t.cy), (2, 10));
}

#[test]
fn without_history_a_shorter_screen_still_keeps_the_cursors_line() {
	let mut t = ten_lines_and_a_prompt();
	t.set_scrollback_limit(0);
	t.resize(20, 2);
	assert_eq!(screen(&t), ["line 9", "$"]);
	t.resize(20, 5);
	assert_eq!(
		screen(&t),
		["line 9", "$", "", "", ""],
		"nothing to take back"
	);
	assert_eq!(t.cy, 1);
}

#[test]
fn history_positions_stay_put_through_a_resize() {
	let mut t = ten_lines_and_a_prompt();
	let absolute = |t: &Term| t.history_base + t.scrollback.len() + t.cy;
	let before = absolute(&t);
	t.resize(20, 4);
	assert_eq!(absolute(&t), before, "the prompt's line keeps its number");
	t.resize(20, 9);
	assert_eq!(absolute(&t), before);
}

#[test]
fn narrowing_and_shortening_at_once_still_keeps_the_cursors_line() {
	let mut t = ten_lines_and_a_prompt();
	t.resize(5, 3);
	assert_eq!(screen(&t), ["line", "line", "$"]);
	assert_eq!((t.cx, t.cy), (2, 2));
	assert_eq!(history(&t).len(), 8);
	assert!(
		history(&t).iter().all(|line| line.len() <= 5),
		"history is cut to the width too"
	);
}

#[test]
fn a_resize_to_the_same_height_moves_nothing() {
	let mut t = ten_lines_and_a_prompt();
	let before = (screen(&t), history(&t), t.cx, t.cy);
	t.resize(20, 12);
	t.resize(30, 12);
	assert_eq!(history(&t), before.1);
	assert_eq!((t.cx, t.cy), (before.2, before.3));
	assert_eq!(screen(&t)[10], "$");
}

#[test]
fn a_reader_scrolled_back_stays_inside_history_as_it_shrinks_and_grows() {
	let mut t = ten_lines_and_a_prompt();
	t.resize(20, 4);
	t.scroll_view(5);
	assert_eq!(t.scroll_offset, 5);
	t.resize(20, 12);
	assert!(t.scroll_offset <= t.scrollback.len(), "{}", t.scroll_offset);
	assert!(
		t.scrollback.is_empty() && t.scroll_offset == 0,
		"nothing left to scroll back to"
	);
}

#[test]
fn a_line_taken_back_keeps_its_colors_and_wide_characters() {
	let mut t = terminal(10, 6);
	feed(&mut t, "\x1b[31mred\x1b[0m 漢字\r\n".as_bytes());
	for n in 0..4 {
		feed(&mut t, format!("{n}\r\n").as_bytes());
	}
	let original: Vec<_> = t.grid[0].to_vec();
	let text = screen(&t)[0].clone();
	t.resize(10, 2);
	assert_eq!(
		history(&t)[0],
		text,
		"the line, wide characters' spacers and all"
	);
	t.resize(10, 6);
	assert_eq!(
		t.grid[0][..7],
		original[..7],
		"the same cells, colors and widths"
	);
	assert_eq!(t.grid[0][0].fg, original[0].fg);
	assert_eq!(t.grid[0][4].width, 2, "the wide character is whole");
}

/// Every line written, oldest first: history, then the screen down to the
/// cursor's row.
fn written(term: &Term) -> Vec<String> {
	let mut lines = history(term);
	lines.extend(screen(term).into_iter().take(term.cy + 1));
	lines
}

#[test]
fn any_sequence_of_heights_keeps_every_line_in_order() {
	let mut rng = crate::bench::corpus::Rng::new(41);
	for _ in 0..50 {
		let mut t = terminal(12, 1 + rng.below(15));
		for n in 0..rng.below(30) {
			feed(&mut t, format!("l{n}\r\n").as_bytes());
		}
		feed(&mut t, b"$ ");
		let lines = written(&t);
		for _ in 0..8 {
			t.resize(12, 1 + rng.below(20));
			assert_eq!(written(&t), lines, "rows {}", t.rows);
			assert_eq!(t.cx, 2, "the cursor stays after the prompt");
		}
	}
}

/// The line at the top of the view, history included.
fn top_of_view(term: &Term) -> String {
	let row = term.scrollback.len() - term.scroll_offset;
	let cells = term.row_cells(row).unwrap();
	cells
		.iter()
		.map(|c| c.ch)
		.collect::<String>()
		.trim_end()
		.to_owned()
}

#[test]
fn jumping_to_an_earlier_prompt_after_a_resize_lands_on_that_prompt() {
	let mut t = terminal(20, 12);
	for command in ["first", "second", "third"] {
		feed(
			&mut t,
			format!("\x1b]133;A\x07$ {command}\r\nout\r\n").as_bytes(),
		);
	}
	feed(&mut t, b"\x1b]133;A\x07$ ");
	t.resize(20, 3);
	assert_eq!(
		top_of_view(&t),
		"$ third",
		"the screen starts at the last command"
	);
	assert!(t.jump_to_prompt(-1));
	assert_eq!(top_of_view(&t), "$ second", "found in history");
	assert!(t.jump_to_prompt(-1));
	assert_eq!(top_of_view(&t), "$ first");
	t.scroll_to_bottom();
	t.resize(20, 12);
	assert!(history(&t).is_empty(), "every prompt is back on screen");
	let prompts: Vec<_> = screen(&t)
		.into_iter()
		.filter(|line| line.starts_with('$'))
		.collect();
	assert_eq!(prompts, ["$ first", "$ second", "$ third", "$"]);
}
