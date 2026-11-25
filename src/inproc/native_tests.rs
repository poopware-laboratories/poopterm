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

#[path = "native_toys_tests.rs"]
mod toys;

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
	nano_edits_and_saves_a_file_on_the_device();
	ssh_runs_a_command_and_a_session_on_a_server();
	the_ssh_server_serves_commands_and_sessions();
	patch_diffstat_and_bzip2_agree_with_the_systems();
	zip_and_unzip_agree_with_the_systems();
	tcc_compiles_and_runs_c_in_memory();
	make_runs_recipes_through_the_shell();
	toys::figlet_draws_with_the_fonts_beside_it();
	toys::lolcat_colours_only_when_asked_or_on_a_terminal();
	toys::cpufetch_names_a_chip_its_tables_do_not_know();
	toys::nyancat_draws_the_frames_it_is_asked_for();
	toys::ncdu_exports_the_sizes_of_a_folder();
	toys::sl_drives_across_and_gives_the_prompt_back();
	toys::cmatrix_rains_until_q();
	toys::ncdu_browses_and_quits();
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

/// Where `ios/packages/build.sh macosx nano` puts the package.
const NANO_DIR: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/target/ios-packages/work-macosx/nano/bin"
);

/// nano is full-screen: it needs raw mode, the live window size and keys
/// as they come, all from the device, through our own ncurses.
fn nano_edits_and_saves_a_file_on_the_device() {
	// The descriptions the app carries (ios/terminfo.sh), as it points
	// TERMINFO at them: the host's are laid out the way Apple's ncurses
	// reads them, not ours.
	let terminfo = scratch("terminfo").join("db");
	let made = std::process::Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ios/terminfo.sh"))
		.arg(&terminfo)
		.output()
		.unwrap();
	assert!(
		made.status.success(),
		"{}",
		String::from_utf8_lossy(&made.stderr)
	);
	if !PathBuf::from(NANO_DIR).join("nano").is_file() {
		eprintln!("skipped: run ios/packages/build.sh macosx nano");
		return;
	}
	let keys: &[&[u8]] = &[
		b"nano note.txt\r",
		b"hello from nano",
		b"\x0f", // ^O: write out
		b"\r",   // keep the name
		b"\x18", // ^X: exit
		b"cat note.txt\r",
		b"exit\r",
	];
	let Some(shown) = device_with("nano", Some(NANO_DIR), Some(&terminfo), keys) else {
		return;
	};
	assert!(
		shown.contains("GNU nano"),
		"nano drew its title bar: {shown:?}"
	);
	assert!(
		shown.contains("\x1b[?1049h") || shown.contains("\x1b["),
		"it drew with escapes"
	);
	assert!(
		shown.contains("cat note.txt\r\nhello from nano\r\n"),
		"the file holds what was typed: {:?}",
		&shown[shown.len().saturating_sub(300)..]
	);
}

/// Where `ios/packages/build.sh macosx make` puts make.
const MAKE_DIR: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/target/ios-packages/work-macosx/make/bin"
);

const MAKEFILE: &str = "NAME = world\n\
all: greet build.txt run\n\
greet:\n\
\t@echo hello $(NAME)\n\
build.txt: a.txt b.txt\n\
\tcat a.txt b.txt > build.txt\n\
\t@echo built\n\
run:\n\
\ttcc -run prog.c\n\
clean:\n\
\trm -f build.txt\n\
objs:\n\
\ttouch x1.o x2.o\n\
\tLEFT=over rm -f *.o\n\
.PHONY: all greet run clean objs\n";

