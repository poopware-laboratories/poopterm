//! Terminal-emulation tests: bytes in, grid state out.
use crate::{
	parser::{Parser, PerCharacter},
	shell_integration::{self, decode_uri_path},
	term::{Cell, Frame, Term},
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize) -> Term {
	Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/tmp".into())
}
/// Text of visible row `y`, accounting for scrollback viewing.
fn view_row(term: &Term, y: usize) -> String {
	let top = term.scrollback.len() - term.scroll_offset;
	term
		.row_cells(top + y)
		.unwrap()
		.iter()
		.map(|c| c.ch)
		.collect()
}
/// The cell at flat index `i` (`row * cols + col`).
fn cell(term: &Term, i: usize) -> Cell {
	term.grid[i / term.cols][i % term.cols]
}
fn assert_grid_shape(term: &Term) {
	assert_eq!(term.grid.iter().count(), term.rows);
	assert!(term.grid.iter().all(|row| row.len() == term.cols));
}
fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}
fn row(term: &Term, row: usize) -> String {
	term.grid[row].iter().map(|c| c.ch).collect()
}

#[test]
fn dec_private_modes_are_read_from_intermediates() {
	let mut t = terminal(20, 5);
	feed(&mut t, b"\x1b[?1;25;1002;1006;2004h");
	assert!(t.application_cursor && t.cursor_visible && t.mouse_sgr && t.bracketed_paste);
	assert_eq!(t.mouse_tracking, 1002);
	feed(&mut t, b"\x1b[?1;25;1002;1006;2004l");
	assert!(!t.application_cursor && !t.cursor_visible && !t.mouse_sgr && !t.bracketed_paste);
	assert_eq!(t.mouse_tracking, 0);
}

#[test]
fn alternate_screen_restores_shell_and_does_not_enter_history() {
	let mut t = terminal(8, 4);
	feed(&mut t, b"shell\x1b[2;3r\x1b[?6h\x1b[?1049h");
	assert!(t.primary_screen.is_some());
	assert_eq!((t.scroll_top, t.scroll_bottom), (0, 3));
	feed(&mut t, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
	assert!(t.scrollback.is_empty());
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(row(&t, 0), "shell   ");
	assert_eq!((t.scroll_top, t.scroll_bottom), (1, 2));
	assert!(t.origin_mode);
}

#[test]
fn right_margin_wrap_is_deferred_and_cancelled_by_cr_or_movement() {
	let mut t = terminal(4, 3);
	feed(&mut t, b"abcd");
	assert_eq!((t.cx, t.cy), (3, 0));
	assert!(t.wrap_pending);
	feed(&mut t, b"\x1b[31me");
	assert_eq!(row(&t, 1), "e   ");
	feed(&mut t, b"fgh\rZ");
	assert_eq!(row(&t, 1), "Zfgh");
	feed(&mut t, b"\x1b[1;4Hq\x1b[D!");
	assert_eq!(row(&t, 0), "ab!q");
}

#[test]
fn no_wrap_overwrites_last_column() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"\x1b[?7labcdef");
	assert_eq!(row(&t, 0), "abcf");
	assert_eq!(t.cy, 0);
}

#[test]
fn dec_line_drawing_supports_g0_and_g1() {
	let mut t = terminal(12, 2);
	feed(&mut t, b"\x1b(0lqqk\x1b(Btext\x1b)0\x0ex\x0fq");
	assert_eq!(row(&t, 0), "┌──┐text│q  ");
}

#[test]
fn erased_cells_keep_the_current_background() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[44m\r\x1b[K");
	assert!(
		t.grid[0][..4]
			.iter()
			.all(|cell| cell.ch == ' ' && cell.bg == t.palette[4])
	);
}

#[test]
fn erase_scrollback_does_not_erase_visible_screen() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"one\r\ntwo\r\nlast");
	assert!(!t.scrollback.is_empty());
	let visible = row(&t, 1);
	feed(&mut t, b"\x1b[3J");
	assert!(t.scrollback.is_empty());
	assert_eq!(row(&t, 1), visible);
}

#[test]
fn restricted_scroll_regions_do_not_modify_rows_outside_them() {
	let mut t = terminal(4, 4);
	feed(&mut t, b"top\x1b[4;1Hlast\x1b[2;3r\x1b[4;1H\n\x1b[L\x1b[M");
	assert_eq!(row(&t, 0), "top ");
	assert_eq!(row(&t, 3), "last");
	assert!(t.scrollback.is_empty());
}

#[test]
fn scrollback_anchor_and_limit_are_preserved() {
	let mut t = terminal(4, 2);
	t.scrollback_limit = 3;
	feed(&mut t, b"a\r\nb\r\nc");
	t.scroll_view(1);
	feed(&mut t, b"\r\nd");
	assert_eq!(t.scroll_offset, 2);
	feed(&mut t, b"\r\ne\r\nf");
	assert_eq!(t.scrollback.len(), 3);
	assert_eq!(t.scroll_offset, 3);
	t.scrollback_limit = 0;
	feed(&mut t, b"\r\ng");
	assert!(t.scrollback.is_empty());
	assert_eq!(t.scroll_offset, 0);
}

