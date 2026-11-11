//! iTerm2's OSC 1337 commands besides images: from bytes in to terminal
//! state, window requests, clipboard writes and replies out.
use super::{
	CursorShape, Setting,
	clipboard::Selection,
	iterm::{Attention, WindowRequest},
};
use crate::{
	graphics::base64::encode,
	parser::Parser,
	shell_integration::Phase,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(10, 3, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn b64(text: &str) -> String {
	encode(text.as_bytes(), false)
}

/// The text of every clipboard write so far.
fn copied(term: &Term) -> Vec<String> {
	term
		.clipboard_writes
		.iter()
		.map(|write| {
			assert_eq!(write.selection, Selection::Clipboard);
			String::from_utf8(write.items[0].1.to_vec()).unwrap()
		})
		.collect()
}

#[test]
fn cursor_shapes_and_the_cursor_guide() {
	let mut t = terminal();
	for (value, shape) in [
		("1", CursorShape::Bar),
		("2", CursorShape::Underline),
		("7", CursorShape::Underline),
		("0", CursorShape::Block),
	] {
		send(&mut t, &format!("\x1b]1337;CursorShape={value}\x07"));
		assert_eq!(t.cursor_shape, shape, "{value}");
	}
	send(&mut t, "\x1b]1337;HighlightCursorLine=yes\x07");
	assert!(t.iterm.cursor_guide);
	send(&mut t, "\x1b]1337;HighlightCursorLine=maybe\x07");
	assert!(t.iterm.cursor_guide);
	send(&mut t, "\x1b]1337;HighlightCursorLine=no\x07");
	assert!(!t.iterm.cursor_guide);
}

#[test]
fn marks_directories_and_hosts() {
	let mut t = terminal();
	send(&mut t, "\x1b]133;A\x07\r\n\r\n\x1b]1337;SetMark\x07");
	assert_eq!(t.shell.phase, Phase::Prompt);
	assert_eq!(t.shell.previous_prompt(3), Some(2));
	assert_eq!(t.shell.previous_prompt(2), Some(0));
	send(&mut t, "\x1b]1337;CurrentDir=/srv/www\x07");
	assert_eq!(t.cwd, std::path::Path::new("/srv/www"));
	send(&mut t, "\x1b]1337;CurrentDir=relative\x07");
	assert_eq!(t.cwd, std::path::Path::new("/srv/www"));
	send(&mut t, "\x1b]1337;RemoteHost=me@web1.example.com\x07");
	assert_eq!(t.iterm.remote_host.as_deref(), Some("me@web1.example.com"));
	let long = format!("u@{}", "h".repeat(254));
	for value in ["nobody", "a\u{85}@b", long.as_str()] {
		send(&mut t, &format!("\x1b]1337;RemoteHost={value}\x07"));
		assert_eq!(
			t.iterm.remote_host.as_deref(),
			Some("me@web1.example.com"),
			"{value}"
		);
	}
}

#[test]
fn clearing_the_scrollback() {
	let mut t = terminal();
	send(&mut t, "1\r\n2\r\n3\r\n4\r\n5");
	assert_eq!(t.scrollback.len(), 2);
	send(&mut t, "\x1b]1337;ClearScrollback\x07");
	assert!(t.scrollback.is_empty());
	assert_eq!(t.history_base, 2);
}

#[test]
fn window_requests_go_to_the_ui() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]1337;StealFocus\x07\x1b]1337;RequestAttention=yes\x07\
		 \x1b]1337;RequestAttention=once\x07\x1b]1337;RequestAttention=fireworks\x07\
		 \x1b]1337;RequestAttention=no\x07\x1b]1337;RequestAttention=x\x07",
	);
	assert_eq!(
		t.window_requests,
		[
			WindowRequest::Focus,
			WindowRequest::Attention(Some(Attention::Critical)),
			WindowRequest::Attention(Some(Attention::Informational)),
			WindowRequest::Attention(Some(Attention::Informational)),
			WindowRequest::Attention(None),
		]
	);
}

