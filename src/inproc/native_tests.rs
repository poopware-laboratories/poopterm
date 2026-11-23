//! Real programs in the in-process shell: finding them on `$PATH`, bounds
//! on what they print, and Apple's own commands run through the shell
//! when `ios/commands/build.sh macosx` has built them.
use std::{
	fs,
	io::{self, Read as _, Write as _},
	path::PathBuf,
	sync::Arc,
	thread,
};

use super::{MAX_OUTPUT, Run, drain, find, run};
use crate::inproc::shell::Shell;

fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"poopterm-native-{name}-{}-{:?}",
		std::process::id(),
		std::thread::current().id()
	));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir.canonicalize().unwrap()
}

#[test]
fn a_command_is_the_first_file_of_its_name_on_the_path() {
	let dir = scratch("find");
	for sub in ["a", "b", "c"] {
		fs::create_dir(dir.join(sub)).unwrap();
	}
	fs::write(dir.join("b/tool"), "").unwrap();
	fs::write(dir.join("c/tool"), "").unwrap();
	fs::create_dir(dir.join("a/tool")).unwrap();
	let path = format!("{0}/a::{0}/missing:{0}/b:{0}/c", dir.display());
	assert_eq!(
		find("tool", Some(&path), &dir),
		Some(dir.join("b/tool")),
		"a directory is skipped"
	);
	assert_eq!(find("other", Some(&path), &dir), None);
	assert_eq!(find("tool", None, &dir), None, "no $PATH, no commands");
	assert_eq!(find("", Some(&path), &dir), None);
}

#[test]
fn a_name_with_a_slash_is_a_path_from_the_current_directory() {
	let dir = scratch("slash");
	fs::create_dir(dir.join("bin")).unwrap();
	fs::write(dir.join("bin/tool"), "").unwrap();
	assert_eq!(find("bin/tool", None, &dir), Some(dir.join("bin/tool")));
	assert_eq!(
		find("./bin/tool", Some("/nowhere"), &dir),
		Some(dir.join("./bin/tool"))
	);
	let absolute = dir.join("bin/tool").display().to_string();
	assert_eq!(
		find(&absolute, None, &PathBuf::from("/")),
		Some(dir.join("bin/tool"))
	);
	assert_eq!(find("bin", None, &dir), None, "a directory is no command");
}

#[test]
fn bundled_frameworks_resolve_before_plain_package_commands() {
	let dir = scratch("framework");
	let framework = dir.join("uname.framework");
	fs::create_dir(&framework).unwrap();
	fs::write(framework.join("uname"), "framework").unwrap();
	fs::write(dir.join("uname"), "package").unwrap();
	assert_eq!(
		find("uname", Some(dir.to_str().unwrap()), &dir),
		Some(framework.join("uname"))
	);
	assert_eq!(find("missing", Some(dir.to_str().unwrap()), &dir), None);
}

#[test]
fn what_is_not_a_command_library_fails_with_the_loaders_reason() {
	let dir = scratch("bad");
	fs::write(dir.join("fake"), "#!/bin/sh\necho hi\n").unwrap();
	let env = Default::default();
	let argv = ["fake".to_owned()];
	let ran = run(
		&dir.join("fake"),
		&Run {
			argv: &argv,
			env: &env,
			cwd: &dir,
			tty: None,
			merge: true,
			terminal: None,
			input: None,
		},
	);
	let error = ran.unwrap_err();
	assert!(error.contains("fake"), "{error}");
}

#[test]
fn arguments_with_nul_bytes_are_refused_before_loading() {
	let dir = scratch("nul");
	let env = Default::default();
	let argv = ["x".to_owned(), "a\0b".to_owned()];
	let error = run(
		&dir.join("never-opened"),
		&Run {
			argv: &argv,
			env: &env,
			cwd: &dir,
			tty: None,
			merge: true,
			terminal: None,
			input: None,
		},
	)
	.unwrap_err();
	assert!(error.contains("NUL"), "{error}");
}