#[test]
fn cursor_queries_remain_inside_screen_after_full_width_output() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[6n");
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[1;4R");
	feed(&mut t, b"\x1b[K\x1b[@\x1b[P\x1b[X");
	assert_grid_shape(&t);
}

#[test]
fn partial_parser_chunks_and_wide_overwrite() {
	let mut t = terminal(8, 2);
	let mut parser = Parser::new();
	for byte in "\x1b[?2004h猫abc".bytes() {
		parser.advance(&mut t, &[byte]);
	}
	assert!(t.bracketed_paste);
	assert_eq!(cell(&t, 0).width, 2);
	feed(&mut t, b"\x1b[2Gx");
	assert_eq!(cell(&t, 0).ch, ' ');
	assert_eq!(cell(&t, 1).ch, 'x');
	assert_eq!(cell(&t, 1).width, 1);
}

#[test]
fn combining_marks_and_joined_emoji_stay_with_their_base_cell() {
	let mut t = terminal(16, 2);
	feed(&mut t, "e\u{301}👨‍👩‍👧‍👦".as_bytes());
	assert_eq!(cell(&t, 0).ch, 'e');
	assert_eq!(t.marks(&cell(&t, 0)), ['\u{301}']);
	assert_eq!(cell(&t, 1).ch, '👨');
	assert_eq!(cell(&t, 1).width, 2);
	assert!(t.marks(&cell(&t, 1)).contains(&'👩'));
}

#[test]
fn repeated_marks_leave_a_shared_extras_table_shared() {
	let mut t = terminal(16, 2);
	feed(&mut t, "e\u{301}".as_bytes());
	// A frame snapshot holds the table, as while the UI thread draws.
	let held = std::sync::Arc::clone(&t.extras);
	feed(&mut t, "e\u{301}".as_bytes());
	assert!(std::sync::Arc::ptr_eq(&held, &t.extras));
	assert_eq!(t.marks(&cell(&t, 1)), ['\u{301}']);
	// A new mark needs a new entry, so the table is copied first.
	feed(&mut t, "e\u{302}".as_bytes());
	assert!(!std::sync::Arc::ptr_eq(&held, &t.extras));
	assert_eq!(held.marks(cell(&t, 2).extra), []);
	assert_eq!(t.marks(&cell(&t, 2)), ['\u{302}']);
}

#[test]
fn cached_marks_keep_joined_emoji_and_hyperlinks_whole() {
	// The link is opened once: interning it is not a mark.
	let link = b"\x1b]8;;https://example.com\x1b\\";
	let cases: [(&[u8], &[u8], &[char]); 2] = [
		(b"", "👨\u{200d}👩".as_bytes(), &['\u{200d}', '👩']),
		(link, "a\u{301}".as_bytes(), &['\u{301}']),
	];
	for (prefix, text, marks) in cases {
		let mut t = terminal(16, 2);
		feed(&mut t, prefix);
		feed(&mut t, text);
		let held = std::sync::Arc::clone(&t.extras);
		feed(&mut t, text);
		assert!(std::sync::Arc::ptr_eq(&held, &t.extras), "{marks:?}");
		let (first, second) = (cell(&t, 0), cell(&t, t.cx - cell(&t, 0).width as usize));
		assert_eq!((t.marks(&second), second.extra), (marks, first.extra));
	}
}

#[test]
fn osc8_hyperlinks_are_attached_to_printed_cells_and_can_be_closed() {
	let mut t = terminal(24, 2);
	feed(
		&mut t,
		b"\x1b]8;id=docs;https://example.com/docs\x1b\\docs\x1b]8;;\x1b\\ plain",
	);
	assert_eq!(
		t.hyperlink_uri(&cell(&t, 0)),
		Some("https://example.com/docs")
	);
	assert_eq!(
		t.hyperlink_uri(&cell(&t, 3)),
		Some("https://example.com/docs")
	);
	assert_eq!(t.hyperlink_uri(&cell(&t, 5)), None);
}

/// Shells repeat the same few links on every prompt; each must reuse its
/// slot, so links keep working however often they repeat.
#[test]
fn repeated_osc8_targets_share_one_slot_and_never_exhaust_the_table() {
	let mut t = terminal(24, 2);
	for _ in 0..5_000 {
		feed(
			&mut t,
			b"\x1b]8;;https://example.com/main\x1b\\m\x1b]8;;\x1b\\\r",
		);
	}
	assert_eq!(t.hyperlinks.len(), 2);
	feed(
		&mut t,
		b"\x1b]8;;https://example.com/new\x1b\\n\x1b]8;;\x1b\\",
	);
	assert_eq!(
		t.hyperlink_uri(&cell(&t, 0)),
		Some("https://example.com/new")
	);
	// The earlier target keeps its slot when it comes back.
	feed(&mut t, b"\r\x1b]8;;https://example.com/main\x1b\\m");
	assert_eq!(
		t.hyperlink_uri(&cell(&t, 0)),
		Some("https://example.com/main")
	);
	assert_eq!(t.hyperlinks.len(), 3);
}

/// Distinct targets still stop at the bound, and the cap never lets an
/// unseen target reuse another's slot.
#[test]
fn distinct_osc8_targets_stop_at_the_bound_without_mixing_targets() {
	let mut t = terminal(24, 2);
	for i in 0..4_200 {
		feed(
			&mut t,
			format!("\x1b]8;;https://example.com/{i}\x1b\\x\x1b]8;;\x1b\\\r").as_bytes(),
		);
	}
	assert_eq!(t.hyperlinks.len(), 4096);
	feed(&mut t, b"\x1b]8;;https://example.com/7\x1b\\s");
	assert_eq!(t.hyperlink_uri(&cell(&t, 0)), Some("https://example.com/7"));
	// A target that does not fit ends the open link instead of extending it.
	feed(&mut t, b"\x1b]8;;https://example.com/unseen\x1b\\u");
	assert_eq!(t.hyperlink_uri(&cell(&t, 1)), None);
}

/// A rejected target (control characters, or too long) also ends the open
/// link: its text must not point at the previous target.
#[test]
fn a_rejected_osc8_target_ends_the_open_link() {
	let long = format!("\x1b]8;;https://example.com/{}\x1b\\b", "x".repeat(2048));
	for rejected in ["\x1b]8;;https://example.com/\u{85}\x1b\\b", long.as_str()] {
		let mut t = terminal(24, 2);
		feed(&mut t, b"\x1b]8;;https://example.com/a\x1b\\a");
		feed(&mut t, rejected.as_bytes());
		assert_eq!(t.hyperlink_uri(&cell(&t, 0)), Some("https://example.com/a"));
		assert_eq!(t.hyperlink_uri(&cell(&t, 1)), None, "{rejected:?}");
	}
}

/// Targets of up to 2048 bytes are kept; one byte more is rejected.
#[test]
fn osc8_targets_are_bounded_at_2048_bytes() {
	for (len, kept) in [(2048, true), (2049, false)] {
		let uri = format!("https://example.com/{}", "x".repeat(len - 20));
		let mut t = terminal(24, 2);
		feed(&mut t, format!("\x1b]8;;{uri}\x1b\\a").as_bytes());
		assert_eq!(
			t.hyperlink_uri(&cell(&t, 0)),
			kept.then_some(uri.as_str()),
			"{len}"
		);
	}
}

/// Reopening a link keeps the current underline color: with one set the
/// cell gets both, and once it is reset the link alone.
#[test]
fn reopened_osc8_links_keep_the_current_underline_color() {
	let mut t = terminal(24, 2);
	let link = b"\x1b]8;;https://example.com/a\x1b\\";
	feed(&mut t, link);
	feed(&mut t, b"a\x1b]8;;\x1b\\\x1b[58;2;1;2;3m");
	feed(&mut t, link);
	feed(&mut t, b"b\x1b]8;;\x1b\\\x1b[59m");
	feed(&mut t, link);
	feed(&mut t, b"c");
	let underline = |i| t.extras.underline_color(cell(&t, i).extra);
	assert_eq!(
		(underline(0), underline(1), underline(2)),
		(None, Some(0x010203), None)
	);
	for i in 0..3 {
		assert_eq!(
			t.hyperlink_uri(&cell(&t, i)),
			Some("https://example.com/a"),
			"{i}"
		);
	}
	assert_eq!(
		(cell(&t, 0).extra, cell(&t, 2).extra),
		(cell(&t, 2).extra, cell(&t, 0).extra)
	);
	assert_ne!(cell(&t, 1).extra, cell(&t, 0).extra);
}

#[test]
fn kitty_keyboard_mode_can_be_enabled_and_restored() {
	let mut t = terminal(12, 2);
	feed(&mut t, b"\x1b[>1u");
	assert!(t.kitty_keyboard);
	feed(&mut t, b"\x1b[<u");
	assert!(!t.kitty_keyboard);
}

#[test]
fn unsupported_private_sequences_do_not_execute_normal_commands() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"abcd\x1b[?2J");
	assert_eq!(row(&t, 0), "abcd");
}