#[test]
fn copying_to_the_clipboard() {
	let mut t = terminal();
	send(&mut t, &format!("\x1b]1337;Copy=:{}\x07", b64("hello")));
	// Without the colon, or with bad base64, nothing is copied.
	send(
		&mut t,
		&format!("\x1b]1337;Copy={}\x07\x1b]1337;Copy=:@@\x07", b64("x")),
	);
	assert_eq!(copied(&t), ["hello"]);
	// Text between CopyToClipboard and EndCopy is copied (and still shown).
	send(
		&mut t,
		"\x1b]1337;CopyToClipboard=\x07ab\r\ncd\x1b[1mé\x1b]1337;EndCopy\x07after",
	);
	assert_eq!(copied(&t), ["hello", "ab\ncdé"]);
	assert!(
		t.grid[1]
			.iter()
			.map(|cell| cell.ch)
			.collect::<String>()
			.starts_with("cdéafter")
	);
	// Other pasteboards are not ours; an EndCopy without a start does nothing.
	send(
		&mut t,
		"\x1b]1337;CopyToClipboard=find\x07x\x1b]1337;EndCopy\x07\x1b]1337;EndCopy\x07",
	);
	assert_eq!(copied(&t).len(), 2);
	// Too much text cancels the copy.
	t.clipboard_writes.clear();
	send(&mut t, "\x1b]1337;CopyToClipboard=\x07");
	let chunk = "x".repeat(64 * 1024);
	for _ in 0..17 {
		send(&mut t, &chunk);
	}
	send(&mut t, "\x1b]1337;EndCopy\x07");
	assert!(t.clipboard_writes.is_empty());
}

#[test]
fn user_variables_are_set_and_reported() {
	let mut t = terminal();
	send(
		&mut t,
		&format!(
			"\x1b]1337;SetUserVar=gitBranch={}\x07\x1b]1337;SetUserVar=bad=@@\x07",
			b64("main")
		),
	);
	send(
		&mut t,
		&format!(
			"\x1b]1337;ReportVariable={}\x07\x1b]1337;ReportVariable={}\x1b\\\
			 \x1b]1337;ReportVariable={}\x07",
			b64("user.gitBranch"),
			b64("user.bad"),
			b64("session.hostname")
		),
	);
	assert_eq!(
		t.responses,
		[
			// Replies are padded, for strict decoders.
			format!("\x1b]1337;ReportVariable={}\x07", encode(b"main", true)).into_bytes(),
			b"\x1b]1337;ReportVariable=\x1b\\".to_vec(),
			b"\x1b]1337;ReportVariable=\x07".to_vec(),
		]
	);
	// Names and values are bounded, and so is their number.
	t.responses.clear();
	let long_name = "n".repeat(65);
	send(
		&mut t,
		&format!("\x1b]1337;SetUserVar={long_name}={}\x07", b64("v")),
	);
	send(
		&mut t,
		&format!("\x1b]1337;SetUserVar=big={}\x07", b64(&"v".repeat(4097))),
	);
	assert_eq!(t.iterm.user_var(&long_name), None);
	assert_eq!(t.iterm.user_var("big"), None);
	for index in 0..100 {
		send(
			&mut t,
			&format!("\x1b]1337;SetUserVar=v{index}={}\x07", b64("1")),
		);
	}
	assert_eq!(t.iterm.user_var("v62"), Some("1"));
	assert_eq!(t.iterm.user_var("v63"), None);
	// Known names can still change.
	send(
		&mut t,
		&format!("\x1b]1337;SetUserVar=gitBranch={}\x07", b64("dev")),
	);
	assert_eq!(t.iterm.user_var("gitBranch"), Some("dev"));
}

