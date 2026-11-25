//! The in-process device: the line discipline's editing keys, the shell's
//! words and built-ins, and the device loop end to end through pipes.
use super::{
	DATA_DIRS, Size, command_path, data_dir,
	line::{Discipline, Event, MAX_LINE, output},
	run,
	shell::{Shell, Word, segments, split},
	start_directory, terminfo_dir,
};
use std::{
	fs,
	io::Write,
	path::{Path, PathBuf},
	sync::{
		Arc, Mutex,
		atomic::{AtomicUsize, Ordering},
	},
};

/// A screen for the device loop: what it shows, kept for the test. It
/// keeps the first `SCREEN_LIMIT` bytes and then the last ones, so a
/// program that never stops printing cannot fill memory.
#[derive(Clone, Default)]
pub(super) struct Screen(pub Arc<Mutex<Vec<u8>>>);

/// Most bytes a test screen keeps.
pub(super) const SCREEN_LIMIT: usize = 1 << 20;

impl Write for Screen {
	fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
		let mut shown = self.0.lock().unwrap();
		shown.extend_from_slice(bytes);
		if shown.len() > SCREEN_LIMIT {
			// Keep the start and the most recent half.
			let cut = shown.len() - SCREEN_LIMIT / 2;
			shown.drain(SCREEN_LIMIT / 2..cut);
		}
		Ok(bytes.len())
	}
	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

impl Screen {
	pub(super) fn shown(&self) -> String {
		String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
	}
}

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
fn cd_moves_the_shell_and_its_prompt_and_pwd_says_where() {
	let (mut sh, home) = shell();
	fs::create_dir_all(home.join("a/b")).unwrap();
	assert_eq!(ran(&mut sh, "cd a; pwd"), format!("{}/a\n", home.display()));
	assert_eq!(sh.prompt(), "\x1b[1;34m~/a\x1b[0m $ ");
	assert_eq!(
		ran(&mut sh, "cd b && cd .. && pwd"),
		format!("{}/a\n", home.display())
	);
	assert_eq!(ran(&mut sh, "cd; pwd"), format!("{}\n", home.display()));
	assert_eq!(sh.prompt(), "\x1b[1;34m~\x1b[0m $ ");
	fs::write(home.join("file"), "").unwrap();
	assert!(ran(&mut sh, "cd file").contains("not a directory"));
	assert_eq!(sh.cwd, home, "a failed cd stays put");
}

#[test]
fn failures_set_the_status_and_stop_and_chains() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "nope"), "poopsh: nope: command not found\n");
	assert_eq!(sh.status, 127);
	assert_eq!(ran(&mut sh, "echo $?"), "127\n");
	assert_eq!(ran(&mut sh, "false && echo no; echo yes"), "yes\n");
	assert!(ran(&mut sh, "cd missing").starts_with("cd: "));
	assert_eq!(sh.status, 1);
	assert_eq!(ran(&mut sh, "true && echo ok"), "ok\n");
}

#[test]
fn the_small_built_ins_do_what_their_names_say() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "echo -n a b"), "a b");
	assert_eq!(ran(&mut sh, "export A=1 B=two; echo $A$B"), "1two\n");
	assert_eq!(sh.env["A"], "1");
	assert_eq!(ran(&mut sh, "unset A; echo [$A]"), "[]\n");
	assert_eq!(ran(&mut sh, "clear"), "\x1b[H\x1b[2J\x1b[3J");
	let help = ran(&mut sh, "help");
	assert!(help.contains(" cd ") && !help.contains(" ls "), "{help}");
	assert!(
		help.contains("$PATH"),
		"says where the rest come from: {help}"
	);
	let exit = sh.run("exit 4");
	assert!(exit.exit && sh.status == 4);
}

#[test]
fn the_device_shows_a_prompt_answers_lines_and_ends_on_exit() {
	let (shell, _) = shell();
	let screen = Screen::default();
	run(
		shell,
		Arc::new(Size::new(80, 24)),
		&b"echo hi\rnope\rexit\r"[..],
		screen.clone(),
	);
	let shown = screen.shown();
	assert!(shown.starts_with("Poopterm in-process shell."), "{shown:?}");
	assert!(
		shown.contains("e\x63ho hi\r\nhi\r\n"),
		"echoed then answered, CR LF: {shown:?}"
	);
	assert!(shown.contains("poopsh: nope: command not found\r\n"));
	assert!(!shown.ends_with("$ "), "no prompt after exit");
}

