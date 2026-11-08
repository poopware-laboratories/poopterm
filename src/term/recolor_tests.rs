//! Switching themes live: text already on screen, in scrollback and on the
//! saved primary screen takes the new theme's colors, while colors programs
//! set with OSC 4, 10–12, 17 and 19 stay on top of it.
use crate::{
	parser::{Parser, encode_color},
	term::{Frame, Setting, Term},
	theme::{Theme, ThemeKind},
	ui::contrast::ratio,
};

fn terminal(kind: ThemeKind) -> Term {
	Term::new(8, 3, Theme::from_kind(kind), "/".into())
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

fn fg_at(term: &Term, row: usize, col: usize) -> u32 {
	term.grid[row][col].fg
}

fn theme(kind: ThemeKind) -> Theme {
	Theme::from_kind(kind)
}

#[test]
fn text_in_a_palette_color_takes_the_new_themes_color() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[31mR\x1b[32mG\x1b[44mB\x1b[0mD");
	t.set_theme(theme(ThemeKind::Dracula));
	let dracula = theme(ThemeKind::Dracula);
	assert_eq!(fg_at(&t, 0, 0), dracula.palette[1]);
	assert_eq!(fg_at(&t, 0, 1), dracula.palette[2]);
	assert_eq!(t.grid[0][2].bg, dracula.palette[4]);
	// Default text and the blank rest of the screen follow the defaults.
	assert_eq!((fg_at(&t, 0, 3), t.grid[0][3].bg), (dracula.fg, dracula.bg));
	assert_eq!(t.grid[2][7].bg, dracula.bg);
}

#[test]
fn bright_and_bold_text_takes_the_new_themes_brights() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[91mR\x1b[0;1;34mB\x1b[0;105mM");
	t.set_theme(theme(ThemeKind::Dracula));
	assert_eq!(fg_at(&t, 0, 0), 0xFF6E6E);
	assert_eq!(fg_at(&t, 0, 1), t.indexed_color(12));
	assert_eq!(t.grid[0][2].bg, 0xFF92DF);
}

#[test]
fn scrollback_and_the_saved_primary_screen_are_recolored_too() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[31mold\x1b[0m\r\n\n\n\n");
	assert!(!t.scrollback.is_empty());
	send(&mut t, "\x1b[H\x1b[33mY\x1b[?1049h\x1b[35mM");
	t.set_theme(theme(ThemeKind::GruvboxDark));
	let gruvbox = theme(ThemeKind::GruvboxDark);
	assert_eq!(t.scrollback.get(0).unwrap()[0].fg, gruvbox.palette[1]);
	assert_eq!(fg_at(&t, 0, 0), gruvbox.palette[5]);
	send(&mut t, "\x1b[?1049l");
	assert_eq!(fg_at(&t, 0, 0), gruvbox.palette[3]);
	assert_eq!(t.grid[1][0].bg, gruvbox.bg);
}

#[test]
fn the_pen_keeps_its_palette_color_across_a_switch() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[32;41m");
	t.set_theme(theme(ThemeKind::Dracula));
	send(&mut t, "X");
	let dracula = theme(ThemeKind::Dracula);
	assert_eq!(
		(fg_at(&t, 0, 0), t.grid[0][0].bg),
		(dracula.palette[2], dracula.palette[1])
	);
}

#[test]
fn colors_outside_the_16_ansi_colors_are_left_alone() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[38;5;196mC\x1b[38;2;1;2;3mT\x1b[38;5;245mG");
	let before: Vec<u32> = (0..3).map(|col| fg_at(&t, 0, col)).collect();
	t.set_theme(theme(ThemeKind::Dracula));
	let after: Vec<u32> = (0..3).map(|col| fg_at(&t, 0, col)).collect();
	assert_eq!(before, after);
	assert_eq!(after, [0xFF0000, 0x010203, 0x8A8A8A]);
}

#[test]
fn a_256_color_equal_to_one_of_the_16_is_recolored_with_it() {
	// Cells keep RGB, not indexes: gray 244 is xterm's bright black, which
	// Nord uses, so it cannot be told from color 8.
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[38;5;244mG");
	assert_eq!(fg_at(&t, 0, 0), 0x808080);
	t.set_theme(theme(ThemeKind::Dracula));
	assert_eq!(fg_at(&t, 0, 0), 0x6272A4);
}

#[test]
fn a_color_shared_by_the_default_and_the_palette_follows_the_default() {
	// Poop's white is its foreground, so white text cannot be told from
	// default text: both take the new foreground, not the new white.
	let mut t = terminal(ThemeKind::Poop);
	send(&mut t, "D\x1b[37mW");
	assert_eq!(fg_at(&t, 0, 0), fg_at(&t, 0, 1));
	t.set_theme(theme(ThemeKind::MacOsDark));
	assert_eq!(fg_at(&t, 0, 0), 0xE6E6E6);
	assert_eq!(fg_at(&t, 0, 1), 0xE6E6E6);
	// New white text gets the theme's white.
	send(&mut t, "\x1b[37mW");
	assert_eq!(fg_at(&t, 0, 2), 0xBBBBBB);
}

#[test]
fn switching_there_and_back_restores_every_color() {
	let mut t = terminal(ThemeKind::CatppuccinMocha);
	send(&mut t, "\x1b[31mR\x1b[92mG\x1b[46mC\x1b[0mD\x1b[7mV");
	let cells = |t: &Term| {
		t.grid[0][..5]
			.iter()
			.map(|c| (c.fg, c.bg))
			.collect::<Vec<_>>()
	};
	let before = cells(&t);
	t.set_theme(theme(ThemeKind::GruvboxDark));
	assert_ne!(cells(&t), before);
	t.set_theme(theme(ThemeKind::CatppuccinMocha));
	assert_eq!(cells(&t), before);
}

#[test]
fn switching_to_the_same_theme_changes_nothing() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[31mR\x1b[38;2;9;9;9mT\x1b[0mD");
	let before = t.grid[0].clone();
	t.set_theme(theme(ThemeKind::Nord));
	assert_eq!(t.grid[0], before);
}

#[test]
fn an_osc_4_color_survives_a_theme_switch_and_its_query() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b]4;1;#123456\x07\x1b[31mR\x1b[32mG");
	t.set_theme(theme(ThemeKind::Dracula));
	assert_eq!(fg_at(&t, 0, 0), 0x123456);
	assert_eq!(fg_at(&t, 0, 1), theme(ThemeKind::Dracula).palette[2]);
	send(&mut t, "\x1b]4;1;?\x07\x1b]4;2;?\x07");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]4;1;rgb:12/34/56\x07".to_owned(),
			format!("\x1b]4;2;{}\x07", encode_color(0x50FA7B)),
		]
	);
	// Resetting it gives the new theme's red, not the old one's.
	send(&mut t, "\x1b]104;1\x07\x1b[31mR");
	assert_eq!(fg_at(&t, 0, 2), 0xFF5555);
}

#[test]
fn osc_10_and_11_defaults_survive_and_reset_to_the_new_theme() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b]10;#ABCDEF\x07\x1b]11;#010101\x07D");
	t.set_theme(theme(ThemeKind::GruvboxDark));
	assert_eq!((t.default_fg, t.default_bg), (0xABCDEF, 0x010101));
	assert_eq!((fg_at(&t, 0, 0), t.grid[0][0].bg), (0xABCDEF, 0x010101));
	send(&mut t, "\x1b]10;?\x07");
	assert_eq!(replies(&mut t), ["\x1b]10;rgb:ab/cd/ef\x07"]);
	send(&mut t, "\x1b]110\x07\x1b]111\x07");
	let gruvbox = theme(ThemeKind::GruvboxDark);
	assert_eq!((t.default_fg, t.default_bg), (gruvbox.fg, gruvbox.bg));
	assert_eq!(fg_at(&t, 0, 0), gruvbox.fg);
}

#[test]
fn only_the_changed_default_is_kept() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b]11;#010101\x07");
	t.set_theme(theme(ThemeKind::Dracula));
	assert_eq!((t.default_fg, t.default_bg), (0xF8F8F2, 0x010101));
	send(&mut t, "\x1b]10;?\x07\x1b]11;?\x07");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]10;rgb:f8/f8/f2\x07".to_owned(),
			"\x1b]11;rgb:01/01/01\x07".to_owned()
		]
	);
}

