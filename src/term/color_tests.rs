//! Color sequences: xterm OSC 4/5/6/10–19/104–119, kitty OSC 21 and the
//! color stacks, from bytes in to colors, frames and replies out.
use crate::{
	parser::Parser,
	term::{Frame, Setting, Term},
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	term
		.responses
		.drain(..)
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn replies_end_like_the_query() {
	let mut t = terminal();
	send(&mut t, "\x1b]4;1;#123456;2;?\x07\x1b]4;1;?\x1b\\");
	assert_eq!(
		replies(&mut t),
		[
			format!("\x1b]4;2;{}\x07", rgb(t.indexed_color(2))),
			"\x1b]4;1;rgb:1212/3434/5656\x1b\\".to_owned()
		]
	);
}

fn rgb(color: u32) -> String {
	crate::parser::encode_color(color)
}

#[test]
fn dynamic_colors_chain_from_the_first_number_and_reset() {
	let mut t = terminal();
	let theme_fg = t.default_fg;
	send(&mut t, "A\x1b]10;#111111;#222222;#333333\x07B");
	assert_eq!((t.default_fg, t.default_bg), (0x111111, 0x222222));
	assert_eq!(t.colors.cursor, Setting::Color(0x333333));
	// Cells drawn in the old default colors follow the new ones.
	assert_eq!((t.grid[0][0].fg, t.grid[0][1].fg), (0x111111, 0x111111));
	send(&mut t, "\x1b]17;red;?\x07");
	assert_eq!(t.colors.selection_background, Setting::Color(0xff0000));
	assert_eq!(
		replies(&mut t),
		["\x1b]18;rgb:1111/1111/1111\x07"],
		"18 follows the fg"
	);
	send(&mut t, "\x1b]19;blue\x07\x1b]12;?\x07");
	assert_eq!(t.colors.selection_foreground, Setting::Color(0xff));
	assert_eq!(replies(&mut t), ["\x1b]12;rgb:3333/3333/3333\x07"]);
	send(&mut t, "\x1b]110\x07\x1b]112\x07\x1b]117\x07\x1b]119\x07");
	assert_eq!(t.default_fg, theme_fg);
	assert_eq!(t.default_bg, 0x222222, "111 was not sent");
	assert_eq!(t.colors.cursor, Setting::Default);
	assert_eq!(t.colors.selection_background, Setting::Default);
	assert_eq!(t.colors.selection_foreground, Setting::Default);
}

#[test]
fn pointer_and_tektronix_colors_are_kept_for_queries() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]13;#010101;#020202;#030303;#040404\x07\x1b]18;#050505\x07",
	);
	send(&mut t, "\x1b]13;?;?;?;?;?;?\x07");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]13;rgb:0101/0101/0101\x07",
			"\x1b]14;rgb:0202/0202/0202\x07",
			"\x1b]15;rgb:0303/0303/0303\x07",
			"\x1b]16;rgb:0404/0404/0404\x07",
			&format!("\x1b]17;{}\x07", rgb(crate::theme::SELECTION_COLORS.1)),
			"\x1b]18;rgb:0505/0505/0505\x07",
		]
	);
	send(&mut t, "\x1b]113\x07\x1b]118\x07\x1b]13;?\x07\x1b]18;?\x07");
	let fg = rgb(t.default_fg);
	assert_eq!(
		replies(&mut t),
		[format!("\x1b]13;{fg}\x07"), format!("\x1b]18;{fg}\x07")]
	);
}

#[test]
fn special_colors_are_set_queried_reset_and_used_when_enabled() {
	let mut t = terminal();
	// OSC 4 numbers past the 256-color table address the special colors.
	send(
		&mut t,
		"\x1b]5;0;#aa0000\x07\x1b]4;258;#00bb00\x07\x1b]5;0;?;2;?\x07",
	);
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]5;0;rgb:aaaa/0000/0000\x07",
			"\x1b]5;2;rgb:0000/bbbb/0000\x07"
		]
	);
	send(&mut t, "\x1b[1mA");
	// Bold default text is brightened unless colorBDMode is on (off by
	// default).
	let bold = t.grid[0][0].fg;
	assert_ne!(bold, 0xaa0000);
	send(&mut t, "\x1b]6;0;1\x07\x1b[1mB\x1b[22mC\x1b[31;1mD");
	assert_eq!(t.grid[0][1].fg, 0xaa0000);
	assert_eq!(t.grid[0][2].fg, t.default_fg);
	assert_eq!(
		t.grid[0][3].fg,
		t.indexed_color(9),
		"SGR colors win (bold is bright)"
	);
	send(
		&mut t,
		"\x1b]106;5;1\x07\x1b[31;1mE\x1b]105;0\x07\x1b[0;1mF",
	);
	assert_eq!(
		t.grid[0][4].fg, 0xaa0000,
		"colorAttrMode overrides SGR colors"
	);
	assert_eq!(t.grid[0][5].fg, bold, "reset");
}