/// make reads a Makefile, runs recipes as programs and as `sh -c` lines
/// (redirections included) in the app, and knows what is up to date.
fn make_runs_recipes_through_the_shell() {
	let tcc = PathBuf::from(TCC_DIR).join("bin/tcc").is_file();
	if !PathBuf::from(MAKE_DIR).join("make").is_file() || !tcc {
		eprintln!("skipped: run ios/packages/build.sh macosx make tcc");
		return;
	}
	let Some((mut sh, home)) = real_shell("make") else {
		return;
	};
	let path = format!("{MAKE_DIR}:{TCC_DIR}/bin:{}", sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	sh.env
		.insert("TCCDIR".into(), format!("{TCC_DIR}/share/tcc"));
	fs::write(home.join("Makefile"), MAKEFILE).unwrap();
	fs::write(home.join("a.txt"), "first\n").unwrap();
	fs::write(home.join("b.txt"), "second\n").unwrap();
	fs::write(
		home.join("prog.c"),
		"#include <stdio.h>\nint main(void) { puts(\"from C\"); return 0; }\n",
	)
	.unwrap();

	let shown = ran(&mut sh, "make; echo status $?");
	assert!(
		shown.contains("hello world\ncat a.txt b.txt > build.txt\nbuilt\ntcc -run prog.c\nfrom C\n"),
		"recipes ran in order, commands echoed: {shown}"
	);
	assert!(shown.ends_with("status 0\n"), "{shown}");
	assert_eq!(
		fs::read_to_string(home.join("build.txt")).unwrap(),
		"first\nsecond\n",
		"a redirection through sh -c"
	);
	let again = ran(&mut sh, "make build.txt");
	assert!(
		again.contains("is up to date"),
		"nothing to rebuild: {again}"
	);
	ran(&mut sh, "make clean");
	assert!(!home.join("build.txt").exists(), "clean removed it");
	let dry = ran(&mut sh, "make -n build.txt");
	assert!(
		dry.contains("cat a.txt b.txt > build.txt") && !home.join("build.txt").exists(),
		"-n only prints: {dry}"
	);
	let named = ran(&mut sh, "make NAME=you greet");
	assert_eq!(named, "hello you\n", "a variable given on the command line");
	let cleaned = ran(&mut sh, "make objs; echo status $?");
	assert!(cleaned.ends_with("status 0\n"), "{cleaned}");
	assert!(
		!home.join("x1.o").exists() && !home.join("x2.o").exists(),
		"the pattern removed both"
	);
	let fail = ran(&mut sh, "make nosuchtarget; echo status $?");
	assert!(
		fail.contains("No rule to make target 'nosuchtarget'") && fail.ends_with("status 2\n"),
		"{fail}"
	);
}

/// Where `ios/packages/build.sh macosx tcc` puts tcc and its folder.
const TCC_DIR: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/target/ios-packages/work-macosx/tcc"
);

const HELLO_C: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
static int counter = 40;
int main(int argc, char **argv) {
	char *text = strdup("poopterm");
	counter += 2;
	printf("hello from tcc: %s, fib(20)=%d, argc=%d, pi=%.3f, big=%ld, counter=%d\n",
		text, fib(20), argc, 3.14159, 1L << 40, counter);
	printf("main at %p, counter at %p\n", (void *)main, (void *)&counter);
	free(text);
	return 7;
}
"#;

const TOOLS_C: &str = r#"
#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
static int by_value(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }
struct point { int x, y; double scale; };
int main(void) {
	int v[] = {5, 3, 9, 1};
	qsort(v, 4, sizeof v[0], by_value);
	printf("sorted %d %d %d %d\n", v[0], v[1], v[2], v[3]);
	printf("math %.4f %.4f %.1f\n", sqrt(2.0), sin(1.0), pow(2, 10));
	printf("ctype %d %c\n", isdigit('7') != 0, toupper('q'));
	errno = 0;
	FILE *missing = fopen("/no/such/file", "r");
	printf("errno %d %s\n", missing == NULL && errno == ENOENT, strerror(errno));
	struct stat st;
	printf("stat %d\n", stat(".", &st) == 0 && S_ISDIR(st.st_mode));
	DIR *dir = opendir(".");
	int visible = 0;
	struct dirent *entry;
	while ((entry = readdir(dir)))
		if (entry->d_name[0] != '.') visible++;
	closedir(dir);
	printf("entries %d\n", visible > 0);
	time_t when = 86400 * 365;
	printf("year %d\n", gmtime(&when)->tm_year + 1900);
	uint64_t big = UINT64_C(1) << 40;
	printf("big %" PRIu64 "\n", big);
	struct point p = {3, 4, 0.5};
	printf("struct %d %.1f\n", p.x * p.x + p.y * p.y, p.scale * 2);
	int fd = open("written.txt", O_WRONLY | O_CREAT | O_TRUNC, 0644);
	write(fd, "hi\n", 3);
	close(fd);
	return 0;
}
"#;