#[test]
fn the_themes_cursor_is_drawn_and_reported_under_osc_12() {
	let mut t = terminal(ThemeKind::Nord);
	t.set_theme(theme(ThemeKind::CatppuccinMocha));
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.cursor, Setting::Color(0xF5E0DC));
	send(&mut t, "\x1b]12;?\x07\x1b]12;#00FF00\x07\x1b]12;?\x07");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]12;rgb:f5/e0/dc\x07".to_owned(),
			"\x1b]12;rgb:00/ff/00\x07".to_owned()
		]
	);
	t.set_theme(theme(ThemeKind::CatppuccinLatte));
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.cursor, Setting::Color(0x00FF00));
	send(&mut t, "\x1b]112\x07");
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.cursor, Setting::Color(0xDC8A78));
}

#[test]
fn a_theme_without_a_cursor_color_leaves_it_to_the_text() {
	let mut t = terminal(ThemeKind::CatppuccinMocha);
	t.set_theme(theme(ThemeKind::Dracula));
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.cursor, Setting::Default);
	send(&mut t, "\x1b]12;?\x07");
	assert_eq!(replies(&mut t), ["\x1b]12;rgb:f8/f8/f2\x07"]);
}

#[test]
fn the_themes_selection_is_drawn_and_reported_under_osc_17_and_19() {
	let mut t = terminal(ThemeKind::Poop);
	t.set_theme(theme(ThemeKind::Nord));
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.selection_foreground, Setting::Color(0xD8DEE9));
	assert_eq!(frame.colors.selection_background, Setting::Color(0x434C5E));
	send(&mut t, "\x1b]17;?\x07\x1b]19;?\x07");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b]17;rgb:43/4c/5e\x07".to_owned(),
			"\x1b]19;rgb:d8/de/e9\x07".to_owned()
		]
	);
	// A program's selection wins, and survives the next switch.
	send(&mut t, "\x1b]17;#222222\x07");
	t.set_theme(theme(ThemeKind::Dracula));
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.selection_background, Setting::Color(0x222222));
	assert_eq!(frame.colors.selection_foreground, Setting::Color(0xF8F8F2));
}