#[test]
fn draining_keeps_a_bounded_prefix_and_reads_to_the_end() {
	let mut source = std::io::repeat(b'y').take(MAX_OUTPUT as u64 + 12345);
	let kept = drain(&mut source);
	assert_eq!(kept.len(), MAX_OUTPUT);
	assert_eq!(
		source.limit(),
		0,
		"the rest was read, so the writer never blocks"
	);
	assert_eq!(drain(&b"short"[..]), b"short");
}

/// Where `ios/commands/build.sh macosx` puts Apple's commands built for
/// this Mac.
const COMMANDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/target/ios-commands/macosx");

/// The commands, or None (with a note) if they are not built.
fn commands() -> Option<String> {
	if cfg!(target_os = "macos") && PathBuf::from(COMMANDS).join("uname").is_file() {
		Some(COMMANDS.to_owned())
	} else {
		eprintln!("skipped: run ios/commands/build.sh macosx to test real commands");
		None
	}
}

/// Commands write to the process's standard output while they run, and so
/// would the test harness, reporting other tests: they run in a process of
/// their own, one after another.
#[test]
fn real_commands_run_through_the_shell() {
	if commands().is_none() {
		return;
	}
	let output = std::process::Command::new(std::env::current_exe().unwrap())
		.args([
			"--exact",
			"inproc::native::tests::real_commands_alone",
			"--ignored",
		])
		.args(["--test-threads=1", "--nocapture"])
		.output()
		.unwrap();
	let shown = String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
	assert!(output.status.success(), "{shown}");
	assert!(shown.contains("1 passed"), "{shown}");
}

#[test]
#[ignore = "run by real_commands_run_through_the_shell, in a process of its own"]
fn real_commands_alone() {
	uname_is_the_real_one();
	files_are_made_listed_read_and_removed_by_the_real_tools();
	each_run_starts_with_fresh_globals();
	ls_lays_out_columns_for_the_terminal_but_one_per_line_into_a_file();
	errors_still_reach_the_screen_when_output_goes_to_a_file();
	programs_see_the_shells_environment_and_directory();
	text_tools_work_on_files();
	pipes_feed_each_stage_what_the_one_before_printed();
	a_command_pkg_installed_runs_from_the_path();
	the_new_text_tools_match_the_systems();
	digests_archives_and_encodings_match_the_systems();
	xargs_and_find_start_other_commands();
	programs_read_typed_lines_and_print_as_they_run();
	interrupting_ends_a_program_and_the_rest_of_its_line();
	the_keyboard_goes_back_to_the_shell_after_a_program();
	stty_reads_and_changes_the_devices_settings();
	less_pages_a_file_and_a_pipe_reading_keys_from_dev_tty();
	awk_and_diff_match_the_systems();
	// Last: from here on every command goes through our own loader.
	real_commands_run_through_our_own_loader();
}

/// The commands as a device without dyld's blessing runs them: loaded by
/// our loader (`jit::loader`), each run with its globals as loaded.
fn real_commands_run_through_our_own_loader() {
	let Some((mut sh, home)) = real_shell("loader") else {
		return;
	};
	super::FORCE_LOADER.store(true, std::sync::atomic::Ordering::Relaxed);
	assert_eq!(ran(&mut sh, "uname -s"), system("/usr/bin/uname", &["-s"]));
	fs::write(home.join("words"), "pear\napple\nfig\n").unwrap();
	assert_eq!(
		ran(&mut sh, "sort words | sed s/p/P/g"),
		"aPPle\nfig\nPear\n"
	);
	assert_eq!(
		ran(&mut sh, "sort words | sed s/p/P/g"),
		"aPPle\nfig\nPear\n",
		"twice: fresh globals"
	);
	assert_eq!(
		ran(&mut sh, "awk '{ n += length($0) } END { print n }' words"),
		"12\n"
	);
	assert_eq!(
		ran(&mut sh, "md5 -q -s abc"),
		"900150983cd24fb0d6963f7d28e17f72\n"
	);
	// Without macOS's AppleDouble copies of extended attributes (._words).
	sh.env.insert("COPYFILE_DISABLE".into(), "1".into());
	assert_eq!(
		ran(&mut sh, "tar -cf w.tar words && tar -tf w.tar"),
		"words\n"
	);
	assert_eq!(
		ran(&mut sh, "echo words | xargs cat | wc -l | tr -d ' '"),
		"3\n",
		"children too"
	);
	let keys: &[&[u8]] = &[
		b"jot 3 > words\r",
		b"less words\r",
		b"q",
		b"echo after\r",
		b"exit\r",
	];
	let shown = device("loader-less", keys).unwrap();
	assert!(shown.contains("1\r\n2\r\n3"), "less paged it: {shown:?}");
	assert!(shown.contains("echo after\r\nafter\r\n"));
}