const EXIT_C: &str = r#"
#include <stdio.h>
#include <stdlib.h>
int main(void) {
	printf("about to exit\n");
	exit(3);
	printf("not reached\n");
	return 0;
}
"#;

/// tcc compiles C from files and runs it in this process: arguments,
/// statics, doubles, a status back, and `exit` that ends the program and
/// not the app. With a region standing in for the one StikDebug prepares,
/// the code runs there and the statics live apart in the writable view.
fn tcc_compiles_and_runs_c_in_memory() {
	if !PathBuf::from(TCC_DIR).join("bin/tcc").is_file() {
		eprintln!("skipped: run ios/packages/build.sh macosx tcc");
		return;
	}
	let Some((mut sh, home)) = real_shell("tcc") else {
		return;
	};
	let path = format!("{TCC_DIR}/bin:{}", sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	sh.env
		.insert("TCCDIR".into(), format!("{TCC_DIR}/share/tcc"));
	fs::write(home.join("hello.c"), HELLO_C).unwrap();
	fs::write(home.join("quit.c"), EXIT_C).unwrap();

	let shown = ran(&mut sh, "tcc -run hello.c a b; echo status $?");
	assert!(
		shown.contains(
			"hello from tcc: poopterm, fib(20)=6765, argc=3, pi=3.142, big=1099511627776, counter=42\n"
		),
		"{shown}"
	);
	assert!(
		shown.ends_with("status 7\n"),
		"the program's status: {shown}"
	);
	let quit = ran(&mut sh, "tcc -run quit.c; echo status $?");
	assert_eq!(
		quit, "about to exit\nstatus 3\n",
		"exit ended the program, not the app"
	);
	fs::write(home.join("tools.c"), TOOLS_C).unwrap();
	let tools = ran(&mut sh, "tcc -run tools.c; echo status $?");
	assert_eq!(
		tools,
		"sorted 1 3 5 9\nmath 1.4142 0.8415 1024.0\nctype 1 Q\nerrno 1 No such file or directory\n\
		 stat 1\nentries 1\nyear 1971\nbig 1099511627776\nstruct 25 1.0\nstatus 0\n"
	);
	assert_eq!(
		fs::read_to_string(home.join("written.txt")).unwrap(),
		"hi\n"
	);
	let missing = ran(&mut sh, "tcc -run nothere.c; echo status $?");
	assert!(
		missing.contains("nothere.c") && missing.ends_with("status 1\n"),
		"{missing}"
	);

	// The prepared-region way: code in the region, data in its view.
	let (start, len) = crate::jit::tests::install_test_region(1 << 20);
	let shown = ran(&mut sh, "tcc -run hello.c; echo status $?");
	crate::jit::tests::remove_test_region();
	assert!(shown.contains("counter=42"), "{shown}");
	let address = |label: &str| {
		let after = shown
			.split(label)
			.nth(1)
			.unwrap_or_else(|| panic!("{label}: {shown}"));
		let hex: String = after
			.trim_start_matches("0x")
			.chars()
			.take_while(char::is_ascii_hexdigit)
			.collect();
		usize::from_str_radix(&hex, 16).unwrap()
	};
	let main = address("main at 0x");
	let counter = address("counter at 0x");
	assert!(
		(start..start + len).contains(&main),
		"main runs in the region: {main:#x} not in {start:#x}+{len:#x}"
	);
	assert!(
		!(start..start + len).contains(&counter),
		"its data is in the writable view"
	);
	assert!(shown.ends_with("status 7\n"), "{shown}");
}

/// zip makes archives the system's unzip reads, and unzip reads (and
/// lists, as zipinfo) the ones the system's zip made.
fn zip_and_unzip_agree_with_the_systems() {
	let dirs = ["zip", "unzip"].map(|name| {
		format!(
			"{}/target/ios-packages/work-macosx/{name}/bin",
			env!("CARGO_MANIFEST_DIR")
		)
	});
	if dirs.iter().any(|dir| {
		!PathBuf::from(dir)
			.join(dir.rsplit('/').nth(1).unwrap())
			.is_file()
	}) {
		eprintln!("skipped: run ios/packages/build.sh macosx zip unzip");
		return;
	}
	let Some((mut sh, home)) = real_shell("zip-unzip") else {
		return;
	};
	let path = format!("{}:{}:{}", dirs[0], dirs[1], sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	fs::create_dir_all(home.join("tree/sub")).unwrap();
	fs::write(home.join("tree/a.txt"), "alpha\n".repeat(100)).unwrap();
	fs::write(home.join("tree/sub/b.txt"), "beta\n").unwrap();
	let made = ran(&mut sh, "zip -r ours.zip tree");
	assert!(
		made.contains("adding: tree/a.txt") && made.contains("adding: tree/sub/b.txt"),
		"{made}"
	);
	let listed = system(
		"/usr/bin/unzip",
		&["-Z1", &home.join("ours.zip").display().to_string()],
	);
	assert!(
		listed.lines().any(|name| name == "tree/sub/b.txt"),
		"the system lists ours: {listed}"
	);
	let system_made = std::process::Command::new("/usr/bin/zip")
		.current_dir(&home)
		.args(["-qr", "theirs.zip", "tree"])
		.status()
		.unwrap();
	assert!(system_made.success());
	fs::remove_dir_all(home.join("tree")).unwrap();
	let extracted = ran(&mut sh, "unzip -o theirs.zip");
	assert!(extracted.contains("inflating: tree/a.txt"), "{extracted}");
	assert_eq!(
		fs::read_to_string(home.join("tree/sub/b.txt")).unwrap(),
		"beta\n"
	);
	assert_eq!(
		fs::read_to_string(home.join("tree/a.txt")).unwrap(),
		"alpha\n".repeat(100)
	);
	let listing = ran(&mut sh, "unzip -l ours.zip");
	assert!(
		listing.contains("tree/sub/b.txt") && listing.contains("4 files"),
		"lists its own archive, directories included: {listing}"
	);
	assert!(
		ran(&mut sh, "zipinfo -1 ours.zip").contains("tree/a.txt"),
		"zipinfo is the same program by another name"
	);
}

/// Where `ios/packages/build.sh macosx bzip2` puts bzip2.
const BZIP2_DIR: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/target/ios-packages/work-macosx/bzip2/bin"
);

/// patch applies what diff made, diffstat counts it, and bzip2 compresses
/// into what the system's bzip2 reads and reads what it made.
fn patch_diffstat_and_bzip2_agree_with_the_systems() {
	let Some((mut sh, home)) = real_shell("patch-bz") else {
		return;
	};
	let bzip2 = PathBuf::from(BZIP2_DIR).join("bzip2").is_file();
	if bzip2 {
		let path = format!("{BZIP2_DIR}:{}", sh.env["PATH"]);
		sh.env.insert("PATH".into(), path);
	}
	fs::write(home.join("old"), "one\ntwo\nthree\nfour\n").unwrap();
	fs::write(home.join("new"), "one\n2\nthree\nfour\nfive\n").unwrap();
	ran(&mut sh, "diff -u old new > change.diff");
	assert_eq!(
		ran(&mut sh, "diffstat -f 0 change.diff | tail -1"),
		" 1 file changed, 2 insertions(+), 1 deletion(-)\n"
	);
	assert_eq!(
		ran(&mut sh, "patch old < change.diff"),
		"patching file old\n"
	);
	assert_eq!(
		ran(&mut sh, "cmp old new && echo same"),
		"same\n",
		"the patch made old into new"
	);
	assert_eq!(
		ran(&mut sh, "patch -R old < change.diff"),
		"patching file old\n"
	);
	assert_eq!(
		fs::read_to_string(home.join("old")).unwrap(),
		"one\ntwo\nthree\nfour\n",
		"and back"
	);
	if !bzip2 {
		eprintln!("skipped bzip2: run ios/packages/build.sh macosx bzip2");
		return;
	}
	let text = "compress me, compress me, compress me\n".repeat(200);
	fs::write(home.join("big"), &text).unwrap();
	ran(&mut sh, "bzip2 -k big");
	assert!(home.join("big.bz2").is_file());
	let theirs = system(
		"/usr/bin/bzip2",
		&["-dc", &home.join("big.bz2").display().to_string()],
	);
	assert_eq!(theirs, text, "the system reads ours");
	assert_eq!(
		ran(&mut sh, "bzcat big.bz2 | md5 -q"),
		ran(&mut sh, "md5 -q big")
	);
	ran(&mut sh, "rm big && bunzip2 big.bz2");
	assert_eq!(
		fs::read_to_string(home.join("big")).unwrap(),
		text,
		"bunzip2 restores it"
	);
}

/// Where `ios/packages/build.sh macosx dropbear` puts ssh and dropbearkey.
const DROPBEAR_DIR: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/target/ios-ports/dropbear-out-macosx/bin"
);