#[test]
fn a_theme_without_a_selection_keeps_the_readable_default() {
	let mut t = terminal(ThemeKind::Nord);
	t.set_theme(theme(ThemeKind::GruvboxDark));
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.selection_background, Setting::Default);
	let (_, bg) = crate::ui::contrast::selection_colors(theme(ThemeKind::GruvboxDark).bg);
	send(&mut t, "\x1b]17;?\x07");
	assert_eq!(
		replies(&mut t),
		[format!("\x1b]17;{}\x07", encode_color(bg))]
	);
}

#[test]
fn a_full_reset_keeps_the_theme_and_drops_program_colors() {
	let mut t = terminal(ThemeKind::Nord);
	t.set_theme(theme(ThemeKind::Dracula));
	send(
		&mut t,
		"\x1b]10;#ABCDEF\x07\x1b]4;9;#000001\x07\x1bc\x1b[91mR",
	);
	assert_eq!(t.default_fg, 0xF8F8F2);
	assert_eq!(fg_at(&t, 0, 0), 0xFF6E6E);
	let mut frame = Frame::default();
	t.snapshot(&mut frame);
	assert_eq!(frame.colors.selection_background, Setting::Color(0x44475A));
}

#[test]
fn popped_colors_follow_the_theme_that_was_switched_to() {
	let mut t = terminal(ThemeKind::Nord);
	send(&mut t, "\x1b[#P");
	t.set_theme(theme(ThemeKind::Dracula));
	send(&mut t, "\x1b[#Q");
	assert_eq!((t.default_fg, t.default_bg), (0xF8F8F2, 0x282A36));
	// A program's own default in the stack is restored as it was.
	send(&mut t, "\x1b]11;#010101\x07\x1b[#P\x1b]111\x07");
	t.set_theme(theme(ThemeKind::Nord));
	send(&mut t, "\x1b[#Q");
	assert_eq!(t.default_bg, 0x010101);
}

#[test]
fn text_from_a_dark_theme_reads_after_switching_to_a_light_one() {
	for kind in [
		ThemeKind::GruvboxLight,
		ThemeKind::Alucard,
		ThemeKind::CatppuccinLatte,
	] {
		let mut t = terminal(ThemeKind::Dracula);
		send(&mut t, "\x1b[92mG\x1b[1;34mB\x1b[0mD");
		t.set_theme(theme(kind));
		let bg = t.default_bg;
		for col in 0..3 {
			let contrast = ratio(fg_at(&t, 0, col), bg);
			assert!(contrast >= 4.5, "{} col {col}: {contrast:.2}", kind.name());
		}
		assert_eq!(fg_at(&t, 0, 0), t.indexed_color(10), "{}", kind.name());
	}
}