fn stty_reads_and_changes_the_devices_settings() {
	let keys: &[&[u8]] = &[
		b"stty size\r",
		b"stty -echo\r",
		b"stty -a | grep -c -w -- -echo\r",
		b"stty echo; stty -a | grep -c -w -- -echo\r",
		b"exit\r",
	];
	let Some(shown) = device("stty", keys) else {
		return;
	};
	assert!(shown.contains("stty size\r\n24 80\r\n"), "{shown:?}");
	assert!(
		shown.contains("grep -c -w -- -echo\r\n1\r\n"),
		"stty -echo lasts: {shown:?}"
	);
	assert!(
		shown.contains("-echo\r\n0\r\n"),
		"until stty echo: {shown:?}"
	);
}

fn less_pages_a_file_and_a_pipe_reading_keys_from_dev_tty() {
	let keys: &[&[u8]] = &[
		b"jot 100 > numbers\r",
		b"less numbers\r",
		b" ",
		b"q",
		b"jot 3 | less\r",
		b"q",
		b"echo back\r",
		b"exit\r",
	];
	let Some(shown) = device("less", keys) else {
		return;
	};
	assert!(shown.contains("23\r\n"), "the first page: {shown:?}");
	assert!(shown.contains("40"), "space showed the next page");
	assert!(!shown.contains("\r\n100\r\n"), "q quit before the end");
	assert!(
		shown.contains("echo back\r\nback\r\n"),
		"the shell had the keys back"
	);
}

fn awk_and_diff_match_the_systems() {
	let Some((mut sh, home)) = real_shell("awk-diff") else {
		return;
	};
	fs::write(home.join("a"), "one:1\ntwo:2\nthree:3\n").unwrap();
	fs::write(home.join("b"), "one:1\nTWO:2\nthree:3\nfour:4\n").unwrap();
	let (a, b) = (
		home.join("a").display().to_string(),
		home.join("b").display().to_string(),
	);
	for (line, program, args) in [
		(
			"awk -F: '{ s += $2 } END { print s }' b",
			"/usr/bin/awk",
			vec!["-F:", "{ s += $2 } END { print s }", &b],
		),
		("awk 'NR % 2' a", "/usr/bin/awk", vec!["NR % 2", &a]),
		("diff a b", "/usr/bin/diff", vec![&a, &b]),
		("diff -e a b", "/usr/bin/diff", vec!["-e", &a, &b]),
	] {
		let expected = system(program, &args).replace(&a, "a").replace(&b, "b");
		assert_eq!(ran(&mut sh, line), expected, "{line}");
	}
	assert_eq!(ran(&mut sh, "diff a a && echo same"), "same\n");
	assert_eq!(ran(&mut sh, "cmp a b"), "a b differ: char 7, line 2\n");
	assert_eq!(ran(&mut sh, "cmp a a && echo same"), "same\n");
}