#[test]
fn uri_decoding_handles_unicode_and_rejects_malformed_paths() {
	assert_eq!(
		decode_uri_path("/tmp/%E7%8C%AB%23x%25"),
		Some("/tmp/猫#x%".into())
	);
	for path in [
		"relative", "/bad%", "/bad%gg", "/bad%00", "/bad%0a", "/bad%ff",
	] {
		assert!(decode_uri_path(path).is_none(), "{path}");
	}
}

#[test]
fn randomized_control_streams_keep_grid_and_cursor_in_bounds() {
	let mut t = terminal(12, 6);
	let mut parser = Parser::new();
	let mut state = 123456789u32;
	let sequences: &[&[u8]] = &[
		b"text",
		"猫".as_bytes(),
		b"\r\n",
		b"\x08",
		b"\t",
		b"\x1b[999A",
		b"\x1b[999B",
		b"\x1b[999C",
		b"\x1b[999D",
		b"\x1b[999P",
		b"\x1b[999@",
		b"\x1b[999X",
		b"\x1b[999L",
		b"\x1b[999M",
		b"\x1b[2;4r",
		b"\x1b[r",
		b"\x1b[?6h",
		b"\x1b[?6l",
		b"\x1b[?1049h",
		b"\x1b[?1049l",
		b"\x1b[2J",
		b"\x1b[1K",
	];
	for index in 0..10_000 {
		state = state.wrapping_mul(1664525).wrapping_add(1013904223);
		parser.advance(&mut t, sequences[state as usize % sequences.len()]);
		if index % 37 == 0 {
			t.resize((state as usize % 20) + 1, (state as usize % 8) + 1);
		}
		assert_grid_shape(&t);
		assert!(t.cx < t.cols && t.cy < t.rows);
		assert!(t.scroll_top <= t.scroll_bottom && t.scroll_bottom < t.rows);
	}
}

#[test]
fn programmable_tabs_and_back_tabs() {
	let mut t = terminal(24, 2);
	feed(&mut t, b"\t");
	assert_eq!(t.cx, 8);
	feed(&mut t, b"\x1b[3g\r\x1b[5G\x1bH\r\t");
	assert_eq!(t.cx, 4);
	feed(&mut t, b"\x1b[I");
	assert_eq!(t.cx, 23);
	feed(&mut t, b"\x1b[Z");
	assert_eq!(t.cx, 4);
}

#[test]
fn erase_and_resize_do_not_leave_half_wide_characters() {
	let mut t = terminal(4, 2);
	feed(&mut t, "猫猫".as_bytes());
	feed(&mut t, b"\x1b[2G\x1b[X");
	assert_eq!(cell(&t, 0).ch, ' ');
	assert_eq!(cell(&t, 1).width, 1);
	t.resize(3, 2);
	assert_eq!(cell(&t, 2).ch, ' ');
	assert_eq!(cell(&t, 2).width, 1);
}

