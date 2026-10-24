//! Poopterm's terminfo entry and choosing `TERM` for spawned shells.
//!
//! Shells get `TERM=xterm-poopterm` only when the entry is installed in a
//! system-wide directory. A copy only this user can see (`~/.terminfo`,
//! `TERMINFO_DIRS`) is lost under `sudo`, which resets those variables, and
//! programs such as `sudo dmesg` then print without colors. Otherwise shells
//! get `TERM=xterm-256color`.
use std::path::PathBuf;

#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod size_tests;

/// The entry's primary name; `poopterm` is an alias.
pub(crate) const NAME: &str = "xterm-poopterm";
/// The terminfo source, also printed by `poopterm --print-terminfo`.
pub(crate) const SOURCE: &str = include_str!("../terminfo/poopterm.terminfo");

/// `TERM` and further variables for spawned shells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TermEnv {
	pub term: String,
	/// Further variables, e.g. for automatic shell integration.
	pub extra: Vec<(String, std::ffi::OsString)>,
}

/// Picks `TERM`: an explicit config value wins; `auto` (or none) means
/// [`NAME`] when its terminfo is available.
pub(crate) fn choose_term(configured: Option<&str>, available: bool) -> &str {
	match configured.map(str::trim) {
		Some(term) if !term.is_empty() && term != "auto" => term,
		_ if available => NAME,
		_ => "xterm-256color",
	}
}

/// Resolves the environment for spawned shells. `configured` is the `term`
/// config key.
pub(crate) fn resolve(configured: Option<&str>) -> TermEnv {
	TermEnv {
		term: choose_term(configured, installed_in(&search_dirs())).to_owned(),
		extra: Vec::new(),
	}
}

/// True if any directory holds the compiled entry, in the Linux (`x/`) or
/// macOS (`78/`, hex) layout.
pub(crate) fn installed_in(dirs: &[PathBuf]) -> bool {
	dirs
		.iter()
		.any(|dir| dir.join("x").join(NAME).is_file() || dir.join("78").join(NAME).is_file())
}

/// System-wide directories ncurses searches, which root and `sudo` see too.
fn search_dirs() -> Vec<PathBuf> {
	[
		"/etc/terminfo",
		"/lib/terminfo",
		"/usr/share/terminfo",
		"/usr/lib/terminfo",
		"/usr/local/share/terminfo",
		"/opt/homebrew/share/terminfo",
		"/usr/local/opt/ncurses/share/terminfo",
	]
	.map(PathBuf::from)
	.to_vec()
}

#[cfg(test)]
pub(super) mod tests {
	use super::*;
	use std::{env, fs, path::Path, process::Command};

	/// Compiles [`SOURCE`] into `dir` with `tic -x`. Returns false if `tic`
	/// is missing or fails.
	pub(super) fn compile_into(dir: &Path) -> bool {
		let source = dir.join("poopterm.terminfo");
		fs::create_dir_all(dir).is_ok()
			&& fs::write(&source, SOURCE).is_ok()
			&& Command::new("tic")
				.arg("-x")
				.arg("-o")
				.arg(dir)
				.arg(&source)
				.output()
				.is_ok_and(|output| output.status.success())
	}