#[test]
fn kitty_color_keys_set_query_reset_and_report_unknown_keys() {
	let mut t = terminal();
	let theme_bg = t.default_bg;
	send(
		&mut t,
		"\x1b]21;foreground=red;background=#000080;cursor=;selection_background=green;\
		 5=rgb:1/2/3;transparent_background_color2=white@0.25\x1b\\",
	);
	assert_eq!((t.default_fg, t.default_bg), (0xff0000, 0x80));
	assert_eq!(t.colors.cursor, Setting::Dynamic);
	assert_eq!(t.indexed_color(5), 0x112233);
	assert_eq!(t.colors.transparent[1], Some((0xffffff, 0.25)));
	send(
		&mut t,
		"\x1b]21;foreground=?;cursor=?;selection_background=?;5=?;nonsense=?;\
		 transparent_background_color2=?;background;background=?\x1b\\",
	);
	assert_eq!(
		replies(&mut t),
		[format!(
			"\x1b]21;foreground=rgb:ff/00/00;cursor=;selection_background=rgb:00/ff/00;\
			 5=rgb:11/22/33;unknown=bm9uc2Vuc2U;transparent_background_color2=rgb:ff/ff/ff@0.25;\
			 background=rgb:{:02x}/{:02x}/{:02x}\x1b\\",
			theme_bg >> 16,
			theme_bg >> 8 & 0xff,
			theme_bg & 0xff
		)]
	);
	// The 256-color table cannot be dynamic; a bare key resets it.
	send(&mut t, "\x1b]21;5=\x07\x1b]21;5=?\x07");
	assert_eq!(replies(&mut t), ["\x1b]21;5=rgb:11/22/33\x07"]);
	send(&mut t, "\x1b]21;5\x07");
	assert_eq!(t.color_overrides[5], None);
}

#[test]
fn color_stacks_save_and_restore_every_color() {
	let mut t = terminal();
	let (fg, red) = (t.default_fg, t.indexed_color(1));
	send(
		&mut t,
		"\x1b]30001\x1b\\\x1b]10;#010101;#020202;#030303\x07\x1b]4;1;#040404\x07",
	);
	send(&mut t, "\x1b]30101\x1b\\");
	assert_eq!((t.default_fg, t.indexed_color(1)), (fg, red));
	assert_eq!(t.colors.cursor, Setting::Default);
	// XTPUSHCOLORS / XTPOPCOLORS / XTREPORTCOLORS, with numbered slots.
	send(&mut t, "\x1b[#P\x1b]4;1;#040404\x07\x1b[2#P\x1b[#R");
	assert_eq!(replies(&mut t), ["\x1b[2;2#Q"]);
	send(&mut t, "\x1b]4;1;#050505\x07\x1b[1#Q");
	assert_eq!(t.indexed_color(1), red, "slot 1 restored and kept");
	send(&mut t, "\x1b[#Q\x1b[#R");
	assert_eq!(t.indexed_color(1), 0x040404, "popped slot 2");
	assert_eq!(replies(&mut t), ["\x1b[1;1#Q"]);
	for _ in 0..12 {
		send(&mut t, "\x1b[#P");
	}
	assert_eq!(t.color_stack.len(), super::colors::STACK_DEPTH);
}

#[test]
fn frames_carry_cursor_and_selection_colors() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]12;#123456\x07\x1b]21;cursor_text=#654321;selection_foreground=\x07",
	);
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.cursor, Setting::Color(0x123456));
	assert_eq!(frame.colors.cursor_text, Setting::Color(0x654321));
	assert_eq!(frame.colors.selection_foreground, Setting::Dynamic);
	assert_eq!(frame.colors.selection_background, Setting::Default);
}
