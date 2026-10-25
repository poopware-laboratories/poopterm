use super::*;
use crate::{
	parser::Parser,
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

/// The cells of `input` printed on a 12-column row.
fn row(input: &str) -> Vec<Cell> {
	let mut term = Term::new(12, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.row(0).to_vec()
}

/// Every `(first, second)` column pair of `input` that may join.
fn pairs(input: &str) -> Vec<(usize, usize)> {
	let cells = row(input);
	(0..cells.len())
		.filter_map(|col| Some((col, partner(&cells, col)?)))
		.collect()
}

/// Each cell's column and the first cell of its pair, for those joined.
fn joined(input: &str) -> Vec<(usize, usize)> {
	let cells = row(input);
	(0..cells.len())
		.filter_map(|col| Some((col, joined_to(&cells, col)?)))
		.collect()
}

#[test]
fn indicators_are_one_column_each_as_programs_count_them() {
	let cells = row("🇺🇸x");
	assert_eq!(
		cells[..3].iter().map(|cell| cell.width).collect::<Vec<_>>(),
		[1, 1, 1]
	);
	assert_eq!(cells[2].ch, 'x');
}

#[test]
fn two_indicators_make_a_pair() {
	assert_eq!(pairs("🇺🇸"), [(0, 1)]);
	assert_eq!(joined("🇺🇸"), [(1, 0)]);
}

#[test]
fn indicators_pair_up_from_the_start_of_their_run() {
	assert_eq!(pairs("🇺🇸🇯🇵"), [(0, 1), (2, 3)]);
	assert_eq!(pairs("🇺🇸🇯"), [(0, 1)], "the last one is alone");
	assert_eq!(joined("🇺🇸🇯🇵🇫"), [(1, 0), (3, 2)]);
	assert_eq!(pairs("x🇺🇸🇯🇵"), [(1, 2), (3, 4)], "a run starts after text");
	assert_eq!(pairs("🇺x🇸🇯"), [(2, 3)], "text ends a run");
}

#[test]
fn a_lone_indicator_joins_nothing() {
	assert_eq!(pairs("🇺"), []);
	assert_eq!(pairs("🇦🇿"), [(0, 1)], "the first and last indicators");
	assert_eq!(pairs("\u{1f1e5}\u{1f1e6}"), [], "the character before them");
	assert_eq!(pairs("🇺 🇸"), []);
	assert_eq!(joined("🇺 🇸"), []);
}

#[test]
fn a_skin_tone_joins_the_emoji_before_it() {
	// Both are two columns wide; the tone starts after the spacer.
	assert_eq!(pairs("👍🏽"), [(0, 2)]);
	assert_eq!(joined("👍🏽"), [(2, 0)]);
	assert_eq!(pairs("a👍🏽"), [(1, 3)]);
	for tone in ['🏻', '🏼', '🏽', '🏾', '🏿'] {
		assert_eq!(pairs(&format!("👍{tone}")), [(0, 2)], "{tone}");
	}
	assert_eq!(pairs("👍🏺"), [], "the character before the tones");
	assert_eq!(pairs("👍🐀"), [], "the character after them");
}

#[test]
fn a_skin_tone_after_text_may_join_it_too() {
	// Whether it does is the font's to say; `partner` only finds candidates.
	assert_eq!(pairs("a🏽"), [(0, 1)]);
}

#[test]
fn skin_tones_and_indicators_do_not_take_a_tone() {
	assert_eq!(pairs("🏽🏽"), []);
	assert_eq!(pairs("👍🏽🏽"), [(0, 2)], "the second tone stays alone");
	assert_eq!(joined("👍🏽🏽"), [(2, 0)]);
	assert_eq!(pairs("🇺🏽"), []);
}

#[test]
fn a_tone_after_a_zwj_sequence_cell_still_joins() {
	// The ZWJ and what follows are marks of the tone's cell.
	let cells = row("👩🏽\u{200d}🚀");
	assert_eq!(partner(&cells, 0), Some(2));
	assert_eq!(joined_to(&cells, 2), Some(0));
}

#[test]
fn nothing_joins_past_the_row() {
	let cells = row("🇺🇸");
	assert_eq!(partner(&cells[..1], 0), None);
	assert_eq!(partner(&cells, 99), None);
	assert_eq!(joined_to(&cells, 99), None);
	assert_eq!(
		joined_to(&cells, 0),
		None,
		"the first indicator has none before it"
	);
}

#[test]
fn spacers_never_start_a_pair() {
	let cells = row("👍🏽");
	assert_eq!(cells[1].width, 0);
	assert_eq!(partner(&cells, 1), None);
	assert_eq!(partner(&cells, 3), None);
}

/// Each cell's characters, without marks.
fn own(cell: &Cell, chars: &mut Vec<char>) -> bool {
	chars.push(cell.ch);
	true
}

#[test]
fn a_pair_the_font_joins_is_drawn_by_its_first_cell() {
	let cells = row("🇺🇸x");
	let flag = Join::First {
		chars: vec!['🇺', '🇸'],
		columns: 2,
	};
	assert_eq!(join(&cells, 0, own, |_| true), flag);
	assert_eq!(join(&cells, 1, own, |_| true), Join::Second);
	assert_eq!(join(&cells, 2, own, |_| true), Join::Alone);
}

#[test]
fn a_toned_emoji_covers_both_wide_cells() {
	let cells = row("👍🏽");
	let toned = Join::First {
		chars: vec!['👍', '🏽'],
		columns: 4,
	};
	assert_eq!(join(&cells, 0, own, |_| true), toned);
	assert_eq!(join(&cells, 2, own, |_| true), Join::Second);
	assert_eq!(join(&cells, 1, own, |_| true), Join::Alone, "the spacer");
}

#[test]
fn the_font_is_asked_about_both_cells_with_their_marks() {
	let cells = row("👍🏽");
	let mut asked = Vec::new();
	let marked = |cell: &Cell, chars: &mut Vec<char>| {
		chars.extend([cell.ch, '+']);
		true
	};
	join(&cells, 2, marked, |chars| {
		asked = chars.to_vec();
		false
	});
	assert_eq!(asked, ['👍', '+', '🏽', '+']);
}

#[test]
fn a_pair_the_font_does_not_join_stays_apart() {
	let cells = row("🇺🇸");
	assert_eq!(join(&cells, 0, own, |_| false), Join::Alone);
	assert_eq!(join(&cells, 1, own, |_| false), Join::Alone);
}

#[test]
fn a_cell_that_cannot_join_keeps_both_apart() {
	let cells = row("🇺🇸");
	let not_first = |cell: &Cell, chars: &mut Vec<char>| {
		chars.push(cell.ch);
		cell.ch != '🇺'
	};
	let not_second = |cell: &Cell, chars: &mut Vec<char>| {
		chars.push(cell.ch);
		cell.ch != '🇸'
	};
	for col in 0..2 {
		assert_eq!(join(&cells, col, not_first, |_| true), Join::Alone);
		assert_eq!(join(&cells, col, not_second, |_| true), Join::Alone);
	}
}

#[test]
fn the_font_is_not_asked_about_cells_without_a_partner() {
	let cells = row("ab");
	let asked = |_: &[char]| -> bool { panic!("asked") };
	assert_eq!(join(&cells, 0, own, asked), Join::Alone);
}

#[test]
fn rows_with_a_partner_may_join() {
	for input in ["🇺🇸", "x🇿", "👍🏻", "a🏿", "abc🇦"] {
		let cells = row(input);
		assert!(may_join(&cells), "{input}");
	}
}

#[test]
fn rows_without_indicators_or_tones_never_join() {
	for input in ["hello", "世界", "\u{1f1e5}\u{1f400}", "👍👍", "🐀"] {
		let cells = row(input);
		assert!(!may_join(&cells), "{input}");
		assert!(
			(0..cells.len())
				.all(|col| partner(&cells, col).is_none() && joined_to(&cells, col).is_none())
		);
	}
	assert!(!may_join(&[]));
}

#[test]
fn ordinary_cells_have_no_partner() {
	assert_eq!(pairs("hello wörld"), []);
	assert_eq!(joined("hello 世界"), []);
}

#[path = "cell_joins_draw_tests.rs"]
pub(crate) mod draw;
