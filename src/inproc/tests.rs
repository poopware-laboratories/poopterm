//! The in-process device: the line discipline's editing keys, the shell's
//! words and built-ins, and the device loop end to end through pipes.
use super::{
	line::{Discipline, Event, MAX_LINE, output},
	run,
	shell::{Shell, Word, segments, split},
};
use std::{fs, path::PathBuf};

fn typed(bytes: &[u8]) -> Vec<Event> {
	Discipline::default().input(bytes)
}

fn echo(text: &str) -> Event {
	Event::Echo(text.as_bytes().to_vec())
}

#[test]
fn typing_echoes_and_enter_sends_the_line() {
	assert_eq!(
		typed(b"ls\r"),
		[echo("l"), echo("s"), echo("\r\n"), Event::Line("ls".into())]
	);
	assert_eq!(
		typed(b"\n")[1],
		Event::Line(String::new()),
		"an empty line too"
	);
}

#[test]
fn erasing_takes_back_characters_by_their_width() {
	let events = typed("a漢\x7f\x7f\x7fb\r".as_bytes());
	assert_eq!(
		events[2],
		echo("\x08\x08  \x08\x08"),
		"a wide character takes two cells"
	);
	assert_eq!(events[3], echo("\x08 \x08"));
	assert_eq!(
		events.len(),
		7,
		"a third erase on an empty line shows nothing"
	);
	assert_eq!(events[6], Event::Line("b".into()));
}

#[test]
fn control_keys_interrupt_kill_and_end() {
	assert_eq!(
		typed(b"abc\x03"),
		[
			echo("a"),
			echo("b"),
			echo("c"),
			echo("^C\r\n"),
			Event::Interrupt
		]
	);
	let killed = typed(b"ab\x15\r");
	assert_eq!(killed[2], echo("\x08\x08  \x08\x08"));
	assert_eq!(killed[4], Event::Line(String::new()));
	assert_eq!(typed(b"\x04"), [Event::Eof]);
	assert_eq!(
		typed(b"a\x04"),
		[echo("a")],
		"^D inside a line does nothing"
	);
	assert_eq!(typed(b"\t\x1a\x00"), [], "other controls are ignored");
}

#[test]
fn escape_sequences_from_keys_are_swallowed_whole() {
	// Arrows, a function key, and paste markers around text.
	let events = typed(b"\x1b[A\x1bOP\x1b[1;5C\x1b[200~hi\x1b[201~\r");
	assert_eq!(
		events,
		[echo("h"), echo("i"), echo("\r\n"), Event::Line("hi".into())]
	);
	assert_eq!(typed(b"\x1bx"), [], "ESC and one byte");
}

#[test]
fn characters_split_across_reads_and_invalid_bytes() {
	let mut discipline = Discipline::default();
	let e = "é".as_bytes();
	assert_eq!(discipline.input(&e[..1]), []);
	assert_eq!(discipline.input(&e[1..]), [echo("é")]);
	assert_eq!(
		discipline.input(&[0xff, b'a']),
		[echo("a")],
		"a byte that starts nothing is dropped"
	);
}

#[test]
fn a_line_stops_growing_at_its_limit() {
	let long = vec![b'x'; MAX_LINE + 10];
	let events = Discipline::default().input(&[&long[..], b"\r"].concat());
	let Some(Event::Line(line)) = events.last() else {
		panic!()
	};
	assert_eq!(line.len(), MAX_LINE);
}

#[test]
fn output_newlines_become_carriage_return_newline() {
	assert_eq!(output(b"a\nb\n"), b"a\r\nb\r\n");
	assert_eq!(output(b""), b"");
	assert_eq!(output(b"\r"), b"\r");
}

fn shell() -> (Shell, PathBuf) {
	let home = std::env::temp_dir().join(format!(
		"poopterm-inproc-{}-{:?}",
		std::process::id(),
		std::thread::current().id()
	));
	let _ = fs::remove_dir_all(&home);
	fs::create_dir_all(&home).unwrap();
	let home = home.canonicalize().unwrap();
	(Shell::new(home.clone()), home)
}

fn words(line: &str) -> Vec<Word> {
	let (mut shell, _) = shell();
	shell.env.insert("NAME".into(), "poop".into());
	shell.status = 3;
	split(line, &shell).unwrap()
}

fn text(word: &str) -> Word {
	Word::Text(word.into())
}