	pub(super) struct TempDir(pub(super) PathBuf);
	impl TempDir {
		pub(super) fn new(name: &str) -> Self {
			let dir =
				std::env::temp_dir().join(format!("poopterm-terminfo-{}-{name}", std::process::id()));
			let _ = fs::remove_dir_all(&dir);
			fs::create_dir_all(&dir).unwrap();
			Self(dir)
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	pub(super) fn have(tool: &str) -> bool {
		Command::new(tool).arg("-V").output().is_ok()
	}

	#[test]
	fn installed_entries_are_found_in_either_directory_layout() {
		let linux = TempDir::new("linux");
		fs::create_dir_all(linux.0.join("x")).unwrap();
		fs::write(linux.0.join("x").join(NAME), b"").unwrap();
		let mac = TempDir::new("mac");
		fs::create_dir_all(mac.0.join("78")).unwrap();
		fs::write(mac.0.join("78").join(NAME), b"").unwrap();
		let empty = TempDir::new("empty");
		assert!(installed_in(&[empty.0.clone(), linux.0.clone()]));
		assert!(installed_in(std::slice::from_ref(&mac.0)));
		assert!(!installed_in(std::slice::from_ref(&empty.0)));
	}

	#[test]
	fn only_system_wide_entries_count() {
		// sudo resets HOME and TERMINFO*, and ssh carries none of them, so an
		// entry only this user can see leaves programs without colors.
		let home = env::var_os("HOME").map(PathBuf::from);
		for dir in search_dirs() {
			assert!(dir.is_absolute(), "{dir:?}");
			assert!(
				home.as_ref().is_none_or(|home| !dir.starts_with(home)),
				"{dir:?}"
			);
		}
	}

	#[test]
	fn term_falls_back_to_xterm_256color_without_the_entry() {
		assert_eq!(choose_term(None, true), NAME);
		assert_eq!(choose_term(None, false), "xterm-256color");
		assert_eq!(choose_term(Some("auto"), false), "xterm-256color");
		assert_eq!(
			choose_term(Some("screen-256color"), true),
			"screen-256color"
		);
	}

	#[test]
	fn a_private_copy_can_be_compiled() {
		if !have("tic") {
			return;
		}
		let dir = TempDir::new("compile");
		assert!(compile_into(&dir.0));
		assert!(installed_in(std::slice::from_ref(&dir.0)));
	}

	#[test]
	fn the_entry_compiles_without_warnings() {
		if !have("tic") {
			return;
		}
		let dir = TempDir::new("warnings");
		let source = dir.0.join("poopterm.terminfo");
		fs::write(&source, SOURCE).unwrap();
		let output = Command::new("tic")
			.arg("-x")
			.arg("-o")
			.arg(&dir.0)
			.arg(&source)
			.output()
			.unwrap();
		assert!(output.status.success());
		// Every install runs tic, so anything it prints is seen by users.
		assert_eq!(String::from_utf8_lossy(&output.stderr), "");
	}

	#[test]
	fn the_last_name_field_is_a_description_and_the_aliases_remain() {
		let header = SOURCE.lines().find(|line| !line.starts_with('#')).unwrap();
		let names: Vec<&str> = header.trim_end_matches(',').split('|').collect();
		assert_eq!(&names[..2], ["xterm-poopterm", "poopterm"]);
		let description = names.last().unwrap();
		assert!(names.len() > 2 && description.contains(' '), "{header}");
	}

	#[test]
	fn infocmp_escapes_are_decoded_in_every_notation() {
		// ncurses 6 prints ^? and \E; macOS's ncurses 5.7 prints octal.
		assert_eq!(decode("^?"), "\x7f");
		assert_eq!(decode("\\177"), "\x7f");
		assert_eq!(decode("\\E[A"), "\x1b[A");
		assert_eq!(decode("\\e[1;2D"), "\x1b[1;2D");
		assert_eq!(decode("^H\\054"), "\x08,");
	}

	/// Decodes an infocmp string value: `\E`/`\e`, octal `\NNN`, `^X`.
	fn decode(value: &str) -> String {
		let mut out = String::new();
		let mut chars = value.chars().peekable();
		while let Some(c) = chars.next() {
			match c {
				'\\' => match chars.next() {
					Some('E' | 'e') => out.push('\x1b'),
					Some(d @ '0'..='7') => {
						let mut code = d.to_digit(8).unwrap();
						for _ in 0..2 {
							if let Some(d) = chars.peek().and_then(|d| d.to_digit(8)) {
								code = code * 8 + d;
								chars.next();
							}
						}
						out.push(char::from(code as u8));
					}
					Some(other) => out.push(other),
					None => {}
				},
				'^' => match chars.next() {
					Some('?') => out.push('\x7f'),
					Some(c) => out.push(char::from(c as u8 & 0x1f)),
					None => out.push('^'),
				},
				c => out.push(c),
			}
		}
		out
	}

	/// Reads string capabilities from the compiled entry via infocmp.
	pub(super) fn capabilities(dir: &Path) -> std::collections::HashMap<String, String> {
		let output = Command::new("infocmp")
			.args(["-x", "-1", "-A"])
			.arg(dir)
			.arg(NAME)
			.output()
			.unwrap();
		String::from_utf8_lossy(&output.stdout)
			.lines()
			.filter_map(|line| line.trim().trim_end_matches(',').split_once('='))
			.map(|(name, value)| (name.to_owned(), decode(value)))
			.collect()
	}

	#[test]
	fn advertised_keys_match_what_poopterm_sends() {
		if !have("tic") || !have("infocmp") {
			return;
		}
		use crate::input::keys::{KeyModes, Modifiers, encode_key};
		use winit::keyboard::{Key, NamedKey};
		let dir = TempDir::new("keys");
		assert!(compile_into(&dir.0));
		let caps = capabilities(&dir.0);
		// ncurses enables application cursor and keypad modes (smkx)
		// before reading keys.
		let app = KeyModes {
			application_cursor: true,
			application_keypad: true,
			..KeyModes::default()
		};
		let shift = Modifiers {
			shift: true,
			..Default::default()
		};
		let ctrl = Modifiers {
			ctrl: true,
			..Default::default()
		};
		let none = Modifiers::default();
		for (cap, key, mods) in [
			("kcuu1", NamedKey::ArrowUp, none),
			("kcud1", NamedKey::ArrowDown, none),
			("kcub1", NamedKey::ArrowLeft, none),
			("kcuf1", NamedKey::ArrowRight, none),
			("khome", NamedKey::Home, none),
			("kend", NamedKey::End, none),
			("kdch1", NamedKey::Delete, none),
			("kich1", NamedKey::Insert, none),
			("kpp", NamedKey::PageUp, none),
			("knp", NamedKey::PageDown, none),
			("kbs", NamedKey::Backspace, none),
			("kf1", NamedKey::F1, none),
			("kf5", NamedKey::F5, none),
			("kf12", NamedKey::F12, none),
			("kLFT", NamedKey::ArrowLeft, shift),
			("kLFT5", NamedKey::ArrowLeft, ctrl),
			("kf13", NamedKey::F1, shift),
			("kcbt", NamedKey::Tab, shift),
		] {
			let sent = encode_key(&Key::Named(key), mods, app).unwrap();
			assert_eq!(
				caps.get(cap).map(String::as_bytes),
				Some(sent.as_slice()),
				"{cap}"
			);
		}
		use crate::input::keypad::encode_keypad;
		let text = |s: &str| Key::Character(s.into());
		for (cap, key) in [
			("kent", Key::Named(NamedKey::Enter)),
			("kb2", Key::Named(NamedKey::Clear)),
			("kp5", Key::Named(NamedKey::Clear)),
			("kpADD", text("+")),
			("kpSUB", text("-")),
			("kpMUL", text("*")),
			("kpDIV", text("/")),
			("kpCMA", text(",")),
		] {
			let sent = encode_keypad(&key, none, app).unwrap();
			assert_eq!(
				caps.get(cap).map(String::as_bytes),
				Some(sent.as_slice()),
				"{cap}"
			);
		}
		for unsent in ["ka2", "kb1", "kb3", "kc2", "kpDOT", "kpZRO"] {
			assert!(!caps.contains_key(unsent), "{unsent} is never sent");
		}
	}

	#[test]
	fn sequences_generated_from_the_entry_take_effect() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::{CursorShape, Term},
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("tput");
		assert!(compile_into(&dir.0));
		let tput = |args: &[&str]| {
			Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.args(args)
				.output()
				.unwrap()
				.stdout
		};
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		for bytes in [
			tput(&["smcup"]),
			tput(&["setrgbf", "1", "2", "3"]),
			b"x".to_vec(),
			tput(&["rep", "65", "3"]),
			tput(&["Ss", "6"]),
		] {
			parser.advance(&mut term, &bytes);
		}
		assert!(
			term.primary_screen.is_some(),
			"smcup enters the alternate screen"
		);
		assert_eq!(term.fg, 0x010203, "setrgbf sets a truecolor foreground");
		let row: String = term.grid[0].iter().map(|c| c.ch).collect();
		assert_eq!(&row[..4], "xAAA", "rep repeats a character");
		assert_eq!(
			term.cursor_shape,
			CursorShape::Bar,
			"Ss sets the cursor shape"
		);
	}