#[test]
fn truecolor_colon_and_semicolon_forms_match() {
	let mut t = terminal(10, 2);
	feed(&mut t, b"\x1b[38;2;12;34;56mA\x1b[38:2::12:34:56mB\x1b[38:2:12:34:56mC\x1b[48:2::9:8:7mD\x1b[38:5:196mE");
	assert!(t.grid[0][..3].iter().all(|c| c.fg == 0x0c2238));
	assert_eq!(cell(&t, 3).bg, 0x090807);
	assert_eq!(cell(&t, 4).fg, 0xff0000);
}

#[test]
fn stateful_vte_parser_handles_fragmented_controls_and_invalid_utf8() {
	let mut t = terminal(16, 2);
	let mut parser = Parser::new();
	for byte in b"before\x1b]2;fragmented title\x1b\\\xf0\x9f\x90\x88\xffX" {
		parser.advance(&mut t, &[*byte]);
	}
	assert_eq!(t.title, "fragmented title");
	assert_eq!(cell(&t, 6).ch, '🐈');
	assert_eq!(cell(&t, 8).ch, '�');
	assert_eq!(cell(&t, 9).ch, 'X');
}

#[test]
fn selection_color_queries_follow_the_theme() {
	let queries = b"\x1b]17;?\x07\x1b]19;?\x07";
	let mut dark = terminal(8, 2);
	feed(&mut dark, queries);
	assert_eq!(dark.responses[0], b"\x1b]17;rgb:89/b4/fa\x07");
	assert_eq!(dark.responses[1], b"\x1b]19;rgb:1e/1e/2e\x07");
	let mut light = Term::new(
		8,
		2,
		Theme::from_kind(ThemeKind::CatppuccinLatte),
		"/".into(),
	);
	feed(&mut light, queries);
	assert_eq!(light.responses[0], b"\x1b]17;rgb:1e/66/f5\x07");
	assert_eq!(light.responses[1], b"\x1b]19;rgb:ff/ff/ff\x07");
	// A program's own selection colors still win.
	feed(&mut light, b"\x1b]17;#102030\x07\x1b]17;?\x07");
	assert_eq!(light.responses[2], b"\x1b]17;rgb:10/20/30\x07");
}

#[test]
fn osc_palette_and_default_color_queries_are_answered() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b]4;1;#123456\x07\x1b[31mA");
	assert_eq!(cell(&t, 0).fg, 0x123456);
	feed(&mut t, b"\x1b]4;1;?\x1b\\\x1b]10;?\x07");
	assert_eq!(t.responses[0], b"\x1b]4;1;rgb:12/34/56\x1b\\");
	assert_eq!(
		t.responses[1],
		format!(
			"\x1b]10;rgb:{:02x}/{:02x}/{:02x}\x07",
			t.default_fg >> 16,
			(t.default_fg >> 8) & 0xff,
			t.default_fg & 0xff
		)
		.as_bytes()
	);
	feed(&mut t, b"\x1b]11;#010203\x07");
	assert_eq!(t.default_bg, 0x010203);
}

#[test]
fn osc_palette_supports_extended_slots_and_reset() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b]4;196;rgb:12/34/56\x07\x1b[38;5;196mX");
	assert_eq!(cell(&t, 0).fg, 0x123456);
	feed(&mut t, b"\x1b]104;196\x07\x1b[38;5;196mY");
	assert_ne!(cell(&t, 1).fg, 0x123456);
}

#[test]
fn sgr_extended_styles_clear_independently() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b[4:2;5;53mA\x1b[24;25;55mB");
	assert_eq!(cell(&t, 0).attrs & (4 | 32 | 64 | 128), 4 | 32 | 64 | 128);
	assert_eq!(cell(&t, 1).attrs & (4 | 32 | 64 | 128), 0);
}

#[test]
fn underline_color_arguments_are_not_read_as_styles() {
	let mut t = terminal(8, 2);
	// 58;2;r;g;b and 58;5;n must consume their arguments, not apply
	// 2 (dim), 1 (bold), 4 (underline) or 7 (reverse); 59 resets.
	feed(
		&mut t,
		b"\x1b[58;2;1;4;7mA\x1b[58;5;7mB\x1b[58:2::1:4:7;59mC",
	);
	for i in 0..3 {
		assert_eq!(cell(&t, i).attrs, 0, "cell {i}");
		assert_eq!(cell(&t, i).fg, t.default_fg, "cell {i}");
		assert_eq!(cell(&t, i).bg, t.default_bg, "cell {i}");
	}
}

#[test]
fn ansi_insert_mode_shifts_cells_at_the_cursor() {
	let mut t = terminal(6, 2);
	feed(&mut t, b"abcd\r\x1b[4hZ");
	assert_eq!(row(&t, 0), "Zabcd ");
	feed(&mut t, b"\x1b[4l\rY");
	assert_eq!(row(&t, 0), "Yabcd ");
}

#[test]
fn text_styles_are_set_and_reset_independently() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"\x1b[2;3;4;9mA\x1b[22;23;24;29mB\x1b[8mC\x1b[0mD");
	assert_eq!(cell(&t, 0).attrs, 1 | 2 | 4 | 16);
	assert_eq!(cell(&t, 1).attrs, 0);
	assert_eq!(cell(&t, 2).fg, cell(&t, 2).bg);
	assert_eq!(cell(&t, 3).attrs, 0);
}