/// The device loop on real commands: keys come from `keys`, a line at a
/// time with a pause between, as a person types, and the screen is kept.
fn device(name: &'static str, keys: &'static [&'static [u8]]) -> Option<String> {
	let (shell, _) = real_shell(name)?;
	let (input, mut typing) = io::pipe().unwrap();
	let screen = crate::inproc::tests::Screen::default();
	let shown = screen.clone();
	let typist = thread::spawn(move || {
		for chunk in keys {
			thread::sleep(std::time::Duration::from_millis(150));
			if typing.write_all(chunk).is_err() {
				return;
			}
		}
	});
	crate::sys::signal::quiet_job_signals();
	crate::inproc::run(
		shell,
		Arc::new(crate::inproc::Size::new(80, 24)),
		input,
		screen,
	);
	typist.join().unwrap();
	Some(shown.shown())
}

fn programs_read_typed_lines_and_print_as_they_run() {
	let Some(shown) = device("typed", &[b"cat\r", b"one\r", b"two\r", b"\x04", b"exit\r"]) else {
		return;
	};
	// Each line is echoed, then printed back by cat before the next is typed.
	assert!(shown.contains("one\r\none\r\ntwo\r\ntwo\r\n"), "{shown:?}");
}

fn interrupting_ends_a_program_and_the_rest_of_its_line() {
	// What is typed right after ^C reaches the shell, not the ending program.
	let keys: &[&[u8]] = &[b"yes; echo never\r", b"\x03echo $?\r", b"exit\r"];
	let Some(shown) = device("interrupt", keys) else {
		return;
	};
	assert!(shown.contains("y\r\ny\r\n"), "yes printed as it ran");
	assert!(
		shown.contains("^C\r\n"),
		"{:?}",
		&shown[shown.len().saturating_sub(200)..]
	);
	let tail = shown.rfind("y\r\n").map_or("", |at| &shown[at..]);
	assert!(
		!shown.contains("\r\nnever\r\n"),
		"the rest of the line was stopped: {tail:?}"
	);
	assert!(
		shown.contains("echo $?\r\n130\r\n"),
		"{:?}",
		&shown[shown.len().saturating_sub(200)..]
	);
}

fn the_keyboard_goes_back_to_the_shell_after_a_program() {
	// ed reads lines; once it quits, the shell's echo and \r\n are back.
	let keys: &[&[u8]] = &[
		b"ed\r",
		b"a\r",
		b"text\r",
		b".\r",
		b",p\r",
		b"Q\r",
		b"echo done\r",
		b"exit\r",
	];
	let Some(shown) = device("ed", keys) else {
		return;
	};
	assert!(
		shown.contains(",p\r\ntext\r\nQ\r\n"),
		"ed printed what it was given: {shown:?}"
	);
	assert!(shown.contains("echo done\r\ndone\r\n"), "{shown:?}");
}

fn real_shell(name: &str) -> Option<(Shell, PathBuf)> {
	let path = commands()?;
	let home = scratch(name);
	let mut shell = Shell::new(home.clone());
	shell.env.insert("PATH".into(), path);
	Some((shell, home))
}

fn ran(shell: &mut Shell, line: &str) -> String {
	String::from_utf8(shell.run(line).output).unwrap()
}

/// What the system's own copy of a command prints.
fn system(program: &str, args: &[&str]) -> String {
	let out = std::process::Command::new(program)
		.args(args)
		.output()
		.unwrap();
	String::from_utf8(out.stdout).unwrap()
}

fn uname_is_the_real_one() {
	let Some((mut sh, _)) = real_shell("uname") else {
		return;
	};
	for flags in ["", "-a", "-m", "-sr", "-n"] {
		let args: Vec<&str> = flags.split_whitespace().collect();
		assert_eq!(
			ran(&mut sh, &format!("uname {flags}")),
			system("/usr/bin/uname", &args),
			"{flags}"
		);
	}
	let bad = ran(&mut sh, "uname -x");
	assert!(
		bad.contains("illegal option -- x") && bad.contains("usage: uname"),
		"{bad}"
	);
	assert_eq!(sh.status, 1);
}

fn files_are_made_listed_read_and_removed_by_the_real_tools() {
	let Some((mut sh, home)) = real_shell("files") else {
		return;
	};
	assert_eq!(
		ran(&mut sh, "mkdir -p a/b && touch a/f && echo hi > a/b/g"),
		""
	);
	assert!(home.join("a/b/g").is_file());
	assert_eq!(ran(&mut sh, "ls -1 a"), "b\nf\n");
	assert_eq!(ran(&mut sh, "cat a/b/g ~/a/b/g"), "hi\nhi\n");
	assert_eq!(
		ran(&mut sh, "cd a && cp f g && mv g b/h && ls -1 b"),
		"g\nh\n"
	);
	assert_eq!(ran(&mut sh, "rm -r b; ls -1"), "f\n");
	let missing = ran(&mut sh, "cat missing");
	assert_eq!(missing, "cat: missing: No such file or directory\n");
	assert_eq!(sh.status, 1);
}

fn each_run_starts_with_fresh_globals() {
	let Some((mut sh, home)) = real_shell("globals") else {
		return;
	};
	fs::write(home.join("file"), "1234").unwrap();
	let long = ran(&mut sh, "ls -l file");
	assert!(long.starts_with("-rw") && long.contains(" 4 "), "{long}");
	assert_eq!(ran(&mut sh, "ls file"), "file\n", "-l did not stick");
}

fn ls_lays_out_columns_for_the_terminal_but_one_per_line_into_a_file() {
	let Some((mut sh, home)) = real_shell("columns") else {
		return;
	};
	for name in ["alpha", "bravo", "charlie", "delta"] {
		fs::write(home.join(name), "").unwrap();
	}
	sh.size = (80, 24);
	assert_eq!(ran(&mut sh, "ls"), "alpha\tbravo\tcharlie\tdelta\n");
	sh.size = (10, 24);
	assert_eq!(ran(&mut sh, "ls"), "alpha\nbravo\ncharlie\ndelta\n");
	ran(&mut sh, "ls > list");
	assert_eq!(
		fs::read_to_string(home.join("list")).unwrap(),
		"alpha\nbravo\ncharlie\ndelta\nlist\n"
	);
}

fn errors_still_reach_the_screen_when_output_goes_to_a_file() {
	let Some((mut sh, home)) = real_shell("stderr") else {
		return;
	};
	fs::write(home.join("here"), "text\n").unwrap();
	let shown = ran(&mut sh, "cat here missing > out");
	assert_eq!(shown, "cat: missing: No such file or directory\n");
	assert_eq!(fs::read_to_string(home.join("out")).unwrap(), "text\n");
}

fn programs_see_the_shells_environment_and_directory() {
	let Some((mut sh, home)) = real_shell("env") else {
		return;
	};
	let env = ran(&mut sh, "export GREETING='hello there'; env");
	assert!(env.contains("GREETING=hello there\n"), "{env}");
	assert!(env.contains(&format!("PWD={}\n", home.display())), "{env}");
	assert_eq!(ran(&mut sh, "printenv GREETING"), "hello there\n");
	fs::create_dir(home.join("sub")).unwrap();
	assert_eq!(ran(&mut sh, "cd sub; ls -1 .."), "sub\n");
	assert_ne!(
		std::env::current_dir().unwrap(),
		home.join("sub"),
		"the app's directory is put back"
	);
}

fn text_tools_work_on_files() {
	let Some((mut sh, home)) = real_shell("text") else {
		return;
	};
	fs::write(home.join("words"), "pear\napple\npear\nfig\n").unwrap();
	assert_eq!(ran(&mut sh, "sort -u words"), "apple\nfig\npear\n");
	assert_eq!(ran(&mut sh, "wc -l words"), "       4 words\n");
	assert_eq!(ran(&mut sh, "head -n 2 words"), "pear\napple\n");
	assert_eq!(ran(&mut sh, "seq 3"), "1\n2\n3\n");
	assert_eq!(ran(&mut sh, "basename /a/b/c.txt .txt"), "c\n");
}

fn pipes_feed_each_stage_what_the_one_before_printed() {
	let Some((mut sh, home)) = real_shell("pipes") else {
		return;
	};
	fs::write(home.join("words"), "pear\napple\npear\nfig\n").unwrap();
	assert_eq!(
		ran(&mut sh, "cat words | sort | uniq -c | sort -rn | head -n 1"),
		"   2 pear\n"
	);
	assert_eq!(
		ran(&mut sh, "echo b a | tr ' ' '\\n' | sort"),
		"a\nb\n",
		"built-ins pipe too"
	);
	assert_eq!(ran(&mut sh, "sort < words | head -n 2"), "apple\nfig\n");
	assert_eq!(
		ran(&mut sh, "seq 100000 | head -n 2"),
		"1\n2\n",
		"a reader that stops early"
	);
	assert_eq!(
		ran(&mut sh, "cat missing | wc -l"),
		"cat: missing: No such file or directory\n       0\n"
	);
	assert_eq!(
		ran(&mut sh, "sort < nope"),
		"poopsh: nope: No such file or directory (os error 2)\n"
	);
	assert_eq!(sh.status, 1);
}

/// Apple's `rev`, published in a `file://` repository as the package
/// `backwards`, installed with `pkg` and run by its new name from `$PATH`.
fn a_command_pkg_installed_runs_from_the_path() {
	let Some((mut sh, home)) = real_shell("pkg") else {
		return;
	};
	let repo = home.join("repo");
	fs::create_dir_all(repo.join("tree/bin")).unwrap();
	fs::copy(
		PathBuf::from(COMMANDS).join("rev"),
		repo.join("tree/bin/backwards"),
	)
	.unwrap();
	let archive = std::process::Command::new("tar")
		.env("COPYFILE_DISABLE", "1")
		.args(["-czf", "-", "-C"])
		.arg(repo.join("tree"))
		.arg("bin")
		.output()
		.unwrap()
		.stdout;
	fs::write(repo.join("backwards-1.tar.gz"), &archive).unwrap();
	let hash = system(
		"shasum",
		&[
			"-a",
			"256",
			repo.join("backwards-1.tar.gz").to_str().unwrap(),
		],
	);
	let index = format!(
		"backwards 1 {} {} backwards-1.tar.gz Reverse lines\n",
		&hash[..64],
		archive.len()
	);
	fs::write(repo.join("index"), index).unwrap();
	let packages = home.join("Library/poopterm/packages/bin");
	let path = format!("{}:{}", packages.display(), COMMANDS);
	sh.env.insert("PATH".into(), path);
	assert_eq!(
		ran(&mut sh, "backwards"),
		"poopsh: backwards: command not found\n"
	);
	ran(
		&mut sh,
		&format!("pkg source file://{}/index", repo.display()),
	);
	assert_eq!(
		ran(&mut sh, "pkg install backwards"),
		"installed backwards 1: backwards\n"
	);
	assert_eq!(ran(&mut sh, "echo stressed | backwards"), "desserts\n");
	assert_eq!(ran(&mut sh, "pkg remove backwards"), "removed backwards\n");
	assert!(ran(&mut sh, "backwards").contains("not found"));
}

/// grep, sed and the other text tools against the system's own copies.
fn the_new_text_tools_match_the_systems() {
	let Some((mut sh, home)) = real_shell("text-tools") else {
		return;
	};
	let text = "pear\napple pie\nfig\n\tgrape\n";
	fs::write(home.join("words"), text).unwrap();
	let file = home.join("words").display().to_string();
	for (line, program, args) in [
		("grep -n p words", "/usr/bin/grep", vec!["-n", "p", &file]),
		("grep -vc a words", "/usr/bin/grep", vec!["-vc", "a", &file]),
		("sed -n 2p words", "/usr/bin/sed", vec!["-n", "2p", &file]),
		("sed s/p/P/g words", "/usr/bin/sed", vec!["s/p/P/g", &file]),
		("expand words", "/usr/bin/expand", vec![&file]),
		("fmt -w 10 words", "/usr/bin/fmt", vec!["-w", "10", &file]),
		("column -t words", "/usr/bin/column", vec!["-t", &file]),
		("hexdump -C words", "/usr/bin/hexdump", vec!["-C", &file]),
		("cksum words", "/usr/bin/cksum", vec![&file]),
		("pr -2 -t words", "/usr/bin/pr", vec!["-2", "-t", &file]),
		("pr -l 12 words", "/usr/bin/pr", vec!["-l", "12", &file]),
	] {
		let expected = system(program, &args).replace(&file, "words");
		assert_eq!(ran(&mut sh, line), expected, "{line}");
	}
	assert_eq!(ran(&mut sh, "test -f words && echo file"), "file\n");
	assert_eq!(ran(&mut sh, "test -d words || echo no"), "no\n");
	assert_eq!(ran(&mut sh, "jot 3 5"), "5\n6\n7\n");
	assert_eq!(
		ran(&mut sh, "gzip -k words && gzip -dc words.gz"),
		text,
		"through a compressed copy and back"
	);
}

/// md5 and the sha family, base64, tar and compress, as the system's.
fn digests_archives_and_encodings_match_the_systems() {
	let Some((mut sh, home)) = real_shell("digests") else {
		return;
	};
	fs::write(home.join("data"), "hello digests\n".repeat(500)).unwrap();
	let file = home.join("data").display().to_string();
	for (line, program, args) in [
		("md5 data", "/sbin/md5", vec![file.as_str()]),
		("md5 -q data", "/sbin/md5", vec!["-q", &file]),
		("sha1 -q data", "/sbin/sha1", vec!["-q", &file]),
		("sha256 -r data", "/sbin/sha256", vec!["-r", &file]),
		("sha512 -q data", "/sbin/sha512", vec!["-q", &file]),
		("base64 -i data", "/usr/bin/base64", vec!["-i", &file]),
	] {
		let expected = system(program, &args).replace(&file, "data");
		assert_eq!(ran(&mut sh, line), expected, "{line}");
	}
	assert_eq!(
		ran(&mut sh, "md5 -s abc"),
		"900150983cd24fb0d6963f7d28e17f72\n",
		"RFC 1321's example"
	);
	assert_eq!(
		ran(&mut sh, "sha224 -q -s abc"),
		"23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n",
		"FIPS 180-2's example"
	);
	assert_eq!(
		ran(
			&mut sh,
			"base64 -i data -o enc && base64 -d -i enc | md5 -q"
		),
		ran(&mut sh, "md5 -q data"),
		"through base64 and back"
	);
	ran(
		&mut sh,
		"mkdir box && cp data box/a && tar -cf box.tar box && rm -r box",
	);
	let listed = system(
		"/usr/bin/tar",
		&["-tf", &home.join("box.tar").display().to_string()],
	);
	assert_eq!(listed, "box\nbox/a\n", "the system's tar reads ours");
	assert_eq!(
		ran(&mut sh, "tar -xf box.tar && cmp box/a data && echo same"),
		"same\n"
	);
	assert_eq!(
		ran(&mut sh, "compress -c data | uncompress -c | md5 -q"),
		ran(&mut sh, "md5 -q data")
	);
}

/// Commands that start others run them inside the shell too.
fn xargs_and_find_start_other_commands() {
	let Some((mut sh, home)) = real_shell("starting") else {
		return;
	};
	assert_eq!(ran(&mut sh, "seq 3 | xargs echo"), "1 2 3\n");
	assert_eq!(ran(&mut sh, "seq 4 | xargs -n 2 echo +"), "+ 1 2\n+ 3 4\n");
	assert_eq!(
		ran(&mut sh, "echo a | xargs -I % cat"),
		"",
		"the child's input is /dev/null, not the rest of xargs's"
	);
	fs::create_dir(home.join("d")).unwrap();
	fs::write(home.join("d/one"), "1\n").unwrap();
	fs::write(home.join("d/two"), "2\n").unwrap();
	assert_eq!(
		ran(&mut sh, "find d -type f -exec cat {} \\; | sort"),
		"1\n2\n"
	);
	assert_eq!(ran(&mut sh, "find d -name one -exec ls {} +"), "d/one\n");
	assert_eq!(
		ran(&mut sh, "find d -name one -exec test -f {} \\; -print"),
		"d/one\n",
		"a child's status decides find's test"
	);
	let missing = ran(&mut sh, "echo x | xargs no-such-tool");
	assert!(missing.contains("no-such-tool"), "{missing}");
	assert_eq!(sh.status, 127);
	assert_eq!(
		ran(&mut sh, "seq 2 | xargs xargs"),
		"xargs: cannot run inside itself\n",
		"refused, with no harm done"
	);
	assert_eq!(ran(&mut sh, "echo still here"), "still here\n");
}
