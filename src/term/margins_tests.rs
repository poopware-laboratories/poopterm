//! Left and right margins (DECLRMM, DECSLRM): setting and reporting them,
//! printing and moving the cursor inside them, and scrolling the rectangle
//! they make with the top and bottom margins.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn feed(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The replies to `bytes`, in order.
fn ask(term: &mut Term, bytes: &str) -> Vec<String> {
	feed(term, bytes);
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

/// The screen's rows, blanks as spaces.
fn screen(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|y| term.grid[y].iter().map(|cell| cell.ch).collect())
		.collect()
}

/// The cursor as `(column, row)`, counted from 1 like esctest's points.
fn cursor(term: &Term) -> (usize, usize) {
	(term.cx + 1, term.cy + 1)
}

/// A 5×5 screen of the letters `a` to `y`, as esctest prepares it.
fn letters() -> Term {
	let mut t = terminal(5, 5);
	feed(&mut t, "abcde\r\nfghij\r\nklmno\r\npqrst\r\nuvwxy\x1b[H");
	t
}

#[test]
fn without_declrmm_csi_s_saves_the_cursor_and_sets_no_margins() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[2;3H\x1b[2;4s\x1b[H\x1b[u");
	assert_eq!(cursor(&t), (3, 2));
	feed(&mut t, "\x1b[Habcdefgh");
	assert_eq!(screen(&t)[0], "abcdefgh  ");
	assert_eq!(ask(&mut t, "\x1b[?69$p"), ["\x1b[?69;2$y"]);
}

#[test]
fn decslrm_sets_the_margins_reports_them_and_homes_the_cursor() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[3;3H\x1b[2;4s");
	assert_eq!(cursor(&t), (1, 1));
	assert_eq!(ask(&mut t, "\x1b[?69$p"), ["\x1b[?69;1$y"]);
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r2;4s\x1b\\"]);
}

#[test]
fn decslrm_defaults_to_the_screen_edges_and_clamps_the_right_margin() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[s");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r1;10s\x1b\\"]);
	feed(&mut t, "\x1b[3s");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r3;10s\x1b\\"]);
	feed(&mut t, "\x1b[0;4s");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r1;4s\x1b\\"]);
	feed(&mut t, "\x1b[2;65535s");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r2;10s\x1b\\"]);
}

#[test]
fn decslrm_ignores_a_left_margin_not_left_of_the_right_one() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;5s\x1b[3;3H");
	for bad in ["\x1b[4;4s", "\x1b[6;3s", "\x1b[11;12s", "\x1b[10s"] {
		feed(&mut t, bad);
		assert_eq!(
			ask(&mut t, "\x1bP$qs\x1b\\"),
			["\x1bP1$r2;5s\x1b\\"],
			"{bad:?}"
		);
		assert_eq!(cursor(&t), (3, 3), "{bad:?} moved the cursor");
	}
}

#[test]
fn text_wraps_from_the_right_margin_to_the_left_one() {
	// esctest DECSET_DECLRMM: from the first column, left of the margins.
	let mut t = terminal(6, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4sabcdefgh");
	assert_eq!(screen(&t)[..3], ["abcd  ", " efg  ", " h    "]);
	// With the mode reset the margins are gone.
	feed(&mut t, "\x1b[?69l\x1b[HABCDEFGH");
	assert_eq!(screen(&t)[..2], ["ABCDEF", "GHfg  "]);
}

#[test]
fn the_cursor_rests_on_the_right_margin_until_the_next_character() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[3;6s\x1b[1;5Hab");
	assert_eq!(cursor(&t), (6, 1));
	assert!(t.wrap_pending);
	feed(&mut t, "d");
	assert_eq!(cursor(&t), (4, 2));
	assert_eq!(screen(&t)[..2], ["    ab    ", "  d       "]);
}

#[test]
fn without_auto_wrap_text_overwrites_the_right_margin() {
	// esctest DECSET_DECAWM_OffRespectsLeftRightMargin.
	let mut t = terminal(10, 10);
	feed(&mut t, "\x1b[?69h\x1b[5;9s\x1b[5;9r\x1b[9;8H\x1b[?7labcdef");
	assert_eq!(cursor(&t), (9, 9));
	assert_eq!(screen(&t)[8], "       af ");
	assert_eq!(screen(&t)[7], "          ");
}

#[test]
fn auto_wrap_at_the_bottom_right_scrolls_only_inside_the_margins() {
	// esctest DECSET_DECAWM_OnRespectsLeftRightMargin.
	let mut t = terminal(10, 10);
	feed(
		&mut t,
		"\x1b[9;1Hzzzzzzzzzz\x1b[?69h\x1b[5;9s\x1b[5;9r\x1b[9;8Habcdef",
	);
	assert_eq!(screen(&t)[7], "    zzzab ");
	assert_eq!(screen(&t)[8], "zzzzcdef z");
	assert!(t.scrollback.is_empty());
}

#[test]
fn text_right_of_the_right_margin_runs_to_the_screen_edge() {
	let mut t = terminal(8, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[1;6Habcd");
	assert_eq!(screen(&t)[..2], ["     abc", " d      "]);
}

#[test]
fn a_wide_character_that_does_not_fit_before_the_margin_wraps() {
	let mut t = terminal(8, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;5s\x1b[1;5H世");
	assert_eq!(screen(&t)[0], "        ");
	assert_eq!(t.grid[1][1].ch, '世');
	assert_eq!(t.grid[1][1].width, 2);
	assert_eq!(cursor(&t), (4, 2));
}

#[test]
fn insert_mode_pushes_cells_off_the_right_margin() {
	// esctest SM_IRM_TruncatesAtRightMargin.
	let mut t = terminal(12, 2);
	feed(
		&mut t,
		"\x1b[1;5Habcdefg\x1b[?69h\x1b[5;10s\x1b[1;7H\x1b[4hX\x1b[?69l",
	);
	assert_eq!(screen(&t)[0], "    abXcdeg ");
}

#[test]
fn carriage_return_goes_to_the_left_margin_from_inside_it() {
	let mut t = terminal(12, 2);
	feed(&mut t, "\x1b[?69h\x1b[5;10s");
	for (from, to) in [(6, 5), (5, 5), (12, 5), (4, 1), (1, 1)] {
		feed(&mut t, &format!("\x1b[1;{from}H\r"));
		assert_eq!(cursor(&t).0, to, "from column {from}");
	}
	// In origin mode it always goes to the left margin.
	feed(&mut t, "\x1b[?6h\x1b[1;4H\r");
	assert_eq!(cursor(&t).0, 5);
}

#[test]
fn next_line_and_line_feed_mode_return_to_the_left_margin() {
	let mut t = terminal(12, 4);
	feed(&mut t, "\x1b[?69h\x1b[5;10s\x1b[1;7H\x1bE");
	assert_eq!(cursor(&t), (5, 2));
	feed(&mut t, "\x1b[20h\x1b[2;8H\n");
	assert_eq!(cursor(&t), (5, 3));
	feed(&mut t, "\x1b[3;2H\x1bE");
	assert_eq!(cursor(&t), (1, 4));
}

#[test]
fn backspace_stops_at_the_left_margin_from_inside_it() {
	let mut t = terminal(12, 4);
	feed(&mut t, "\x1b[?45l\x1b[?69h\x1b[5;10s");
	for (from, to) in [(5, 5), (6, 5), (4, 3), (1, 1)] {
		feed(&mut t, &format!("\x1b[2;{from}H\x08"));
		assert_eq!(cursor(&t), (to, 2), "from column {from}");
	}
}

#[test]
fn reverse_wrap_goes_to_the_right_margin_of_the_line_above() {
	// esctest BS_ReverseWrapWithLeftRight and
	// BS_ReversewrapFromLeftEdgeToRightMargin.
	let mut t = terminal(12, 4);
	feed(&mut t, "\x1b[?7h\x1b[?45h\x1b[?69h\x1b[5;10s\x1b[3;5H\x08");
	assert_eq!(cursor(&t), (10, 2));
	feed(&mut t, "\x1b[3;1H\x08");
	assert_eq!(cursor(&t), (10, 2));
}

#[test]
fn tabs_stop_at_the_right_margin_and_back_tabs_at_the_left_one() {
	// esctest DECSET_DECAWM_NoLineWrapOnTabWithLeftRightMargin.
	let mut t = terminal(80, 4);
	feed(&mut t, "\x1b[?69h\x1b[10;20s");
	let mut stops = Vec::new();
	for _ in 0..4 {
		feed(&mut t, "\t");
		stops.push(cursor(&t).0);
	}
	assert_eq!(stops, [9, 17, 20, 20]);
	feed(&mut t, "\x1b[Z\x1b[Z\x1b[Z");
	assert_eq!(cursor(&t).0, 10);
	// Right of the right margin, tabs go on to the screen edge.
	feed(&mut t, "\x1b[1;30H\t\t\t\t\t\t\t");
	assert_eq!(cursor(&t).0, 80);
	// Left of the left margin, back tabs go on to the screen edge.
	feed(&mut t, "\x1b[1;9H\x1b[Z\x1b[Z");
	assert_eq!(cursor(&t).0, 1);
}

#[test]
fn cursor_forward_and_back_stop_at_the_margins_from_inside_them() {
	let mut t = terminal(20, 4);
	feed(&mut t, "\x1b[?69h\x1b[5;10s");
	let cases = [
		("\x1b[3;7H\x1b[99C", 10),
		("\x1b[3;7H\x1b[99a", 10),
		("\x1b[3;12H\x1b[99C", 20),
		("\x1b[3;7H\x1b[99D", 5),
		("\x1b[3;7H\x1b[99j", 5),
		("\x1b[3;4H\x1b[99D", 1),
		("\x1b[3;12H\x1b[99D", 5),
		("\x1b[3;2H\x1b[99C", 10),
	];
	for (keys, column) in cases {
		feed(&mut t, keys);
		assert_eq!(cursor(&t), (column, 3), "{keys:?}");
	}
}

#[test]
fn origin_mode_counts_columns_from_the_left_margin() {
	// esctest CUP_RespectsOriginMode and CHA_RespectsOriginMode.
	let mut t = terminal(20, 12);
	feed(&mut t, "\x1b[6;11r\x1b[?69h\x1b[5;10s\x1b[?6h");
	assert_eq!(cursor(&t), (5, 6));
	feed(&mut t, "\x1b[1;1HX");
	assert_eq!(t.grid[5][4].ch, 'X');
	feed(&mut t, "\x1b[2;3H");
	assert_eq!(cursor(&t), (7, 7));
	assert_eq!(
		ask(&mut t, "\x1b[6n\x1b[?6n"),
		["\x1b[2;3R", "\x1b[?2;3;1R"]
	);
	feed(&mut t, "\x1b[4G");
	assert_eq!(cursor(&t), (8, 7));
	feed(&mut t, "\x1b[2`");
	assert_eq!(cursor(&t), (6, 7));
	// Columns past the right margin stop at it.
	feed(&mut t, "\x1b[1;99H");
	assert_eq!(cursor(&t), (10, 6));
	feed(&mut t, "\x1b[99G");
	assert_eq!(cursor(&t), (10, 6));
	// Leaving origin mode homes the cursor to the screen's corner.
	feed(&mut t, "\x1b[?6l");
	assert_eq!(cursor(&t), (1, 1));
}

#[test]
fn setting_the_top_and_bottom_margins_in_origin_mode_homes_to_the_left_margin() {
	let mut t = terminal(20, 12);
	feed(&mut t, "\x1b[?69h\x1b[5;10s\x1b[?6h\x1b[3;8r");
	assert_eq!(cursor(&t), (5, 3));
	feed(&mut t, "\x1b[2;6s");
	assert_eq!(cursor(&t), (2, 3));
	feed(&mut t, "\x1b[?6l\x1b[3;8r");
	assert_eq!(cursor(&t), (1, 1));
}

#[test]
fn scroll_up_moves_only_the_columns_between_the_margins() {
	// esctest SU_RespectsLeftRightScrollRegion and
	// SU_OutsideLeftRightScrollRegion: the cursor's column does not matter.
	for column in [3, 1] {
		let mut t = letters();
		feed(
			&mut t,
			&format!("\x1b[?69h\x1b[2;4s\x1b[2;{column}H\x1b[2S"),
		);
		assert_eq!(screen(&t), ["almne", "fqrsj", "kvwxo", "p   t", "u   y"]);
		assert!(t.scrollback.is_empty());
	}
}

#[test]
fn scroll_up_moves_only_the_rectangle_inside_all_four_margins() {
	// esctest SU_LeftRightAndTopBottomScrollRegion and its big scroll.
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[2;1H\x1b[2S");
	assert_eq!(screen(&t), ["abcde", "fqrsj", "k   o", "p   t", "uvwxy"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[99S");
	assert_eq!(screen(&t), ["abcde", "f   j", "k   o", "p   t", "uvwxy"]);
}

#[test]
fn scroll_down_moves_only_the_rectangle_inside_the_margins() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2T");
	assert_eq!(screen(&t), ["a   e", "f   j", "kbcdo", "pghit", "ulmny"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[2T");
	assert_eq!(screen(&t), ["abcde", "f   j", "k   o", "pghit", "uvwxy"]);
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;4r\x1b[99T");
	assert_eq!(screen(&t), ["abcde", "f   j", "k   o", "p   t", "uvwxy"]);
}

#[test]
fn scrolling_the_rectangle_keeps_its_background() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[44m\x1b[S\x1b[m");
	let blue = t.grid[4][2].bg;
	assert_ne!(blue, t.grid[4][0].bg);
	assert_eq!([t.grid[4][1].bg, t.grid[4][3].bg], [blue, blue]);
	assert_eq!(t.grid[3][2].bg, t.grid[4][0].bg);
}

#[test]
fn a_wide_character_cut_by_a_margin_loses_both_halves_when_scrolled() {
	let mut t = terminal(6, 3);
	feed(&mut t, "\x1b[2;1Ha世b\x1b[?69h\x1b[3;6s\x1b[S");
	// The left half stayed outside the rectangle; its right half moved.
	assert_eq!(t.grid[1][1].ch, ' ');
	assert_eq!(t.grid[1][1].width, 1);
	assert_eq!(t.grid[0][2].ch, ' ');
	assert_eq!(t.grid[0][2].width, 1);
	assert_eq!(t.grid[0][3].ch, 'b');
}

#[test]
fn index_at_the_bottom_margin_scrolls_only_inside_the_margins() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[5;3H\n\x1bD\x1b[5;2H\x1bE");
	assert_eq!(screen(&t), ["aqrse", "fvwxj", "k   o", "p   t", "u   y"]);
	assert_eq!(cursor(&t), (2, 5));
	assert!(t.scrollback.is_empty());
}

#[test]
fn line_feed_outside_the_margins_does_not_scroll() {
	// esctest LF_MovesDoesNotScrollOutsideLeftRight, and the same for IND,
	// NEL's line feed, VT and FF.
	for feed_line in ["\n", "\x1bD", "\x0b", "\x0c"] {
		let mut t = terminal(10, 8);
		feed(&mut t, "\x1b[2;5r\x1b[?69h\x1b[2;5s\x1b[5;3Hx");
		for (column, row, to) in [(6, 5, 5), (6, 4, 5), (6, 8, 8), (1, 5, 5), (1, 8, 8)] {
			feed(&mut t, &format!("\x1b[{row};{column}H{feed_line}"));
			assert_eq!(
				cursor(&t),
				(column, to),
				"{feed_line:?} from ({column}, {row})"
			);
			assert_eq!(
				t.grid[4][2].ch, 'x',
				"{feed_line:?} scrolled from ({column}, {row})"
			);
		}
	}
}

#[test]
fn next_line_outside_the_margins_moves_down_without_scrolling() {
	// esctest NEL_MovesDoesNotScrollOutsideLeftRight: the line feed goes
	// first, then the carriage returns.
	for next_line in ["\x1bE", "\u{85}"] {
		let mut t = terminal(10, 8);
		feed(&mut t, "\x1b[2;5r\x1b[?69h\x1b[2;5s\x1b[5;3Hx");
		for (column, row, to) in [
			(6, 5, (2, 5)),
			(6, 4, (2, 5)),
			(6, 8, (2, 8)),
			(1, 5, (1, 5)),
		] {
			feed(&mut t, &format!("\x1b[{row};{column}H{next_line}"));
			assert_eq!(cursor(&t), to, "{next_line:?} from ({column}, {row})");
			assert_eq!(
				t.grid[4][2].ch, 'x',
				"{next_line:?} scrolled from ({column}, {row})"
			);
		}
	}
}

#[test]
fn reverse_index_outside_the_margins_does_not_scroll() {
	// esctest RI_MovesDoesNotScrollOutsideLeftRight, with ESC M and C1 RI.
	for reverse in ["\x1bM", "\u{8d}"] {
		let mut t = terminal(10, 8);
		feed(&mut t, "\x1b[2;5r\x1b[?69h\x1b[2;5s\x1b[5;3Hx");
		for (column, row, to) in [(6, 2, 2), (6, 1, 1), (1, 2, 2), (1, 1, 1), (6, 3, 2)] {
			feed(&mut t, &format!("\x1b[{row};{column}H{reverse}"));
			assert_eq!(
				cursor(&t),
				(column, to),
				"{reverse:?} from ({column}, {row})"
			);
			assert_eq!(
				t.grid[4][2].ch, 'x',
				"{reverse:?} scrolled from ({column}, {row})"
			);
		}
	}
}

#[test]
fn reverse_index_at_the_top_margin_scrolls_only_inside_the_margins() {
	let mut t = letters();
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[1;2H\x1bM\x1bM");
	assert_eq!(screen(&t), ["a   e", "f   j", "kbcdo", "pghit", "ulmny"]);
}

#[test]
fn resetting_declrmm_removes_the_margins() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[?69l\x1b[?69h");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r1;10s\x1b\\"]);
}

#[test]
fn soft_reset_removes_the_margins_and_the_mode() {
	// esctest DECSET_DECLRMM_MarginsResetByDECSTR and ModeResetByDECSTR.
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[!p");
	assert_eq!(ask(&mut t, "\x1b[?69$p"), ["\x1b[?69;2$y"]);
	feed(&mut t, "\x1b[2;4s\x1b[3;3Habc");
	assert_eq!(cursor(&t), (6, 3));
	feed(&mut t, "\x1b[?69h");
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r1;10s\x1b\\"]);
}

#[test]
fn full_reset_alignment_and_resize_remove_the_margins() {
	for reset in ["\x1bc", "\x1b#8"] {
		let mut t = terminal(10, 4);
		feed(&mut t, "\x1b[?69h\x1b[2;4s");
		feed(&mut t, reset);
		feed(&mut t, "\x1b[?69h");
		assert_eq!(
			ask(&mut t, "\x1bP$qs\x1b\\"),
			["\x1bP1$r1;10s\x1b\\"],
			"{reset:?}"
		);
	}
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4s");
	t.resize(12, 4);
	assert_eq!(ask(&mut t, "\x1bP$qs\x1b\\"), ["\x1bP1$r1;12s\x1b\\"]);
	feed(&mut t, "\x1b[Habcdefghijkl");
	assert_eq!(screen(&t)[0], "abcdefghijkl");
}

#[test]
fn full_reset_turns_the_mode_off() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1bc");
	assert_eq!(ask(&mut t, "\x1b[?69$p"), ["\x1b[?69;2$y"]);
}

#[test]
fn xtsave_and_xtrestore_keep_declrmm() {
	let mut t = terminal(10, 4);
	feed(&mut t, "\x1b[?69h\x1b[?69s\x1b[?69l\x1b[?69r");
	assert_eq!(ask(&mut t, "\x1b[?69$p"), ["\x1b[?69;1$y"]);
}

#[test]
fn repeat_wraps_inside_the_margins() {
	let mut t = terminal(8, 4);
	feed(&mut t, "\x1b[?69h\x1b[3;5s\x1b[1;3Hx\x1b[5b");
	assert_eq!(screen(&t)[..2], ["  xxx   ", "  xxx   "]);
}

#[test]
fn decslrm_clears_a_pending_wrap() {
	let mut t = terminal(4, 3);
	feed(&mut t, "\x1b[?69h\x1b[1;4Hx\x1b[1;3sy");
	assert_eq!(screen(&t)[..2], ["y  x", "    "]);
}

#[test]
fn next_and_previous_line_return_to_the_left_margin() {
	// CNL and CPL return the carriage as CR does: from inside the margins
	// to the left one, from left of them to the first column.
	let mut t = terminal(12, 6);
	feed(&mut t, "\x1b[2;4r\x1b[?69h\x1b[5;10s\x1b[3;7H\x1b[E");
	assert_eq!(cursor(&t), (5, 4));
	feed(&mut t, "\x1b[3;7H\x1b[F");
	assert_eq!(cursor(&t), (5, 2));
	feed(&mut t, "\x1b[3;2H\x1b[E");
	assert_eq!(cursor(&t), (1, 4));
}

#[test]
fn text_beyond_ascii_wraps_between_the_margins() {
	let mut t = terminal(8, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[1;2Hàéîõü");
	assert_eq!(screen(&t)[..2], [" àéî    ", " õü     "]);
	// A combining mark at the right margin joins the character before it.
	feed(&mut t, "\x1b[3;2Habc\u{301}d");
	assert_eq!(screen(&t)[2..4], [" abc    ", " d      "]);
	assert_eq!(t.extras.marks(t.grid[2][3].extra), ['\u{301}']);
}

#[test]
fn a_wide_character_that_fits_before_the_margin_waits_there() {
	let mut t = terminal(8, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;5s\x1b[1;4H世");
	let (left, right) = (t.grid[0][3], t.grid[0][4]);
	assert_eq!((left.ch, left.width, right.width), ('世', 2, 0));
	assert_eq!(cursor(&t), (5, 1));
	assert!(t.wrap_pending);
}

#[test]
fn insert_mode_right_of_the_right_margin_shifts_to_the_screen_edge() {
	let mut t = terminal(10, 2);
	feed(&mut t, "\x1b[1;6Habcde\x1b[?69h\x1b[2;4s\x1b[1;7H\x1b[4hX");
	assert_eq!(screen(&t)[0], "     aXbcd");
}

#[test]
fn scrolling_the_rectangle_erases_sixel_images_inside_it_only() {
	let mut t = terminal(10, 4);
	(t.cell_width, t.cell_height) = (10, 20);
	let sixel = "\x1bPq#0;2;0;0;0~\x1b\\";
	feed(&mut t, &format!("\x1b[2;1H{sixel}\x1b[2;4H{sixel}"));
	assert_eq!(t.images.len(), 2);
	feed(&mut t, "\x1b[?69h\x1b[3;6s\x1b[S");
	let columns: Vec<usize> = t.images.iter().map(|image| image.col).collect();
	assert_eq!(columns, [0]);
}

#[test]
fn scrolling_the_rectangle_erases_scaled_text_it_cuts() {
	let mut t = terminal(10, 4);
	feed(
		&mut t,
		"\x1b[2;1H\x1b]66;s=2;ab\x07\x1b[?69h\x1b[2;6s\x1b[S",
	);
	for y in 0..4 {
		for x in 0..10 {
			assert_eq!(t.extras.multicell(t.grid[y][x].extra), None, "({x}, {y})");
		}
	}
}

#[test]
fn scrolling_the_rectangle_into_empty_rows_marks_them_used() {
	for background in ["", "\x1b[44m"] {
		let mut t = terminal(5, 5);
		feed(
			&mut t,
			&format!("\x1b[5;1Habcde\x1b[?69h\x1b[2;4s{background}\x1b[4S"),
		);
		assert_eq!(screen(&t)[0], " bcd ", "{background:?}");
		assert_eq!(screen(&t)[4], "a   e", "{background:?}");
		t.grid.assert_extents();
	}
}

#[test]
fn outside_origin_mode_cursor_positions_ignore_the_margins() {
	let mut t = terminal(12, 4);
	feed(&mut t, "\x1b[?69h\x1b[2;4s\x1b[2;9H");
	assert_eq!(cursor(&t), (9, 2));
	feed(&mut t, "\x1b[11G");
	assert_eq!(cursor(&t), (11, 2));
	assert_eq!(ask(&mut t, "\x1b[6n"), ["\x1b[2;11R"]);
	feed(&mut t, "\x1b[99`");
	assert_eq!(cursor(&t), (12, 2));
}

#[test]
fn without_auto_wrap_a_wide_character_backs_up_from_the_right_margin() {
	let mut t = terminal(8, 2);
	feed(&mut t, "\x1b[?7l\x1b[?69h\x1b[2;5s\x1b[1;5H世");
	let (left, right) = (t.grid[0][3], t.grid[0][4]);
	assert_eq!((left.ch, left.width, right.width), ('世', 2, 0));
	assert_eq!(cursor(&t), (5, 1));
	assert_eq!(screen(&t)[1], "        ");
}

#[test]
fn text_skipping_scaled_text_past_the_right_margin_runs_on_to_the_screen_edge() {
	let mut t = terminal(8, 3);
	// A block of doubled text in columns 2 and 3, rows 1 and 2.
	feed(&mut t, "\x1b[1;2H\x1b]66;s=2;a\x07\x1b[?69h\x1b[2;3s");
	// From the block's lower row the text skips it, past the right margin.
	feed(&mut t, "\x1b[2;2Hxy");
	assert_eq!((t.grid[1][3].ch, t.grid[1][4].ch), ('x', 'y'));
	assert_eq!(cursor(&t), (6, 2));
	assert!(!t.wrap_pending);
	feed(&mut t, "zzz");
	assert_eq!(screen(&t)[1].get(5..), Some("zzz"));
}