	#[test]
	fn cursor_color_overline_and_single_insert_take_effect() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::{Dynamic, Term, attr::OVERLINE},
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("extras");
		assert!(compile_into(&dir.0));
		let tput = |args: &[&str]| {
			let output = Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.args(args)
				.output()
				.unwrap();
			assert!(output.status.success(), "{args:?} is advertised");
			output.stdout
		};
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let default_cursor = term.dynamic_color(Dynamic::Cursor);
		let mut parser = Parser::new();
		parser.advance(&mut term, &tput(&["Cs", "rgb:12/34/56"]));
		assert_eq!(term.dynamic_color(Dynamic::Cursor), 0x123456);
		parser.advance(&mut term, &tput(&["Cr"]));
		assert_eq!(term.dynamic_color(Dynamic::Cursor), default_cursor);
		parser.advance(&mut term, &tput(&["Smol"]));
		assert_ne!(term.attrs & OVERLINE, 0, "Smol starts overlining");
		parser.advance(&mut term, &tput(&["Rmol"]));
		assert_eq!(term.attrs & OVERLINE, 0, "Rmol stops it");
		parser.advance(&mut term, b"abc\r");
		parser.advance(&mut term, &tput(&["ich1"]));
		let row: String = term.grid[0].iter().map(|c| c.ch).collect();
		assert_eq!(&row[..4], " abc", "ich1 inserts one blank");
	}