#[test]
fn a_new_shell_and_help_say_how_to_get_more_programs() {
	let (device, _) = shell();
	let screen = Screen::default();
	run(
		device,
		Arc::new(Size::new(80, 24)),
		&b"exit\r"[..],
		screen.clone(),
	);
	let shown = screen.shown();
	let notice = super::MORE_PROGRAMS.replace('\n', "\r\n");
	assert!(
		shown.starts_with(&format!(
			"Poopterm in-process shell. Type `help` for its built-in commands.\r\n{notice}"
		)),
		"before the first prompt: {shown:?}"
	);
	assert!(super::MORE_PROGRAMS.contains("pkg install NAME"));
	assert!(
		super::MORE_PROGRAMS.contains("jit enable"),
		"a phone needs JIT for them"
	);
	let (mut sh, _) = shell();
	assert!(ran(&mut sh, "help").ends_with(super::MORE_PROGRAMS));
}

#[test]
fn a_missing_command_a_package_provides_suggests_installing_it() {
	let (mut sh, _) = shell();
	assert_eq!(
		ran(&mut sh, "cmatrix"),
		"poopsh: cmatrix: command not found\n",
		"no list fetched yet: no suggestion"
	);
	let root = crate::pkg::root(&sh.support);
	fs::create_dir_all(&root).unwrap();
	let hash = "ab".repeat(32);
	fs::write(
		root.join("index"),
		format!("# packages\ncmatrix 2.0 {hash} 10 ios-cmatrix-2.0.tar.gz Falling characters\n"),
	)
	.unwrap();
	assert_eq!(
		ran(&mut sh, "cmatrix; echo $?"),
		"poopsh: cmatrix: command not found\npoopsh: `pkg install cmatrix` adds it\n127\n"
	);
	assert_eq!(
		ran(&mut sh, "nope"),
		"poopsh: nope: command not found\n",
		"only names the list has"
	);
	fs::write(root.join("index"), "not an index\n").unwrap();
	assert_eq!(
		ran(&mut sh, "cmatrix"),
		"poopsh: cmatrix: command not found\n",
		"a list it cannot read suggests nothing"
	);
}