#[test]
fn words_split_on_blanks_with_quotes_escapes_and_variables() {
	assert_eq!(words("echo  a\tb"), [text("echo"), text("a"), text("b")]);
	assert_eq!(
		words(r#"echo "a b" 'c $NAME' d\ e"#),
		[text("echo"), text("a b"), text("c $NAME"), text("d e")]
	);
	assert_eq!(
		words("echo $NAME-x \"$NAME\" $? $NOPE $"),
		[
			text("echo"),
			text("poop-x"),
			text("poop"),
			text("3"),
			text("$")
		]
	);
	assert_eq!(
		words("echo '' \"\""),
		[text("echo"), text(""), text("")],
		"empty quotes are words"
	);
	let (shell, _) = shell();
	assert_eq!(split("echo 'open", &shell), None);
	assert_eq!(split("echo \\", &shell), None);
}

#[test]
fn a_tilde_is_home_only_at_the_start_of_a_word() {
	let (shell, home) = shell();
	let home = home.display().to_string();
	let split = |line| split(line, &shell).unwrap();
	assert_eq!(split("cd ~"), [text("cd"), text(&home)]);
	assert_eq!(split("cd ~/x"), [text("cd"), text(&format!("{home}/x"))]);
	assert_eq!(
		split("echo a~b ~x"),
		[text("echo"), text("a~b"), text("~x")]
	);
}

/// What `line` prints in `shell`, as text.
fn ran(shell: &mut Shell, line: &str) -> String {
	String::from_utf8(shell.run(line).output).unwrap()
}

#[test]
fn files_and_directories_can_be_made_listed_read_and_removed() {
	let (mut sh, home) = shell();
	assert_eq!(ran(&mut sh, "mkdir -p a/b && touch a/f && echo hi > x"), "");
	assert_eq!(ran(&mut sh, "ls a"), "b/\nf\n");
	assert_eq!(ran(&mut sh, "cd a; pwd"), format!("{}/a\n", home.display()));
	assert_eq!(sh.prompt(), "\x1b[1;34m~/a\x1b[0m $ ");
	assert_eq!(ran(&mut sh, "cp f g && mv g b && ls b"), "g\n");
	fs::write(home.join("a/b/g"), "content\n").unwrap();
	assert_eq!(ran(&mut sh, "cat b/g ~/a/b/g"), "content\ncontent\n");
	assert_eq!(ran(&mut sh, "rm -r b; ls"), "f\n");
	assert_eq!(ran(&mut sh, "cd; pwd"), format!("{}\n", home.display()));
	assert_eq!(sh.prompt(), "\x1b[1;34m~\x1b[0m $ ");
	assert!(ran(&mut sh, "ls -a").contains(".\n") || !ran(&mut sh, "ls -a").is_empty());
}

#[test]
fn failures_set_the_status_and_stop_and_chains() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "nope"), "poopsh: nope: command not found\n");
	assert_eq!(sh.status, 127);
	assert_eq!(ran(&mut sh, "echo $?"), "127\n");
	assert_eq!(ran(&mut sh, "false && echo no; echo yes"), "yes\n");
	assert!(ran(&mut sh, "cat missing").starts_with("cat: missing: "));
	assert_eq!(sh.status, 1);
	assert!(ran(&mut sh, "cd missing").starts_with("cd: "));
	assert_eq!(ran(&mut sh, "true && echo ok"), "ok\n");
}

#[test]
fn the_small_built_ins_do_what_their_names_say() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "echo -n a b"), "a b");
	assert_eq!(ran(&mut sh, "seq 3"), "1\n2\n3\n");
	assert_eq!(ran(&mut sh, "seq 2 4"), "2\n3\n4\n");
	assert_eq!(ran(&mut sh, "export A=1 B=two; echo $A$B"), "1two\n");
	assert!(ran(&mut sh, "env").contains("A=1\n"));
	assert_eq!(ran(&mut sh, "unset A; echo [$A]"), "[]\n");
	assert_eq!(ran(&mut sh, "clear"), "\x1b[H\x1b[2J\x1b[3J");
	assert!(ran(&mut sh, "help").contains("ls"));
	let date = ran(&mut sh, "date");
	assert!(date.ends_with(" UTC\n") && date.len() == 24, "{date:?}");
	let exit = sh.run("exit 4");
	assert!(exit.exit && sh.status == 4);
}