/// The system's own sshd, started for one user on a spare port: what our
/// ssh talks to. It ends with the test.
struct Server {
	child: std::process::Child,
	port: u16,
}

impl Drop for Server {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

fn start_sshd(dir: &std::path::Path, authorized: &std::path::Path) -> Option<Server> {
	if !PathBuf::from("/usr/sbin/sshd").is_file() {
		return None;
	}
	let hostkey = dir.join("hostkey");
	let made = std::process::Command::new("ssh-keygen")
		.args(["-q", "-t", "ed25519", "-N", "", "-f"])
		.arg(&hostkey)
		.status()
		.ok()?;
	if !made.success() {
		return None;
	}
	let port = std::net::TcpListener::bind("127.0.0.1:0")
		.ok()?
		.local_addr()
		.ok()?
		.port();
	let config = dir.join("sshd_config");
	fs::write(
		&config,
		format!(
			"Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\n\
			 PubkeyAuthentication yes\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n\
			 UsePAM no\nStrictModes no\nPidFile {}\n",
			hostkey.display(),
			authorized.display(),
			dir.join("sshd.pid").display()
		),
	)
	.ok()?;
	let child = std::process::Command::new("/usr/sbin/sshd")
		.args(["-D", "-f"])
		.arg(&config)
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.spawn()
		.ok()?;
	let server = Server { child, port };
	for _ in 0..50 {
		if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
			return Some(server);
		}
		thread::sleep(std::time::Duration::from_millis(100));
	}
	None
}

