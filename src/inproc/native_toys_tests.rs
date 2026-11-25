//! The small popular programs packaged for the device (figlet, lolcat,
//! cpufetch, nyancat, ncdu, sl, cmatrix), run through the in-process shell
//! when `ios/packages/build.sh macosx` has built them.
use std::{fs, path::PathBuf};

use super::{device_in, ran, real_shell, scratch};
use crate::inproc::shell::Shell;

/// Where `ios/packages/build.sh macosx <name>` leaves a package, if it has.
fn package(name: &str) -> Option<PathBuf> {
	let dir = PathBuf::from(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/target/ios-packages/work-macosx"
	))
	.join(name);
	if dir.join("bin").join(name).is_file() {
		Some(dir)
	} else {
		eprintln!("skipped: run ios/packages/build.sh macosx {name}");
		None
	}
}

/// A shell with the package `name` first on `$PATH`, and its folder.
fn shell_with(name: &str) -> Option<(Shell, PathBuf, PathBuf)> {
	let dir = package(name)?;
	let (mut sh, home) = real_shell(name)?;
	let path = format!("{}:{}", dir.join("bin").display(), sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	Some((sh, home, dir))
}

/// `text` without its escape sequences (CSI ones, as these programs use).
fn plain(text: &str) -> String {
	let mut out = String::new();
	let mut chars = text.chars();
	while let Some(c) = chars.next() {
		if c != '\x1b' {
			out.push(c);
			continue;
		}
		if chars.next() == Some('[') {
			for c in chars.by_ref() {
				if ('@'..='~').contains(&c) {
					break;
				}
			}
		}
	}
	out
}

#[test]
fn plain_drops_csi_sequences_and_keeps_the_text() {
	assert_eq!(plain("\x1b[38;5;83mhell\x1b[0mo\n"), "hello\n");
	assert_eq!(plain("a\x1b[?25hb\x1b[Hc"), "abc");
	assert_eq!(plain("no escapes"), "no escapes");
	assert_eq!(
		plain("cut \x1b[12"),
		"cut ",
		"an unfinished sequence is dropped"
	);
}

/// FIGlet's own rendering of "Hi" in its standard font.
const FIGLET_HI: &str = " _   _ _ \n| | | (_)\n| |_| | |\n|  _  | |\n|_| |_|_|\n         \n";

pub(super) fn figlet_draws_with_the_fonts_beside_it() {
	let Some((mut sh, _, dir)) = shell_with("figlet") else {
		return;
	};
	let fonts = dir.join("share/figlet");
	assert!(
		fonts.join("standard.flf").is_file(),
		"its fonts are packaged"
	);
	assert_eq!(
		ran(&mut sh, "figlet Hi; echo status $?"),
		"figlet: standard: Unable to open font file\nstatus 1\n",
		"without FIGLET_FONTDIR it looks in ./fonts"
	);
	sh.env
		.insert("FIGLET_FONTDIR".into(), fonts.display().to_string());
	assert_eq!(
		ran(&mut sh, "figlet Hi; echo status $?"),
		format!("{FIGLET_HI}status 0\n")
	);
	assert_eq!(
		ran(&mut sh, "figlet -f standard.flf Hi"),
		FIGLET_HI,
		"a font named with its suffix is the same font"
	);
	let slant = ran(&mut sh, "figlet -f slant ok");
	assert!(slant.contains("/ __ \\/ //_/"), "{slant}");
	assert_eq!(
		ran(&mut sh, "figlet -f nope x; echo status $?"),
		"figlet: nope: Unable to open font file\nstatus 1\n"
	);
	assert_eq!(
		ran(&mut sh, "figlet -w 4 -f standard ab | wc -l"),
		"      12\n",
		"too narrow for both letters: one under the other"
	);
}

pub(super) fn lolcat_colours_only_when_asked_or_on_a_terminal() {
	let Some((mut sh, home, _)) = shell_with("lolcat") else {
		return;
	};
	assert_eq!(
		ran(&mut sh, "echo hello | lolcat | cat; echo status $?"),
		"hello\nstatus 0\n",
		"into a pipe, plain"
	);
	let shown = ran(&mut sh, "echo hello | lolcat");
	assert!(
		shown.contains("\x1b[38;5;"),
		"on the terminal, 256 colours: {shown:?}"
	);
	assert_eq!(plain(&shown), "hello\n");
	let forced = ran(&mut sh, "echo hello | lolcat -f | cat");
	assert!(
		forced.contains("\x1b[38;5;"),
		"-f colours a pipe too: {forced:?}"
	);
	let truecolor = ran(&mut sh, "echo hi | lolcat -b");
	assert!(truecolor.contains("\x1b[38;2;"), "24-bit: {truecolor:?}");
	fs::write(home.join("a.txt"), "one\ntwo\n").unwrap();
	assert_eq!(
		ran(&mut sh, "lolcat a.txt missing.txt | cat"),
		"Cannot open input file \"missing.txt\": No such file or directory\none\ntwo\n",
		"a file it cannot open is reported on the screen, the others printed"
	);
}

pub(super) fn cpufetch_names_a_chip_its_tables_do_not_know() {
	let Some((mut sh, _, _)) = shell_with("cpufetch") else {
		return;
	};
	let run = sh.run("cpufetch; echo status $?");
	let shown = plain(&String::from_utf8_lossy(&run.output));
	assert!(shown.ends_with("status 0\n"), "{shown}");
	assert!(!shown.contains("ERROR"), "{shown}");
	let soc = shown
		.lines()
		.find_map(|line| line.split("SoC:").nth(1))
		.map(str::trim)
		.unwrap_or_default();
	assert!(soc.starts_with("Apple "), "the chip by name: {shown}");
	// Both clusters, with the cores the system counts.
	let cores: Vec<u32> = shown
		.lines()
		.filter_map(|line| line.split("Cores:").nth(1))
		.filter_map(|cores| cores.split_whitespace().next()?.parse().ok())
		.collect();
	let total = super::system("sysctl", &["-n", "hw.physicalcpu"]);
	assert_eq!(cores.len(), 2, "{shown}");
	assert_eq!(
		cores.iter().sum::<u32>().to_string(),
		total.trim(),
		"{shown}"
	);
}

pub(super) fn nyancat_draws_the_frames_it_is_asked_for() {
	let Some((mut sh, _, _)) = shell_with("nyancat") else {
		return;
	};
	let shown = ran(&mut sh, "nyancat -f 2 -n -s; echo status $?");
	assert!(shown.starts_with("\x1b["), "it draws with escapes");
	assert!(
		shown.ends_with("\x1b[?25h\x1b[0m\x1b[H\x1b[2Jstatus 0\n"),
		"it ends after two frames and puts the cursor back: {:?}",
		&shown[shown.len().saturating_sub(80)..]
	);
	assert_eq!(
		ran(&mut sh, "nyancat -h >/dev/null; echo status $?"),
		"status 0\n"
	);
}

pub(super) fn ncdu_exports_the_sizes_of_a_folder() {
	let Some((mut sh, home, _)) = shell_with("ncdu") else {
		return;
	};
	fs::create_dir_all(home.join("tree/sub")).unwrap();
	fs::write(home.join("tree/big.bin"), vec![7u8; 100_000]).unwrap();
	fs::write(home.join("tree/sub/small.txt"), "x").unwrap();
	assert_eq!(
		ran(&mut sh, "ncdu -0 -o out.json tree; echo status $?"),
		"status 0\n"
	);
	let json = fs::read_to_string(home.join("out.json")).unwrap();
	assert!(
		json.starts_with("[1,2,{\"progname\":\"ncdu\",\"progver\":\"1.22\""),
		"{json}"
	);
	assert!(
		json.contains("{\"name\":\"big.bin\",\"asize\":100000"),
		"{json}"
	);
	assert!(
		json.contains("{\"name\":\"small.txt\",\"asize\":1"),
		"{json}"
	);
	assert!(
		json.contains("[{\"name\":\"sub\""),
		"a folder is a list: {json}"
	);
	assert_eq!(
		ran(&mut sh, "ncdu -0 -o out.json missing; echo status $?"),
		"Error obtaining full path: No such file or directory.\nstatus 0\n",
		"a folder that is not there is reported (ncdu 1.x still ends with 0)"
	);
}

/// The terminal descriptions the app carries, as `TERMINFO` points at them.
fn terminfo(name: &str) -> PathBuf {
	let dir = scratch(&format!("terminfo-{name}")).join("db");
	let made = std::process::Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ios/terminfo.sh"))
		.arg(&dir)
		.output()
		.unwrap();
	assert!(
		made.status.success(),
		"{}",
		String::from_utf8_lossy(&made.stderr)
	);
	dir
}

/// Runs the full-screen package `name` on the device with `keys` typed.
fn on_the_device(name: &'static str, keys: &'static [&'static [u8]]) -> Option<String> {
	let (sh, _, dir) = shell_with(name)?;
	let terminfo = terminfo(name);
	let bin = dir.join("bin").display().to_string();
	Some(device_in(sh, Some(&bin), Some(&terminfo), keys))
}

pub(super) fn sl_drives_across_and_gives_the_prompt_back() {
	// sl reads and drops keys while it drives (about seven seconds across
	// 80 columns): the next command is on its line, and `exit` waits.
	const WAIT: &[u8] = b"";
	let keys: &[&[u8]] = &[
		b"sl; echo after sl\r",
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		WAIT,
		b"exit\r",
	];
	let Some(shown) = on_the_device("sl", keys) else {
		return;
	};
	assert!(shown.contains("(@@@@)"), "the smoke was drawn");
	assert!(shown.contains("\x1b[?1049h"), "on the alternate screen");
	assert!(
		shown.contains("after sl\r\n"),
		"the shell ran the next line once the train had gone: {:?}",
		&shown[shown.len().saturating_sub(200)..]
	);
}

pub(super) fn cmatrix_rains_until_q() {
	let keys: &[&[u8]] = &[b"cmatrix -b\r", b"q", b"echo after cmatrix\r", b"exit\r"];
	let Some(shown) = on_the_device("cmatrix", keys) else {
		return;
	};
	let matrix = shown.find("cmatrix -b").unwrap();
	let after = shown.rfind("after cmatrix\r\n").expect("q ended it");
	assert!(
		shown[matrix..after].contains("\x1b[32m") || shown[matrix..after].contains("\x1b[1;32m"),
		"it drew in green"
	);
}

pub(super) fn ncdu_browses_and_quits() {
	let keys: &[&[u8]] = &[b"ncdu\r", b"q", b"echo after ncdu\r", b"exit\r"];
	let Some(shown) = on_the_device("ncdu", keys) else {
		return;
	};
	assert!(
		shown.contains("ncdu 1.22 ~ Use the arrow keys"),
		"it drew its title"
	);
	assert!(shown.contains("after ncdu\r\n"), "q quit it");
}

/// What a phone runs them with: not in the IPA, so not signed with it,
/// they are loaded by our own loader (`jit::loader`) once JIT is on.
pub(super) fn downloads_run_through_our_own_loader() {
	crate::inproc::native::FORCE_LOADER.store(true, std::sync::atomic::Ordering::Relaxed);
	figlet_draws_with_the_fonts_beside_it();
	lolcat_colours_only_when_asked_or_on_a_terminal();
	cpufetch_names_a_chip_its_tables_do_not_know();
	nyancat_draws_the_frames_it_is_asked_for();
	ncdu_exports_the_sizes_of_a_folder();
	cmatrix_rains_until_q();
}