#[test]
fn the_device_ends_on_end_of_file_or_a_closed_input() {
	for input in [&b"\x04ignored\r"[..], &b"echo partial"[..]] {
		let (shell, _) = shell();
		let screen = Screen::default();
		run(shell, Arc::new(Size::new(80, 24)), input, screen.clone());
		let shown = screen.shown();
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
	assert_eq!(ran(&mut sh, "echo new >f && echo done"), "done\n");
	assert_eq!(fs::read_to_string(home.join("f")).unwrap(), "new\n");
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
		"poopsh: a file must follow <, > or >>\n"
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
fn a_test_screen_stays_bounded_keeping_the_start_and_the_end() {
	let mut screen = Screen::default();
	screen.write_all(b"start").unwrap();
	for _ in 0..3 * SCREEN_LIMIT / 4096 {
		screen.write_all(&[b'y'; 4096]).unwrap();
	}
	screen.write_all(b"end").unwrap();
	let shown = screen.shown();
	assert!(shown.len() <= SCREEN_LIMIT + 4096, "{}", shown.len());
	assert!(shown.starts_with("start") && shown.ends_with("end"));
}

#[test]
fn a_size_reads_back_as_set_and_clamps_to_what_a_terminal_holds() {
	let size = Size::new(80, 24);
	assert_eq!(size.get(), (80, 24));
	size.set(200, 50);
	assert_eq!(size.get(), (200, 50));
	size.set(100_000, 0);
	assert_eq!(size.get(), (u16::MAX, 0));
}

#[test]
fn a_size_tells_whoever_listens_of_each_change_only() {
	let size = Size::new(80, 24);
	let calls = Arc::new(AtomicUsize::new(0));
	let counted = calls.clone();
	size.on_resize(Box::new(move || {
		counted.fetch_add(1, Ordering::SeqCst);
	}));
	size.on_resize(Box::new(|| panic!("only the first listener stays")));
	size.set(80, 24);
	assert_eq!(calls.load(Ordering::SeqCst), 0, "no change, no call");
	size.set(81, 24);
	size.set(81, 25);
	assert_eq!(calls.load(Ordering::SeqCst), 2);
	assert_eq!(format!("{size:?}"), "Size((81, 25))");
}

#[test]
fn programs_come_from_beside_the_app_then_from_packages_unless_chosen() {
	let home = PathBuf::from("/home/me");
	let exe = PathBuf::from("/App.app");
	assert_eq!(
		command_path(Some(&exe), &home, None),
		"/App.app/Frameworks:/home/me/Library/poopterm/packages/bin"
	);
	assert_eq!(
		command_path(None, &home, None),
		"/home/me/Library/poopterm/packages/bin"
	);
	assert_eq!(command_path(Some(&exe), &home, Some("/x:/y")), "/x:/y");
}

#[test]
fn pkg_is_a_built_in_that_installs_under_the_users_library() {
	let (mut sh, home) = shell();
	let help = ran(&mut sh, "help");
	assert!(help.contains(" pkg "), "{help}");
	let usage = ran(&mut sh, "pkg");
	assert!(usage.contains("install NAME"), "{usage}");
	assert_eq!(sh.status, 0);
	// Before one is set, Poopterm's own for this platform (none fetched).
	let own = ran(&mut sh, "pkg source");
	match crate::pkg::source::default() {
		Some(default) => assert_eq!(own, format!("{default} (the default)\n")),
		None => assert!(own.contains("pkg source URL"), "{own}"),
	}
	let repo = home.join("repo");
	let set = ran(
		&mut sh,
		&format!("pkg source file://{}/index", repo.display()),
	);
	assert!(set.contains("packages now come from"), "{set}");
	let saved = home.join("Library/poopterm/packages/source");
	assert_eq!(
		fs::read_to_string(saved).unwrap(),
		format!("file://{}/index", repo.display())
	);
	fs::create_dir_all(&repo).unwrap();
	fs::write(repo.join("index"), "# empty\n").unwrap();
	assert_eq!(ran(&mut sh, "pkg update"), "0 packages available\n");
}

#[test]
fn the_environment_can_point_pkg_elsewhere() {
	let (mut sh, home) = shell();
	let repo = home.join("other");
	fs::create_dir_all(&repo).unwrap();
	fs::write(repo.join("index"), "# none\n").unwrap();
	let url = format!("file://{}/index", repo.display());
	ran(&mut sh, &format!("export POOPTERM_PKG_SOURCE={url}"));
	assert_eq!(ran(&mut sh, "pkg source"), format!("{url}\n"));
	assert_eq!(ran(&mut sh, "pkg update"), "0 packages available\n");
	let saved = home.join("Library/poopterm/packages/source");
	assert!(!saved.exists(), "the environment's source is not saved");
}

#[test]
fn programs_see_who_they_run_for_and_which_terminal_runs_them() {
	let (sh, _) = shell();
	let account = crate::sys::user::login(crate::sys::user::uid());
	let user = super::shell::user_name(std::env::var("USER").ok(), account, cfg!(target_os = "ios"));
	assert_eq!(sh.env.get("USER"), user.as_ref());
	assert_eq!(sh.env.get("LOGNAME"), user.as_ref());
	assert_eq!(
		sh.env["TERM_PROGRAM"], "poopterm",
		"as the desktop app's shells see"
	);
	assert_eq!(sh.env["TERM_PROGRAM_VERSION"], env!("CARGO_PKG_VERSION"));
	assert_eq!(sh.env["COLORTERM"], "truecolor");
}

#[test]
fn the_user_is_the_apps_own_then_the_accounts_then_mobile_on_ios() {
	use super::shell::user_name;
	let (process, account) = (Some("app".to_owned()), Some("account".to_owned()));
	assert_eq!(user_name(process.clone(), account.clone(), true), process);
	assert_eq!(
		user_name(Some(String::new()), account.clone(), false),
		account,
		"empty is none"
	);
	assert_eq!(user_name(None, account.clone(), true), account);
	assert_eq!(
		user_name(None, None, true).as_deref(),
		Some("mobile"),
		"iOS's one account"
	);
	assert_eq!(user_name(None, None, false), None);
}

#[test]
fn or_runs_the_next_command_only_after_a_failure() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "false || echo rescued"), "rescued\n");
	assert_eq!(sh.status, 0);
	assert_eq!(ran(&mut sh, "true || echo never"), "");
	assert_eq!(
		ran(&mut sh, "nope || echo $?"),
		"poopsh: nope: command not found\n127\n"
	);
}