#[test]
fn synchronized_output_is_committed_atomically_with_protocol_replies() {
	let mut t = terminal(8, 3);
	feed(
		&mut t,
		b"before\r\x1b[?2026h\x1b[2Jafter\x1b[?2026$p\x1b[6n",
	);
	// Updates apply at once, but frames show the screen as it was until
	// the block ends.
	let shown = |t: &Term| {
		let mut frame = Frame::default();
		t.snapshot(&mut frame);
		frame.row(0).iter().map(|cell| cell.ch).collect::<String>()
	};
	assert_eq!(row(&t, 0), "after   ");
	assert_eq!(shown(&t), "before  ");
	assert!(t.synchronized.is_some());
	assert_eq!(
		t.responses,
		vec![b"\x1b[?2026;1$y".to_vec(), b"\x1b[1;6R".to_vec()]
	);
	feed(&mut t, b"\x1b[?2026l");
	assert_eq!(shown(&t), "after   ");
	assert!(t.synchronized.is_none());
	feed(&mut t, b"\x1b[?2026$p");
	assert_eq!(t.responses.last().unwrap(), b"\x1b[?2026;2$y");
}

#[test]
fn each_synchronized_block_freezes_the_screen_it_began_with() {
	let mut t = terminal(8, 3);
	let shown = |t: &Term| {
		let mut frame = Frame::default();
		t.snapshot(&mut frame);
		frame.row(0).iter().map(|cell| cell.ch).collect::<String>()
	};
	feed(&mut t, b"one\x1b[?2026h\rtwo\x1b[?2026l");
	assert!(
		t.spare_frame.is_some(),
		"the frozen frame is kept for the next block"
	);
	feed(&mut t, b"\x1b[?2026h\rsix");
	assert_eq!(shown(&t), "two     ");
	feed(&mut t, b"\x1b[?2026l");
	assert_eq!(shown(&t), "six     ");
}

#[test]
fn synchronization_timeout_recovers_and_duplicate_begin_is_not_nested() {
	let mut t = terminal(8, 3);
	feed(&mut t, b"\x1b[?2026hA\x1b[?2026hB");
	let deadline = t.sync_deadline.unwrap();
	assert!(!t.finish_sync_if_expired(deadline - std::time::Duration::from_millis(1)));
	assert!(t.finish_sync_if_expired(deadline));
	assert_eq!(row(&t, 0), "AB      ");
	assert!(t.synchronized.is_none());
}

#[test]
fn semantic_markers_record_boundaries_and_exit_status() {
	let mut t = terminal(40, 3);
	feed(
		&mut t,
		b"\x1b]133;A\x07prompt> \x1b]133;B\x1b\\false\x1b]133;C\x07\r\noutput\x1b]133;D;1\x07",
	);
	assert_eq!(t.shell.phase, shell_integration::Phase::Finished);
	assert_eq!(t.shell.last_exit_code, Some(1));
	assert_eq!(t.shell.markers.len(), 4);
	assert_eq!(t.shell.markers[1].col, 8);
	assert_eq!(t.shell.markers[2].col, 13);
	feed(&mut t, b"\x1b]133;Z\x07");
	assert_eq!(t.shell.markers.len(), 4);
	feed(&mut t, b"\x1b]133;D;invalid\x07");
	assert_eq!(t.shell.last_exit_code, None);
	for _ in 0..300 {
		feed(&mut t, b"\x1b]133;A\x07");
	}
	assert_eq!(t.shell.markers.len(), 256);
}

#[test]
fn sixel_dcs_is_incremental_and_cursor_advances_at_termination() {
	let mut t = terminal(20, 10);
	t.cell_height = 6;
	let mut parser = Parser::new();
	for byte in b"\x1bP7;1q#1;2;100;0;0!12~" {
		parser.advance(&mut t, &[*byte]);
	}
	assert!(t.images.is_empty());
	assert_eq!(t.cy, 0);
	parser.advance(&mut t, b"\x1b\\");
	assert_eq!(t.images.len(), 1);
	assert_eq!(t.images[0].bitmap.width, 12);
	assert_eq!(t.images[0].bitmap.pixels[0], 0xffff0000);
	assert_eq!((t.cx, t.cy), (0, 1));
	feed(&mut t, b"\x1b[2J");
	assert!(t.images.is_empty());
}

#[test]
fn sixel_obeys_sync_and_alternate_screen_boundaries() {
	let mut t = terminal(20, 10);
	feed(&mut t, b"\x1b[?2026h\x1bP7;1q~\x1b\\");
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert!(frame.images.is_empty());
	feed(&mut t, b"\x1b[?2026l");
	t.snapshot(&mut frame);
	assert_eq!(frame.images.len(), 1);
	feed(&mut t, b"\x1b[?1049h");
	assert!(t.images.is_empty());
	feed(&mut t, b"\x1b[?1049l");
	assert_eq!(t.images.len(), 1);
}

#[test]
fn text_on_screen_leaves_sixel_images_in_the_scrollback_alone() {
	let mut t = terminal(20, 3);
	t.cell_height = 6;
	// A one-row image, scrolled three rows up into history.
	feed(&mut t, b"\x1bP7;1q~\x1b\\\r\n\r\n\r\n\r\n");
	assert_eq!(t.images[0].row, -3);
	feed(&mut t, b"\x1b[Hx\x1b[2;1Hy");
	assert_eq!(t.images.len(), 1);
}

#[test]
fn saved_cursor_restores_styles_and_coordinates() {
	let mut t = terminal(8, 3);
	feed(&mut t, b"\x1b[31;4mAB\x1b7\x1b[0m\x1b[3;8H\x1b8C");
	assert_eq!(cell(&t, 2).ch, 'C');
	assert_eq!(cell(&t, 2).fg, t.palette[1]);
	assert_eq!(cell(&t, 2).attrs, 4);
	assert_eq!((t.cx, t.cy), (3, 0));
}