	#[test]
	fn memory_lock_and_title_capabilities_take_effect() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::Term,
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("memory");
		assert!(compile_into(&dir.0));
		let tput = |args: &[&str]| {
			let output = Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.args(args)
				.output()
				.unwrap();
			assert!(output.status.success(), "{args:?} is advertised");
			output.stdout
		};
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		parser.advance(&mut term, b"\x1b[3H");
		parser.advance(&mut term, &tput(&["meml"]));
		assert_eq!(term.scroll_top, 2, "meml locks the lines above the cursor");
		parser.advance(&mut term, &tput(&["memu"]));
		assert_eq!(term.scroll_top, 0, "memu unlocks them");
		assert_eq!(tput(&["TS"]), tput(&["tsl"]), "TS starts a title like tsl");
		let mut title = tput(&["TS"]);
		title.extend_from_slice(b"hello\x07");
		parser.advance(&mut term, &title);
		assert_eq!(term.title, "hello");
	}

	#[test]
	fn cub1_in_the_first_column_wraps_as_bw_says() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::Term,
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("bw");
		assert!(compile_into(&dir.0));
		let tput = |cap: &str| {
			Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.arg(cap)
				.output()
				.unwrap()
		};
		assert!(tput("bw").status.success(), "bw is advertised");
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		parser.advance(&mut term, b"\x1b[3H");
		parser.advance(&mut term, &tput("cub1").stdout);
		assert_eq!((term.cx, term.cy), (9, 1));
	}

	#[test]
	fn flash_reverses_the_screen_and_puts_it_back() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::Term,
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("flash");
		assert!(compile_into(&dir.0));
		let output = Command::new("tput")
			.env("TERMINFO", &dir.0)
			.arg("-T")
			.arg(NAME)
			.arg("flash")
			.output()
			.unwrap();
		assert!(output.status.success(), "flash is advertised");
		let flash = output.stdout;
		let on = b"\x1b[?5h";
		assert!(flash.starts_with(on) && flash.ends_with(b"\x1b[?5l"));
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		parser.advance(&mut term, on);
		assert!(term.reverse_screen, "the flash turns the screen around");
		parser.advance(&mut term, &flash[on.len()..]);
		assert!(!term.reverse_screen, "and back");
	}

	#[test]
	fn meta_mode_capabilities_switch_alt_between_the_eighth_bit_and_escape() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			input::keys::{KeyModes, Modifiers, encode_key},
			parser::Parser,
			term::Term,
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("meta");
		assert!(compile_into(&dir.0));
		let tput = |cap: &str| {
			let output = Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.arg(cap)
				.output()
				.unwrap();
			assert!(output.status.success(), "{cap} is advertised");
			output.stdout
		};
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		let alt_a = |term: &Term| {
			let modes = KeyModes {
				keyboard: term.keyboard,
				..KeyModes::default()
			};
			let alt = Modifiers {
				alt: true,
				..Modifiers::default()
			};
			encode_key(&winit::keyboard::Key::Character("a".into()), alt, modes).unwrap()
		};
		assert_eq!(alt_a(&term), b"\x1ba", "Alt sends ESC by default");
		parser.advance(&mut term, &tput("smm"));
		assert_eq!(alt_a(&term), "\u{e1}".as_bytes(), "smm sets the eighth bit");
		parser.advance(&mut term, &tput("rmm"));
		assert_eq!(alt_a(&term), b"\x1ba", "rmm goes back to ESC");
		assert!(!term.keyboard.eight_bit_meta, "and clears the eighth bit");
	}

	#[test]
	fn setulc_and_setal_set_the_underline_color() {
		if !have("tic") || !have("tput") {
			return;
		}
		use crate::{
			parser::Parser,
			term::Term,
			theme::{Theme, ThemeKind},
		};
		let dir = TempDir::new("underline");
		assert!(compile_into(&dir.0));
		let tput = |args: &[&str]| {
			let output = Command::new("tput")
				.env("TERMINFO", &dir.0)
				.arg("-T")
				.arg(NAME)
				.args(args)
				.output()
				.unwrap();
			assert!(output.status.success(), "{args:?} is advertised");
			output.stdout
		};
		assert_eq!(tput(&["Setulc", "1193046"]), b"\x1b[58:2::18:52:86m");
		assert_eq!(tput(&["setal", "1193046"]), tput(&["Setulc", "1193046"]));
		let mut term = Term::new(10, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		let mut parser = Parser::new();
		parser.advance(&mut term, &tput(&["setal", &0xff00ff.to_string()]));
		parser.advance(&mut term, b"A");
		let extra = term.grid[0][0].extra;
		assert_eq!(term.extras.underline_color(extra), Some(0xff00ff));
	}
}