/// ssh, from Dropbear: a key made by our dropbearkey logs in to a real
/// server, runs a command, and carries an interactive session, where the
/// device's raw mode and the keys reach the far shell.
fn ssh_runs_a_command_and_a_session_on_a_server() {
	if !PathBuf::from(DROPBEAR_DIR).join("ssh").is_file() {
		eprintln!("skipped: run ios/packages/build.sh macosx dropbear");
		return;
	}
	let Some((mut sh, home)) = real_shell("ssh") else {
		return;
	};
	let path = format!("{DROPBEAR_DIR}:{}", sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	fs::create_dir_all(home.join(".ssh")).unwrap();
	let made = ran(&mut sh, "dropbearkey -t ed25519 -f id");
	assert!(made.contains("Public key portion"), "{made}");
	let public = ran(&mut sh, "dropbearkey -y -f id | grep ssh-ed25519");
	assert!(public.starts_with("ssh-ed25519 "), "{public:?}");
	let authorized = home.join("authorized_keys");
	fs::write(&authorized, public).unwrap();
	let Some(server) = start_sshd(&home, &authorized) else {
		eprintln!("skipped: no sshd to talk to");
		return;
	};
	let user = sh.env.get("USER").cloned().unwrap_or_else(|| "root".into());
	let port = server.port;
	let target = format!("-y -i id -p {port} {user}@127.0.0.1");
	let once: &'static [u8] = Box::leak(
		format!("ssh {target} echo hello-from-ssh\r")
			.into_bytes()
			.into_boxed_slice(),
	);
	let keys: &'static [&'static [u8]] =
		Box::leak(vec![once, b"exit\r" as &'static [u8]].into_boxed_slice());
	let output = device_in(sh.clone(), Some(DROPBEAR_DIR), None, keys);
	assert!(output.contains("hello-from-ssh\r\n"), "{output:?}");
	// An interactive session on a terminal there (-t): what is typed goes
	// through the device's raw mode, the network and the far terminal's
	// echo, and cat sends it back; ^D ends it and the local prompt returns.
	let login: &'static [u8] = Box::leak(
		format!("ssh -t {target} cat\r")
			.into_bytes()
			.into_boxed_slice(),
	);
	let session: &'static [&'static [u8]] = Box::leak(
		vec![
			login,
			b"typed-over-ssh\r" as &'static [u8],
			b"\x04",
			b"echo back-home\r",
			b"exit\r",
		]
		.into_boxed_slice(),
	);
	let shown = device_in(sh, Some(DROPBEAR_DIR), None, session);
	drop(server);
	assert!(
		shown.matches("typed-over-ssh").count() >= 2,
		"echoed there, then sent back by cat: {shown:?}"
	);
	assert!(
		shown.contains("echo back-home\r\nback-home\r\n"),
		"the local shell has the keys again: {shown:?}"
	);
}