#[test]
fn complete_sixel_color_chart_survives_the_following_prompt() {
	let mut t = terminal(100, 30);
	t.cell_height = 33;
	feed(&mut t, b"\x1bP7;1q\"1;1;180;36#1;2;100;20;20!60~#2;2;20;100;20!60~#3;2;20;20;100!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~-#1!60~#2!60~#3!60~\x1b\\\x1b]133;D;7\x07prompt> ");
	assert_eq!(t.images.len(), 1);
	assert_eq!(t.images[0].bitmap.width, 180);
	assert_eq!(t.images[0].bitmap.height, 36);
	assert_eq!(t.cy, 2);
}

#[test]
fn osc_directory_and_bell_are_tracked() {
	let mut term = Term::new(20, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
	Parser::new().advance(&mut term, b"\x1b]7;file://localhost/tmp/my%20dir\x07\x07");
	assert_eq!(term.cwd, std::path::PathBuf::from("/tmp/my dir"));
	assert!(term.bell);
}

#[test]
fn lowering_the_scrollback_limit_discards_oldest_lines_and_clamps_the_view() {
	let mut t = terminal(4, 1);
	feed(&mut t, b"a\r\nb\r\nc\r\nd");
	t.scroll_view(3);
	t.set_scrollback_limit(1);
	assert_eq!(t.scrollback.len(), 1);
	assert_eq!(t.scrollback[0][0].ch, 'c');
	assert_eq!(t.history_base, 2);
	assert_eq!(t.scroll_offset, 1);
}

#[test]
fn secondary_device_attributes_are_answered() {
	let mut t = terminal(10, 2);
	feed(&mut t, b"\x1b[>c");
	let reply = String::from_utf8(t.responses.pop().unwrap()).unwrap();
	assert!(
		reply.starts_with("\x1b[>0;") && reply.ends_with(";1c"),
		"{reply:?}"
	);
}

#[test]
fn mode_queries_report_set_and_reset_states() {
	let mut t = terminal(10, 2);
	feed(&mut t, b"\x1b[?2004h\x1b[?2004$p\x1b[?1006$p\x1b[?9999$p");
	assert_eq!(t.responses.remove(0), b"\x1b[?2004;1$y");
	assert_eq!(t.responses.remove(0), b"\x1b[?1006;2$y");
	assert_eq!(t.responses.remove(0), b"\x1b[?9999;0$y");
}

#[test]
fn cells_stay_compact() {
	assert_eq!(std::mem::size_of::<Cell>(), 16);
	// One aligned vector store or compare per cell.
	assert_eq!(std::mem::align_of::<Cell>(), 16);
}

#[test]
fn filling_cells_sets_every_field_of_every_cell_in_range() {
	let mut cells = vec![Cell::default(); 5];
	let styled = Cell {
		ch: 'q',
		fg: 0x0102_0304,
		bg: 0x0506_0708,
		attrs: 9,
		width: 2,
		extra: 0x0a0b,
	};
	Cell::fill(&mut cells[1..4], styled);
	assert_eq!(
		cells,
		[Cell::default(), styled, styled, styled, Cell::default()]
	);
	Cell::fill(&mut [], styled);
}

#[test]
fn cell_bits_compare_like_cells() {
	let base = Cell::default();
	let variants = [
		Cell { ch: 'x', ..base },
		Cell { fg: 1, ..base },
		Cell { bg: 1, ..base },
		Cell { attrs: 1, ..base },
		Cell { width: 2, ..base },
		Cell { extra: 1, ..base },
	];
	assert_eq!(base.bits(), Cell::default().bits());
	for variant in variants {
		assert_ne!(variant.bits(), base.bits(), "{variant:?}");
		assert_eq!(variant.bits(), { variant }.bits());
	}
}

#[test]
fn one_segment_of_mixed_widths_stores_each_width_in_the_style() {
	let mut t = terminal(12, 2);
	feed(&mut t, "\x1b[1;4;31ma猫b🙂é日".as_bytes());
	let style = t.grid[0][0];
	let expected = [
		('a', 1),
		('猫', 2),
		('猫', 0),
		('b', 1),
		('🙂', 2),
		('🙂', 0),
		('é', 1),
		('日', 2),
		('日', 0),
	];
	for (x, (ch, width)) in expected.into_iter().enumerate() {
		let cell = t.grid[0][x];
		// Right halves of wide characters are spacers without a character.
		let ch = if width == 0 { Cell::default().ch } else { ch };
		assert_eq!(cell, Cell { ch, width, ..style }, "column {x}");
	}
	assert_ne!(style.fg, terminal(12, 2).grid[0][0].fg, "styled");
	assert_eq!(
		t.grid[0][9],
		terminal(12, 2).grid[0][9],
		"nothing past the text"
	);
	assert_eq!((t.cx, t.last_printed), (9, Some('日')));
}

/// Feeds the same random streams through the batched `Parser` and through
/// vte driving `Term` directly (one `print` per character), and requires
/// identical terminal state. Guards every print fast path.
#[test]
fn batched_printing_matches_per_character_printing() {
	let fragments: &[&[u8]] = &[
		b"hello world ",
		"🙂猫e\u{301}日本語".as_bytes(),
		b"\xff\xe7x\xab",
		b"\xc0\x80\xed\xa0\x80",
		b"\xf4\x90\x80\x80",
		b"\x1b[9G\xe7\x8c\xab",
		b"\x1b[38;2;1;2;3m",
		b"\x1b[1;2;3;4;5;6m",
		b"\x1b[0;0H\x1b[65535A\x1b[0D",
		b"\x1b[?6h\x1b[2;2H\x1b[?6l",
		b"x",
		b"\r\n",
		b"\n",
		b"\t",
		b"\x08",
		b"\x1b[31m",
		b"\x1b[1;7m",
		b"\x1b[0m",
		b"\x1b[2D",
		b"\x1b[5G",
		b"\x1b[K",
		b"\x1b[2;3H",
		b"\x1b[?7l",
		b"\x1b[?7h",
		b"\x1b[4h",
		b"\x1b[4l",
		b"\x1b(0lqk\x1b(B",
		"猫".as_bytes(),
		"e\u{301}".as_bytes(),
		"👨\u{200d}👩".as_bytes(),
		"\u{1100}\u{1161}\u{11a8}\u{115f}\u{1160}".as_bytes(),
		"a\u{ad}\u{d4e}\u{600}1\u{1f1e9}\u{1f1ea}\u{2764}".as_bytes(),
		"\u{20000}\u{3fffd}\u{ff21}".as_bytes(),
		b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
		b"\x7f",
		b"\x1b[3;5r",
		b"\x1b[r",
		b"\x1b[1\x18x",
		b"\x1b]0;title\x1a",
		b"\x1b[1<2mz",
		b"\x1b]2;bell title\x07",
		b"\x1b]2;st title\x1b\\",
		b"\x1bPq#0;2;0;0;0~-\x1b\\",
		"\u{9b}".as_bytes(),
		"日本\x1b[32m語".as_bytes(),
		b"abc\x1b",
		b"[0m",
		b"\x1b7\x1b8",
		b"\x00\x05",
		// Erases and shifts in a background other than the padding's.
		b"\x1b[44m",
		b"\x1b[1K",
		b"\x1b[2K",
		b"\x1b[J",
		b"\x1b[2@",
		b"\x1b[P",
		b"\x1b[3X",
		b"\x1b[S\x1b[T",
		b"\x1b[L\x1b[M",
		b"\x1b]11;#102030\x1b\\",
		b"\x1b[?1049h",
		b"\x1b[?1049l",
	];
	let mut seed = 0x9e37_79b9_u32;
	for _ in 0..200 {
		let mut stream = Vec::new();
		for _ in 0..60 {
			seed ^= seed << 13;
			seed ^= seed >> 17;
			seed ^= seed << 5;
			stream.extend_from_slice(fragments[seed as usize % fragments.len()]);
		}
		let mut batched = terminal(9, 5);
		batched.scrollback_limit = 20;
		let mut reference = batched.clone();
		let mut parser = Parser::new();
		for chunk in stream.chunks(7) {
			parser.advance(&mut batched, chunk);
			batched.grid.assert_extents();
		}
		vte::Parser::new().advance(&mut PerCharacter(&mut reference), &stream);
		reference.grid.assert_extents();
		assert_eq!(
			(batched.cx, batched.cy, batched.wrap_pending),
			(reference.cx, reference.cy, reference.wrap_pending),
			"cursor after {:?}",
			String::from_utf8_lossy(&stream)
		);
		// `extra` ids depend on interning order, so compare what they resolve to.
		let resolve = |t: &Term, row: &[Cell]| -> Vec<_> {
			row
				.iter()
				.map(|c| {
					(
						c.ch,
						c.fg,
						c.bg,
						c.attrs,
						c.width,
						t.marks(c).to_vec(),
						t.hyperlink_uri(c).map(str::to_owned),
					)
				})
				.collect()
		};
		let context = String::from_utf8_lossy(&stream);
		for y in 0..batched.rows {
			assert_eq!(
				resolve(&batched, &batched.grid[y]),
				resolve(&reference, &reference.grid[y]),
				"row {y} after {context:?}"
			);
		}
		assert_eq!(batched.scrollback.len(), reference.scrollback.len());
		for (a, b) in batched.scrollback.iter().zip(reference.scrollback.iter()) {
			assert_eq!(
				resolve(&batched, a),
				resolve(&reference, b),
				"history after {context:?}"
			);
		}
	}
}

#[test]
fn controls_after_a_utf8_character_split_across_reads_are_not_dropped() {
	// vte 0.15 swallows bytes copied after a split character; see Parser::advance.
	let mut t = terminal(8, 2);
	let mut parser = Parser::new();
	parser.advance(&mut t, b"e\xcc");
	// The lookahead vte copies (`81 08 e7`) ends inside 猫, which triggers the bug.
	parser.advance(&mut t, b"\x81\x08\xe7\x8c\xab");
	assert_eq!(row(&t, 0), "猫       ");
}

#[test]
fn a_character_split_over_several_reads_is_reassembled() {
	let mut t = terminal(6, 1);
	let mut parser = Parser::new();
	for byte in "🙂X".bytes() {
		parser.advance(&mut t, &[byte]);
	}
	assert_eq!(row(&t, 0), "🙂 X   ");
	assert_eq!(cell(&t, 0).width, 2);
}

#[test]
fn rep_repeats_the_previous_graphic_character() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"ab\x1b[3b");
	assert_eq!(row(&t, 0), "abbbb   ");
	feed(&mut t, "\r\n猫\x1b[2b".as_bytes());
	assert_eq!(row(&t, 1), "猫 猫 猫   ");
}