#[test]
fn and_and_or_chain_left_to_right_as_a_posix_shell_does() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "false && echo a || echo b"), "b\n");
	assert_eq!(ran(&mut sh, "true && echo a || echo b"), "a\n");
	assert_eq!(
		ran(&mut sh, "true || echo a && echo b"),
		"b\n",
		"skipping a keeps true's status"
	);
	assert_eq!(ran(&mut sh, "false || false || echo third"), "third\n");
	assert_eq!(
		ran(&mut sh, "false || echo x; echo after"),
		"x\nafter\n",
		"; ends the chain"
	);
}

#[test]
fn or_is_split_outside_quotes_and_apart_from_pipes() {
	let pairs = |list: &[(&str, &'static str)]| {
		list
			.iter()
			.map(|&(c, o)| (c.to_owned(), o))
			.collect::<Vec<_>>()
	};
	assert_eq!(
		segments("a || b | c"),
		Some(pairs(&[("a ", "||"), (" b | c", ";")])),
		"one | is a pipe, inside the second command"
	);
	assert_eq!(
		segments("echo 'a||b' \"c||d\""),
		Some(pairs(&[("echo 'a||b' \"c||d\"", ";")]))
	);
	assert_eq!(
		segments("a \\|| b"),
		Some(pairs(&[("a \\|| b", ";")])),
		"an escaped | is a word, and the one after it a pipe"
	);
}

#[test]
fn programs_are_told_not_to_start_a_pager_or_background_work() {
	let (sh, _) = shell();
	assert_eq!(
		sh.env["PAGER"], "cat",
		"a pager would be a second program at once"
	);
	assert_eq!(sh.env["GIT_PAGER"], "cat");
	// git reads GIT_CONFIG_COUNT pairs as configuration.
	let count: usize = sh.env["GIT_CONFIG_COUNT"].parse().unwrap();
	let pairs: Vec<(&str, &str)> = (0..count)
		.map(|n| {
			let key = sh.env[&format!("GIT_CONFIG_KEY_{n}")].as_str();
			(key, sh.env[&format!("GIT_CONFIG_VALUE_{n}")].as_str())
		})
		.collect();
	assert!(pairs.contains(&("maintenance.auto", "false")), "{pairs:?}");
	assert!(pairs.contains(&("gc.auto", "0")), "{pairs:?}");
}

#[test]
fn the_defaults_can_be_replaced_like_any_variable() {
	let (mut sh, _) = shell();
	assert_eq!(ran(&mut sh, "export PAGER=less; echo $PAGER"), "less\n");
}

#[test]
fn whoami_names_the_user_programs_run_for() {
	let (mut sh, _) = shell();
	sh.env.insert("USER".into(), "mobile".into());
	assert_eq!(ran(&mut sh, "whoami"), "mobile\n");
	assert_eq!(ran(&mut sh, "whoami && echo ok"), "mobile\nok\n");
	sh.env.remove("USER");
	assert_eq!(
		ran(&mut sh, "whoami || echo failed"),
		"whoami: no user name\nfailed\n"
	);
	assert!(
		ran(&mut sh, "help").contains(" whoami"),
		"listed as a built-in"
	);
}

#[test]
fn jit_is_a_built_in_that_reports_where_code_can_run() {
	let (mut sh, _) = shell();
	let shown = ran(&mut sh, "jit");
	assert!(
		shown.contains("JIT: ") || shown.starts_with("jit: "),
		"{shown}"
	);
	assert_eq!(sh.status, 0);
	assert!(ran(&mut sh, "jit nonsense").starts_with("usage: jit") || !cfg!(target_vendor = "apple"));
	assert!(
		ran(&mut sh, "help").contains(" jit "),
		"listed as a built-in"
	);
}

#[test]
fn the_terminfo_beside_the_app_is_used_unless_the_user_chose_one() {
	let dir = std::env::temp_dir().join(format!("poopterm-terminfo-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	assert_eq!(terminfo_dir(Some(&dir), None), None, "none bundled");
	fs::create_dir_all(dir.join("terminfo/78")).unwrap();
	assert_eq!(terminfo_dir(Some(&dir), None), Some(dir.join("terminfo")));
	assert_eq!(
		terminfo_dir(Some(&dir), Some("")),
		Some(dir.join("terminfo")),
		"empty is unset"
	);
	assert_eq!(
		terminfo_dir(Some(&dir), Some("/mine")),
		None,
		"the user's stays"
	);
	assert_eq!(terminfo_dir(None, None), None, "no executable directory");
	fs::write(dir.join("terminfo2"), "a file").unwrap();
	assert_eq!(
		terminfo_dir(Some(&dir.join("terminfo2")), None),
		None,
		"a file is no directory"
	);
	let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_shell_starts_where_asked_inside_home_and_in_home_otherwise() {
	let home = PathBuf::from("/app/Documents");
	assert_eq!(
		start_directory(home.join("project"), &home),
		home.join("project")
	);
	assert_eq!(start_directory(home.clone(), &home), home);
	assert_eq!(
		start_directory(PathBuf::from("/app"), &home),
		home,
		"the container's root"
	);
	assert_eq!(start_directory(PathBuf::from("/"), &home), home);
	assert_eq!(
		start_directory(PathBuf::from("/app/Documents-old"), &home),
		home,
		"a sibling sharing a prefix is outside"
	);
}

#[test]
fn packages_live_under_support_while_home_is_the_users() {
	let (mut sh, home) = shell();
	let support = home.join("support");
	sh.support = support.clone();
	assert_eq!(sh.home, home);
	let shown = ran(&mut sh, "pkg list");
	assert!(shown.contains("no package") || !shown.is_empty(), "{shown}");
	let installed = crate::pkg::root(&support);
	assert!(installed.starts_with(&support) && !installed.starts_with(sh.home.join("Library")));
}

/// Where tcc's folder is, as the shell looks for `TCCDIR`.
fn tcc(exe_dir: Option<&Path>, support: &Path, chosen: Option<&str>) -> Option<PathBuf> {
	assert_eq!(DATA_DIRS[0].0, "TCCDIR");
	data_dir(exe_dir, support, DATA_DIRS[0].1, chosen)
}

#[test]
fn tcc_finds_its_folder_in_the_app_then_among_installed_packages() {
	let dir = std::env::temp_dir().join(format!("poopterm-tccdir-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	let (app, support) = (dir.join("App.app"), dir.join("support"));
	fs::create_dir_all(&app).unwrap();
	fs::create_dir_all(&support).unwrap();
	assert_eq!(tcc(Some(&app), &support, None), None, "none anywhere");
	let installed = support.join("Library/poopterm/packages/share/tcc");
	fs::create_dir_all(installed.join("include")).unwrap();
	assert_eq!(
		tcc(Some(&app), &support, None),
		Some(installed.clone()),
		"the package's"
	);
	fs::create_dir_all(app.join("tcc/include")).unwrap();
	assert_eq!(
		tcc(Some(&app), &support, None),
		Some(app.join("tcc")),
		"the app's comes first"
	);
	assert_eq!(
		tcc(None, &support, None),
		Some(installed),
		"no app folder to look in"
	);
	assert_eq!(
		tcc(Some(&app), &support, Some("/mine")),
		None,
		"the user's stays"
	);
	assert_eq!(
		tcc(Some(&app), &support, Some("")),
		Some(app.join("tcc")),
		"empty is unset"
	);
	fs::create_dir_all(dir.join("App2.app/tcc")).unwrap();
	assert_eq!(
		tcc(Some(&dir.join("App2.app")), &dir.join("none"), None),
		None,
		"a folder without include/ is not it"
	);
	let _ = fs::remove_dir_all(&dir);
}

#[test]
fn figlet_finds_its_fonts_where_tcc_finds_its_headers() {
	let dir = std::env::temp_dir().join(format!("poopterm-figletdir-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	let (app, support) = (dir.join("App.app"), dir.join("support"));
	let fonts = DATA_DIRS
		.iter()
		.find(|(variable, _)| *variable == "FIGLET_FONTDIR")
		.expect("the shell sets FIGLET_FONTDIR")
		.1;
	fs::create_dir_all(app.join("figlet")).unwrap();
	assert_eq!(
		data_dir(Some(&app), &support, fonts, None),
		None,
		"a folder without the standard font is not it"
	);
	let installed = support.join("Library/poopterm/packages/share/figlet");
	fs::create_dir_all(&installed).unwrap();
	fs::write(installed.join("standard.flf"), "flf2a").unwrap();
	assert_eq!(
		data_dir(Some(&app), &support, fonts, None),
		Some(installed),
		"the package's"
	);
	fs::write(app.join("figlet/standard.flf"), "flf2a").unwrap();
	assert_eq!(
		data_dir(Some(&app), &support, fonts, None),
		Some(app.join("figlet")),
		"the app's comes first"
	);
	assert_eq!(
		data_dir(Some(&app), &support, fonts, Some("/my/fonts")),
		None,
		"the user's stays"
	);
	assert_eq!(
		DATA_DIRS.map(|(variable, _)| variable),
		["TCCDIR", "FIGLET_FONTDIR"]
	);
	let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unquoted_patterns_expand_to_sorted_names_and_quoted_ones_do_not() {
	let (mut sh, home) = shell();
	for file in ["b.o", "a.o", "a.c", ".h.o", "star*.o"] {
		fs::write(home.join(file), "").unwrap();
	}
	sh.env.insert("PATTERN".into(), "*.c".into());
	let say = |sh: &mut Shell, line: &str| String::from_utf8(sh.run(line).output).unwrap();
	assert_eq!(say(&mut sh, "echo *.o"), "a.o b.o star*.o\n");
	assert_eq!(say(&mut sh, "echo '*.o' \"*.o\" \\*.o"), "*.o *.o *.o\n");
	assert_eq!(say(&mut sh, "echo a.? [ab].c"), "a.c a.o a.c\n");
	assert_eq!(
		say(&mut sh, "echo *.h"),
		"*.h\n",
		"no match keeps the pattern"
	);
	assert_eq!(
		say(&mut sh, "echo $PATTERN"),
		"*.c\n",
		"an expansion is not a pattern"
	);
	assert_eq!(say(&mut sh, "echo star\\*.o"), "star*.o\n");
	assert_eq!(say(&mut sh, "echo .*.o"), ".h.o\n");
	assert_eq!(say(&mut sh, "echo *.o > list; echo list*"), "list\n");
	assert_eq!(
		fs::read_to_string(home.join("list")).unwrap(),
		"a.o b.o star*.o\n"
	);
}

#[test]
fn name_equals_value_words_set_variables_for_the_command_or_for_good() {
	let (mut sh, _) = shell();
	let say = |sh: &mut Shell, line: &str| String::from_utf8(sh.run(line).output).unwrap();
	assert_eq!(say(&mut sh, "GREETING=hi; echo $GREETING"), "hi\n");
	assert_eq!(say(&mut sh, "A=1 B=two; echo $A $B"), "1 two\n");
	sh.env.insert("KEEP".into(), "old".into());
	assert_eq!(
		say(&mut sh, "KEEP=new echo $KEEP"),
		"old\n",
		"expanded before the command runs"
	);
	assert_eq!(sh.env["KEEP"], "old", "restored after the command");
	assert_eq!(
		say(&mut sh, "TEMP=x echo done; echo [$TEMP]"),
		"done\n[]\n",
		"removed after"
	);
	assert_eq!(
		say(&mut sh, "echo A=1"),
		"A=1\n",
		"after the command it is a plain word"
	);
	assert_eq!(
		say(&mut sh, "1A=x"),
		"poopsh: 1A=x: command not found\n",
		"not a name"
	);
	assert_eq!(say(&mut sh, "=x"), "poopsh: =x: command not found\n");
}