/// The `sshd` built-in: the system's own `ssh` logs in with a key and a
/// password, runs a command (with its status) and a session on a terminal.
fn the_ssh_server_serves_commands_and_sessions() {
	use std::process::{Command, Stdio};
	let server = PathBuf::from(DROPBEAR_DIR);
	let client = PathBuf::from("/usr/bin/ssh");
	if !server.join("sshd").is_file() || !client.is_file() {
		eprintln!("skipped: run ios/packages/build.sh macosx dropbear");
		return;
	}
	let Some((mut sh, home)) = real_shell("sshd") else {
		return;
	};
	sh.support = home.clone();
	let path = format!("{DROPBEAR_DIR}:{}", sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
	sh.env.insert("USER".into(), user.clone());
	let key = home.join("client_key");
	let made = Command::new("ssh-keygen")
		.args(["-q", "-t", "ed25519", "-N", "", "-f"])
		.arg(&key)
		.status()
		.unwrap();
	assert!(made.success());
	fs::create_dir_all(home.join(".ssh")).unwrap();
	fs::copy(
		home.join("client_key.pub"),
		home.join(".ssh/authorized_keys"),
	)
	.unwrap();
	let port = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap()
		.local_addr()
		.unwrap()
		.port();
	let started = ran(&mut sh, &format!("sshd start -p {port} -P open-sesame"));
	assert!(started.starts_with("sshd: started\n"), "{started}");
	let ssh = |extra: &[&str], word: &str| {
		let mut command = Command::new(&client);
		command
			.args([
				"-o",
				"StrictHostKeyChecking=no",
				"-o",
				"UserKnownHostsFile=/dev/null",
			])
			.args([
				"-o",
				"LogLevel=ERROR",
				"-o",
				"ConnectTimeout=20",
				"-p",
				&port.to_string(),
			])
			.args(extra)
			.arg(format!("{user}@127.0.0.1"))
			.arg(word)
			.stdin(Stdio::null());
		command
	};
	let by_key = ["-o", "BatchMode=yes", "-i", key.to_str().unwrap()];
	let output = ssh(&by_key, "echo hi-from-device; pwd").output().unwrap();
	let text = String::from_utf8_lossy(&output.stdout).into_owned();
	let early = ran(&mut sh, "sshd log");
	assert!(
		text.starts_with("hi-from-device\n") && text.contains(home.to_str().unwrap()),
		"{text:?} {} log: {early}",
		String::from_utf8_lossy(&output.stderr)
	);
	let status = ssh(&by_key, "false").status().unwrap();
	let log_now = ran(&mut sh, "sshd log");
	assert_eq!(
		status.code(),
		Some(1),
		"the command's status comes back; log: {log_now}"
	);
	// A terminal session: typed lines run in the app's shell.
	let mut session = ssh(
		&["-tt", "-o", "BatchMode=yes", "-i", key.to_str().unwrap()],
		"",
	);
	session.stdin(Stdio::piped()).stdout(Stdio::piped());
	let mut child = session.spawn().unwrap();
	// A session that never ends is cut off, so the test fails instead of hanging.
	let pid = child.id();
	thread::spawn(move || {
		thread::sleep(std::time::Duration::from_secs(25));
		let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
	});
	let mut stdin = child.stdin.take().unwrap();
	std::io::Write::write_all(&mut stdin, b"echo interactive-ok\rexit\r").unwrap();
	let shown = child.wait_with_output().unwrap();
	drop(stdin);
	let shown = String::from_utf8_lossy(&shown.stdout).into_owned();
	assert!(
		shown.contains("interactive-ok"),
		"{shown:?} log: {}",
		ran(&mut sh, "sshd log")
	);
	// A password, given by ssh's askpass helper; a wrong one is refused.
	let asker = |password: &str| {
		let script = home.join(format!("askpass-{password}"));
		fs::write(&script, format!("#!/bin/sh\necho {password}\n")).unwrap();
		fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
		script
	};
	let by_password = |password: &str| {
		let mut command = ssh(
			&[
				"-o",
				"PreferredAuthentications=password",
				"-o",
				"PubkeyAuthentication=no",
				"-o",
				"NumberOfPasswordPrompts=1",
			],
			"echo password-ok",
		);
		command
			.env("SSH_ASKPASS", asker(password))
			.env("SSH_ASKPASS_REQUIRE", "force")
			.env("DISPLAY", ":0");
		command.output().unwrap()
	};
	let good = by_password("open-sesame");
	assert!(
		String::from_utf8_lossy(&good.stdout).contains("password-ok"),
		"{good:?}"
	);
	let bad = by_password("wrong");
	assert!(!bad.status.success(), "a wrong password is refused");
	let log = ran(&mut sh, "sshd log");
	assert!(
		log.contains("Password auth succeeded") || log.contains("auth succeeded"),
		"{log}"
	);
	assert_eq!(ran(&mut sh, "sshd stop"), "sshd: stopped\n");
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
	device_with(name, None, None, keys)
}

/// `device`, with the programs of `extra` (a directory of command
/// libraries) found first on `$PATH`, and `terminfo` as `$TERMINFO`.
fn device_with(
	name: &'static str,
	extra: Option<&str>,
	terminfo: Option<&std::path::Path>,
	keys: &'static [&'static [u8]],
) -> Option<String> {
	let (shell, _) = real_shell(name)?;
	Some(device_in(shell, extra, terminfo, keys))
}