#[test]
fn cursor_shape_follows_decscusr() {
	use crate::term::CursorShape;
	let mut t = terminal(4, 2);
	assert_eq!(t.cursor_shape, CursorShape::Block);
	feed(&mut t, b"\x1b[6 q");
	assert_eq!(t.cursor_shape, CursorShape::Bar);
	feed(&mut t, b"\x1b[4 q");
	assert_eq!(t.cursor_shape, CursorShape::Underline);
	feed(&mut t, b"\x1b[0 q");
	assert_eq!(t.cursor_shape, CursorShape::Block);
}

#[test]
fn focus_reporting_mode_is_tracked_and_queryable() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"\x1b[?1004h\x1b[?1004$p");
	assert!(t.focus_reporting);
	assert_eq!(t.responses.pop().unwrap(), b"\x1b[?1004;1$y");
	feed(&mut t, b"\x1b[?1004l");
	assert!(!t.focus_reporting);
}

#[test]
fn xtversion_reports_name_and_version() {
	let mut t = terminal(4, 2);
	feed(&mut t, b"\x1b[>q");
	let reply = String::from_utf8(t.responses.pop().unwrap()).unwrap();
	assert_eq!(
		reply,
		format!("\x1bP>|poopterm {}\x1b\\", env!("CARGO_PKG_VERSION"))
	);
}