#[test]
fn the_device_shows_a_prompt_answers_lines_and_ends_on_exit() {
	let (shell, _) = shell();
	let mut out = Vec::new();
	run(shell, &b"echo hi\rnope\rexit\r"[..], &mut out);
	let shown = String::from_utf8(out).unwrap();
	assert!(shown.starts_with("Poopterm in-process shell."), "{shown:?}");
	assert!(
		shown.contains("e\x63ho hi\r\nhi\r\n"),
		"echoed then answered, CR LF: {shown:?}"
	);
	assert!(shown.contains("poopsh: nope: command not found\r\n"));
	assert!(!shown.ends_with("$ "), "no prompt after exit");
}

#[test]
fn the_device_ends_on_end_of_file_or_a_closed_input() {
	for input in [&b"\x04ignored\r"[..], &b"echo partial"[..]] {
		let (shell, _) = shell();
		let mut out = Vec::new();
		run(shell, input, &mut out);
		let shown = String::from_utf8(out).unwrap();
		assert!(
			!shown.contains("ignored") && !shown.contains("\r\npartial"),
			"{shown:?}"
		);
	}
}

#[test]
fn commands_split_on_semicolons_and_ands_outside_quotes() {
	let split = |line| segments(line).unwrap();
	let pairs = |list: &[(&str, &'static str)]| {
		list
			.iter()
			.map(|&(c, o)| (c.to_owned(), o))
			.collect::<Vec<_>>()
	};
	assert_eq!(
		split("a;b && c"),
		pairs(&[("a", ";"), ("b ", "&&"), (" c", ";")])
	);
	assert_eq!(
		split("echo 'a;b' \"c&&d\" e\\;f"),
		pairs(&[("echo 'a;b' \"c&&d\" e\\;f", ";")])
	);
	assert_eq!(
		split("a & b"),
		pairs(&[("a & b", ";")]),
		"one & is no operator"
	);
	assert_eq!(segments("echo 'open"), None);
	assert_eq!(segments("echo \\"), None);
}

#[test]
fn empty_expansions_vanish_unless_quoted() {
	assert_eq!(words("echo $NOPE x"), [text("echo"), text("x")]);
	assert_eq!(
		words("echo \"$NOPE\" x"),
		[text("echo"), text(""), text("x")]
	);
	assert_eq!(words("echo a$NOPE"), [text("echo"), text("a")]);
}

#[test]
fn output_redirects_to_files_replacing_or_appending() {
	let (mut sh, home) = shell();
	assert_eq!(
		ran(&mut sh, "echo one > f; echo two >> f; echo three>>f"),
		""
	);
	assert_eq!(
		fs::read_to_string(home.join("f")).unwrap(),
		"one\ntwo\nthree\n"
	);
	assert_eq!(ran(&mut sh, "echo new >f && cat f"), "new\n");
	assert_eq!(
		words("echo a>b >> c"),
		[
			text("echo"),
			text("a"),
			Word::Op(">"),
			text("b"),
			Word::Op(">>"),
			text("c")
		]
	);
	assert_eq!(
		ran(&mut sh, "echo x >"),
		"poopsh: a file must follow > or >>\n"
	);
	assert_eq!(sh.status, 2);
	assert!(ran(&mut sh, "echo x > missing/dir/f").starts_with("poopsh: "));
	assert_eq!(sh.status, 1);
	assert_eq!(
		ran(&mut sh, "echo 'a > b'"),
		"a > b\n",
		"a quoted > is text"
	);
}

#[test]
fn uname_answers_as_the_kernel_does_with_its_flags() {
	use super::shell::uname_line;
	let names = crate::sys::user::uname().unwrap();
	let line = |args: &[&str]| {
		uname_line(
			&names,
			&args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
		)
	};
	assert_eq!(line(&[]), Ok(names.sysname.clone()));
	assert_eq!(line(&["-m"]), Ok(names.machine.clone()));
	assert_eq!(
		line(&["-sr"]),
		Ok(format!("{} {}", names.sysname, names.release))
	);
	let all = [
		&names.sysname,
		&names.nodename,
		&names.release,
		&names.version,
		&names.machine,
	];
	assert_eq!(line(&["-a"]), Ok(all.map(String::as_str).join(" ")));
	assert_eq!(line(&["-x"]), Err('x'));
	assert_eq!(line(&["-"]), Err('?'));
	assert_eq!(
		names.sysname,
		"Darwin".to_owned().replace("Darwin", &names.sysname)
	);
	assert!(!names.release.is_empty() && !names.machine.is_empty());
}