/// `device_with` for a shell already prepared (a home with keys in it).
/// A program that does not end would hang the test run, and one that
/// never stops printing would fill memory: the screen keeps a bounded
/// amount, and a watchdog gives up loudly after a minute.
fn device_in(
	mut shell: Shell,
	extra: Option<&str>,
	terminfo: Option<&std::path::Path>,
	keys: &'static [&'static [u8]],
) -> String {
	if let Some(terminfo) = terminfo {
		shell
			.env
			.insert("TERMINFO".into(), terminfo.display().to_string());
	}
	if let Some(extra) = extra {
		let path = format!("{extra}:{}", shell.env["PATH"]);
		shell.env.insert("PATH".into(), path);
	}
	let (input, mut typing) = io::pipe().unwrap();
	let screen = crate::inproc::tests::Screen::default();
	let shown = screen.clone();
	let typist = thread::spawn(move || {
		for chunk in keys {
			thread::sleep(std::time::Duration::from_millis(400));
			if typing.write_all(chunk).is_err() {
				return;
			}
		}
	});
	crate::sys::signal::quiet_job_signals();
	let (done, finished) = std::sync::mpsc::channel::<()>();
	let on_screen = shown.clone();
	let watchdog = thread::spawn(move || {
		if finished
			.recv_timeout(std::time::Duration::from_secs(60))
			.is_err()
		{
			// stderr belongs to the program running: leave a note in a file.
			let note = std::env::temp_dir().join("poopterm-device-test-timeout.txt");
			let _ = fs::write(note, on_screen.shown());
			std::process::abort();
		}
	});
	crate::inproc::run(
		shell,
		Arc::new(crate::inproc::Size::new(80, 24)),
		input,
		screen,
	);
	let _ = done.send(());
	watchdog.join().unwrap();
	typist.join().unwrap();
	shown.shown()
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