#[test]
fn soft_reset_restores_modes_but_keeps_the_screen() {
	let mut t = terminal(6, 3);
	feed(
		&mut t,
		b"text\x1b[?25l\x1b[4h\x1b[?6h\x1b[?7l\x1b[?1h\x1b[2;3r\x1b[1;31m\x1b(0",
	);
	feed(&mut t, b"\x1b[!p");
	assert!(t.cursor_visible && t.auto_wrap);
	assert!(!t.insert_mode && !t.origin_mode && !t.application_cursor);
	assert_eq!((t.scroll_top, t.scroll_bottom), (0, 2));
	assert_eq!((t.fg, t.attrs, t.bold), (t.default_fg, 0, false));
	assert!(!t.charsets[0]);
	assert_eq!(row(&t, 0), "text  ");
}

#[test]
fn last_command_output_is_extracted_across_scrollback() {
	let mut t = terminal(10, 3);
	feed(
		&mut t,
		b"\x1b]133;A\x07$ ls\x1b]133;B\x07\r\n\x1b]133;C\x07one\r\ntwo  \r\nthree\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ ",
	);
	assert!(
		!t.scrollback.is_empty(),
		"the output scrolled partly into history"
	);
	assert_eq!(t.last_command_output().as_deref(), Some("one\ntwo\nthree"));
}

#[test]
fn prompt_jumps_scroll_the_view_to_the_prompt() {
	let mut t = terminal(10, 2);
	feed(
		&mut t,
		b"\x1b]133;A\x07$ a\r\nout\r\nout\r\n\x1b]133;A\x07$ b\r\nout\r\nout\r\n\x1b]133;A\x07$ ",
	);
	assert!(t.jump_to_prompt(-1));
	assert_eq!(
		view_row(&t, 0).trim_end(),
		"$ b",
		"previous prompt at the top"
	);
	assert!(t.jump_to_prompt(-1));
	assert_eq!(view_row(&t, 0).trim_end(), "$ a");
	assert!(!t.jump_to_prompt(-1), "no earlier prompt");
	assert!(t.jump_to_prompt(1));
	assert_eq!(view_row(&t, 0).trim_end(), "$ b");
}

#[test]
fn utf8_and_common_csi_match_vte_at_every_read_boundary() {
	let stream = concat!(
		"héllo wörld 日本語 → λ 🙂 naïve ─┼─\r\n",
		"\x1b[1;31mabc\x1b[0m\x1b[2;3H猫\x1b[K",
		"\x1b[2A\x1b[0B\x1b[999999C\x1b[2D",
		"e\u{301}👨\u{200d}👩\r\n",
		"\x1b[38:2::1:2:3mx\x1b[0mend"
	)
	.as_bytes();
	let mut reference = terminal(9, 5);
	vte::Parser::new().advance(&mut PerCharacter(&mut reference), stream);
	let mut expected = super::Frame::default();
	reference.snapshot(&mut expected);
	for boundary in 0..=stream.len() {
		let mut actual = terminal(9, 5);
		let mut parser = Parser::new();
		parser.advance(&mut actual, &stream[..boundary]);
		parser.advance(&mut actual, &stream[boundary..]);
		let mut frame = super::Frame::default();
		actual.snapshot(&mut frame);
		assert_eq!(frame.cursor, expected.cursor, "boundary {boundary}");
		assert_eq!(actual.responses, reference.responses, "boundary {boundary}");
		for row in 0..frame.rows {
			assert!(
				frame.row_matches(row, &expected, row),
				"boundary {boundary}, row {row}"
			);
		}
	}
}

#[test]
fn erasing_either_half_of_a_wide_character_repairs_only_its_boundary() {
	for (sequence, expected) in [
		("\x1b[2G\x1b[K", "      "),
		("\x1b[1G\x1b[1K", "  xy  "),
		("\x1b[2K", "      "),
	] {
		let mut t = terminal(6, 1);
		feed(&mut t, "猫xy".as_bytes());
		feed(&mut t, sequence.as_bytes());
		assert_eq!(row(&t, 0), expected);
		assert!(t.grid[0].iter().all(|cell| cell.width == 1));
	}
}

#[test]
fn typing_scrolls_back_to_the_bottom() {
	let mut t = terminal(8, 2);
	feed(&mut t, b"1\r\n2\r\n3\r\n4");
	t.scroll_offset = 2;
	assert!(t.scroll_to_bottom());
	assert_eq!(t.scroll_offset, 0);
	assert!(!t.scroll_to_bottom(), "nothing changes at the bottom");
}
