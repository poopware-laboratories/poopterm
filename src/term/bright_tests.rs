//! Bright colors 8–15 in the terminal: SGR 90–97 and 100–107, 38;5 and
//! 48;5, OSC 4, theme switches and RIS, on light and dark themes.
use crate::{
	parser::{Parser, encode_color},
	term::Term,
	theme::{Theme, ThemeKind},
	ui::contrast::ratio,
};

fn terminal(kind: ThemeKind) -> Term {
	Term::new(8, 2, Theme::from_kind(kind), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn first_cell(term: &Term) -> (u32, u32) {
	let cell = term.grid[0][0];
	(cell.fg, cell.bg)
}

#[test]
fn a_bright_green_prompt_reads_on_light_mode() {
	let mut t = terminal(ThemeKind::MacOsLight);
	send(&mut t, "\x1b[92mX");
	let (fg, _) = first_cell(&t);
	assert_eq!(fg, 0x2B812B);
	assert!(ratio(fg, t.default_bg) >= 4.5);
}

#[test]
fn a_bold_prompt_reads_on_every_light_theme() {
	// The prompt that showed the bug: bold green user, bold blue directory.
	for kind in [
		ThemeKind::MacOsLight,
		ThemeKind::RosePineDawn,
		ThemeKind::CatppuccinLatte,
	] {
		for prompt in ["\x1b[1;32mX", "\x1b[1;34mX"] {
			let mut t = terminal(kind);
			send(&mut t, prompt);
			let fg = first_cell(&t).0;
			let contrast = ratio(fg, t.default_bg);
			assert!(contrast >= 4.5, "{} {prompt:?}: {contrast:.2}", kind.name());
		}
	}
}

#[test]
fn every_way_to_name_a_bright_color_gives_the_same_one() {
	let mut t = terminal(ThemeKind::CatppuccinLatte);
	let expected = t.indexed_color(11);
	assert!(ratio(expected, t.default_bg) >= 4.5);
	for sequence in ["\x1b[93mX", "\x1b[38;5;11mX", "\x1b[1;33m\x1b[93mX"] {
		send(&mut t, "\x1b[H\x1b[0m");
		send(&mut t, sequence);
		assert_eq!(first_cell(&t).0, expected, "{sequence:?}");
	}
	send(&mut t, "\x1b[H\x1b[0m\x1b[103mX");
	assert_eq!(first_cell(&t).1, expected);
	send(&mut t, "\x1b[H\x1b[0m\x1b[48;5;11mX");
	assert_eq!(first_cell(&t).1, expected);
}

#[test]
fn dark_themes_still_draw_xterms_bright_green() {
	let mut t = terminal(ThemeKind::Poop);
	send(&mut t, "\x1b[92mX");
	assert_eq!(first_cell(&t).0, 0x55FF55);
}

#[test]
fn osc_4_reports_the_readable_bright_color() {
	let mut t = terminal(ThemeKind::MacOsLight);
	send(&mut t, "\x1b]4;10;?\x07");
	let reply = String::from_utf8(t.responses.remove(0)).unwrap();
	assert_eq!(reply, format!("\x1b]4;10;{}\x07", encode_color(0x2B812B)));
}

#[test]
fn a_program_set_bright_color_wins_until_it_is_reset() {
	let mut t = terminal(ThemeKind::MacOsLight);
	send(&mut t, "\x1b]4;10;#00ff00\x07");
	assert_eq!(t.indexed_color(10), 0x00FF00);
	send(&mut t, "\x1b]104;10\x07");
	assert_eq!(t.indexed_color(10), 0x2B812B);
}

#[test]
fn switching_themes_switches_the_bright_colors() {
	let mut t = terminal(ThemeKind::Poop);
	assert_eq!(t.indexed_color(10), 0x55FF55);
	t.set_theme(Theme::from_kind(ThemeKind::MacOsLight));
	assert_eq!(t.indexed_color(10), 0x2B812B);
	t.set_theme(Theme::from_kind(ThemeKind::Nord));
	assert_eq!(t.indexed_color(10), 0x55FF55);
}

#[test]
fn a_full_reset_keeps_the_light_themes_bright_colors() {
	let mut t = terminal(ThemeKind::MacOsLight);
	send(&mut t, "\x1bc");
	assert_eq!(t.indexed_color(10), 0x2B812B);
}