#[test]
fn colors_follow_iterm_names() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]1337;SetColors=fg=102030\x07\x1b]1337;SetColors=bg=srgb:405060\x07\
		 \x1b]1337;SetColors=red=f00\x07\x1b]1337;SetColors=br_white=p3:ffffff\x07\
		 \x1b]1337;SetColors=selbg=010203\x07\x1b]1337;SetColors=selfg=040506\x07\
		 \x1b]1337;SetColors=curbg=070809\x07\x1b]1337;SetColors=curfg=0a0b0c\x07\
		 \x1b]1337;SetColors=bold=0d0e0f\x07\x1b]1337;SetColors=underline=111213\x07",
	);
	assert_eq!((t.default_fg, t.default_bg), (0x102030, 0x405060));
	assert_eq!(t.color_overrides[1], Some(0xff0000));
	assert_eq!(t.color_overrides[15], Some(0xffffff));
	assert_eq!(t.colors.selection_background, Setting::Color(0x010203));
	assert_eq!(t.colors.selection_foreground, Setting::Color(0x040506));
	assert_eq!(t.colors.cursor, Setting::Color(0x070809));
	assert_eq!(t.colors.cursor_text, Setting::Color(0x0a0b0c));
	assert_eq!(t.colors.special[0], Some(0x0d0e0f));
	assert_eq!(t.colors.special[1], Some(0x111213));
	assert!(t.colors.special_modes[0] && t.colors.special_modes[1]);
	// Bad values, unknown keys and presets change nothing.
	let before = t.colors;
	send(
		&mut t,
		"\x1b]1337;SetColors=fg=12\x07\x1b]1337;SetColors=fg=xyz:abcdef\x07\
		 \x1b]1337;SetColors=fg=gggggg\x07\x1b]1337;SetColors=preset=Solarized\x07\
		 \x1b]1337;SetColors=tab=default\x07\x1b]1337;SetColors=nope=fff\x07",
	);
	assert_eq!(t.colors, before);
	assert_eq!(t.default_fg, 0x102030);
}

#[test]
fn queries_are_answered_with_the_queries_terminator() {
	let mut t = terminal();
	t.cell_width = 16;
	t.cell_height = 35;
	t.scale = 2.0;
	send(
		&mut t,
		"\x1b]1337;ReportCellSize\x07\x1b]1337;RequestUpload=format=tgz\x1b\\",
	);
	assert_eq!(
		t.responses,
		[
			b"\x1b]1337;ReportCellSize=17.50;8.00;2.0\x07".to_vec(),
			b"abort\n".to_vec(),
		]
	);
}

#[test]
fn unsupported_commands_are_ignored() {
	let mut t = terminal();
	for command in [
		"SetProfile=Default",
		"SetProfileProperty=Cursor Type=Mg==",
		"SetBackgroundImageFile=L2V0Yy9wYXNzd2Q=",
		"OpenURL=:aHR0cHM6Ly9leGFtcGxlLmNvbQ==",
		"SetKeyLabel=F1=Help",
		"PushKeyLabels",
		"UnicodeVersion=8",
		"ShellIntegrationVersion=19;bash",
		"ClearCapturedOutput",
		"Custom=id=secret:pattern",
		"AddAnnotation=hello",
		"Block=id=b;attr=start",
		"Button=type=custom;code=42",
		"Disinter",
		"Nonsense",
	] {
		send(&mut t, &format!("\x1b]1337;{command}\x07"));
	}
	assert!(t.responses.is_empty() && t.window_requests.is_empty());
	assert!(t.clipboard_writes.is_empty());
	assert_eq!((t.cx, t.cy), (0, 0));
}

#[test]
fn remote_hosts_are_labeled_unless_they_are_this_one() {
	let mut t = terminal();
	assert_eq!(t.iterm.remote_label("zeta"), None);
	send(&mut t, "\x1b]1337;RemoteHost=me@zeta.example.com\x07");
	assert_eq!(t.iterm.remote_label("zeta"), None);
	assert_eq!(t.iterm.remote_label("ZETA.example.com"), None);
	assert_eq!(t.iterm.remote_label("beta"), Some("me@zeta.example.com"));
}

#[test]
fn copies_capture_every_encoded_length_once_across_reads() {
	let mut t = terminal();
	let mut parser = Parser::new();
	parser.advance(
		&mut t,
		b"\x1b]1337;CopyToClipboard=\x07\xe7\x8c\xab\xf0\x9f",
	);
	// The emoji is cut between reads; it is captured once, whole.
	parser.advance(&mut t, b"\x98\x80\xc3\xa9\x1b]1337;EndCopy\x07");
	assert_eq!(copied(&t), ["\u{732b}\u{1f600}\u{e9}"]);
}

#[test]
fn copies_capture_replacements_but_not_c1_controls() {
	let mut t = terminal();
	// NEL (C1, encoded) moves the cursor but is not text; a stray byte is
	// shown and copied as U+FFFD.
	send(&mut t, "\x1b]1337;CopyToClipboard=\x07a\u{85}b");
	Parser::new().advance(&mut t, b"\xffc\x1b]1337;EndCopy\x07");
	assert_eq!(copied(&t), ["ab\u{fffd}c"]);
	assert_eq!(t.grid[1][0].ch, 'b');
}
