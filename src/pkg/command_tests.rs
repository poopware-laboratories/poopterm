//! The `pkg` command: its subcommands, run against a directory of their
//! own with downloads answered from memory.
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

use super::{
	command::run,
	sha256::{hex, sha256},
	store_tests::archive,
};

const SOURCE: &str = "https://packages.example/arm64/index";

/// A package directory, the files a server holds, and what was asked of it.
struct World {
	root: PathBuf,
	files: BTreeMap<String, Vec<u8>>,
	asked: RefCell<Vec<String>>,
}

impl World {
	fn new(name: &str) -> Self {
		let root = std::env::temp_dir().join(format!("poopterm-pkgcmd-{name}-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&root);
		Self {
			root,
			files: BTreeMap::new(),
			asked: RefCell::new(Vec::new()),
		}
	}

	/// Publishes `name` at `version` with a command of the same name that
	/// holds `body`, and rewrites the index.
	fn publish(&mut self, name: &str, version: &str, body: &[u8], summary: &str) {
		let bytes = archive(&[(&format!("bin/{name}") as &str, body)]);
		let file = format!("{name}-{version}.tar.gz");
		let line = format!(
			"{name} {version} {} {} {file} {summary}\n",
			hex(&sha256(&bytes)),
			bytes.len()
		);
		self
			.files
			.insert(format!("https://packages.example/arm64/{file}"), bytes);
		let index = self.files.entry(SOURCE.to_owned()).or_default();
		let mut text = String::from_utf8(index.clone()).unwrap();
		text = text
			.lines()
			.filter(|old| !old.starts_with(&format!("{name} ")))
			.map(|old| format!("{old}\n"))
			.collect();
		text.push_str(&line);
		*index = text.into_bytes();
	}

	/// Runs `pkg` with `args`, the source given by the environment.
	fn pkg(&self, args: &[&str]) -> (String, i32) {
		self.pkg_from(args, Some(SOURCE))
	}

	fn pkg_from(&self, args: &[&str], source: Option<&str>) -> (String, i32) {
		let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
		let fetch = |url: &str| {
			self.asked.borrow_mut().push(url.to_owned());
			self
				.files
				.get(url)
				.cloned()
				.ok_or_else(|| format!("{url}: not found"))
		};
		run(&args, &self.root, source, &fetch)
	}
}

impl Drop for World {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

#[test]
fn pkg_alone_or_help_explains_every_subcommand() {
	let world = World::new("help");
	for args in [&[][..], &["help"][..], &["--help"][..]] {
		let (out, status) = world.pkg(args);
		assert_eq!(status, 0, "{args:?}");
		for word in [
			"install", "remove", "list", "search", "update", "upgrade", "source",
		] {
			assert!(out.contains(word), "{word} in {out}");
		}
	}
	let (out, status) = world.pkg(&["frobnicate"]);
	assert_eq!(status, 2);
	assert!(out.contains("frobnicate"));
}

#[test]
fn without_a_source_pkg_says_how_to_set_one() {
	let world = World::new("nosource");
	let (out, status) = world.pkg_from(&["update"], None);
	assert_eq!(status, 1);
	assert!(out.contains("pkg source"), "{out}");
	assert!(world.asked.borrow().is_empty());
}

#[test]
fn a_source_set_once_is_remembered_and_shown() {
	let mut world = World::new("source");
	world.publish("jot", "1", b"jot", "Numbers");
	assert_eq!(world.pkg_from(&["source"], None).1, 1, "none yet");
	let (_, status) = world.pkg_from(&["source", SOURCE], None);
	assert_eq!(status, 0);
	assert_eq!(
		world.pkg_from(&["source"], None),
		(format!("{SOURCE}\n"), 0)
	);
	let (out, status) = world.pkg_from(&["update"], None);
	assert_eq!((status, out.as_str()), (0, "1 package available\n"));
}

#[test]
fn a_source_that_is_not_https_is_refused() {
	let world = World::new("insecure");
	let (out, status) = world.pkg_from(&["source", "http://packages.example/index"], None);
	assert_eq!(status, 1);
	assert!(out.contains("https"), "{out}");
	assert_eq!(world.pkg_from(&["source"], None).1, 1, "nothing was saved");
}

#[test]
fn update_fetches_the_index_and_keeps_the_last_good_one() {
	let mut world = World::new("update");
	world.publish("jot", "1", b"j", "Numbers");
	world.publish("hexdump", "1", b"h", "Bytes");
	assert_eq!(
		world.pkg(&["update"]),
		("2 packages available\n".to_owned(), 0)
	);
	world
		.files
		.insert(SOURCE.to_owned(), b"broken line\n".to_vec());
	let (out, status) = world.pkg(&["update"]);
	assert_eq!(status, 1);
	assert!(out.contains("line 1"), "{out}");
	let (list, _) = world.pkg(&["list"]);
	assert!(list.contains("hexdump") && list.contains("jot"), "{list}");
}

#[test]
fn list_shows_each_package_and_marks_those_installed() {
	let mut world = World::new("list");
	assert!(
		world.pkg(&["list"]).0.contains("pkg update"),
		"no index yet"
	);
	world.publish("jot", "1.0", b"j", "Print numbers");
	world.publish("hexdump", "2", b"h", "Show bytes");
	world.pkg(&["update"]);
	world.pkg(&["install", "jot"]);
	let (out, status) = world.pkg(&["list"]);
	assert_eq!(status, 0);
	let lines: Vec<_> = out.lines().collect();
	assert_eq!(lines.len(), 2, "{out}");
	assert!(lines[0].starts_with("hexdump") && lines[0].contains("Show bytes"));
	assert!(!lines[0].contains("installed"));
	assert!(
		lines[1].starts_with("jot") && lines[1].contains("1.0") && lines[1].contains("installed")
	);
}

#[test]
fn search_finds_words_in_names_and_summaries() {
	let mut world = World::new("search");
	world.publish("jot", "1", b"j", "Print sequential numbers");
	world.publish("hexdump", "1", b"h", "Show a file's bytes");
	world.pkg(&["update"]);
	let (out, status) = world.pkg(&["search", "BYTES"]);
	assert_eq!(status, 0);
	assert!(out.contains("hexdump") && !out.contains("jot"), "{out}");
	assert!(world.pkg(&["search", "jo"]).0.contains("jot"));
	let (out, status) = world.pkg(&["search", "zzz"]);
	assert_eq!(status, 1);
	assert!(out.contains("zzz"));
}

#[test]
fn install_fetches_the_index_first_then_each_archive() {
	let mut world = World::new("install");
	world.publish("jot", "1.2", b"jot library", "Numbers");
	let (out, status) = world.pkg(&["install", "jot"]);
	assert_eq!(status, 0, "{out}");
	assert_eq!(out, "installed jot 1.2: jot\n");
	assert_eq!(
		*world.asked.borrow(),
		[SOURCE, "https://packages.example/arm64/jot-1.2.tar.gz"]
	);
	assert_eq!(
		std::fs::read(world.root.join("bin/jot")).unwrap(),
		b"jot library"
	);
	let (again, status) = world.pkg(&["install", "jot"]);
	assert_eq!(
		(again.as_str(), status),
		("jot 1.2 is already installed\n", 0)
	);
}

#[test]
fn an_unknown_package_fails_but_the_others_install() {
	let mut world = World::new("unknown");
	world.publish("jot", "1", b"j", "");
	let (out, status) = world.pkg(&["install", "nope", "jot"]);
	assert_eq!(status, 1);
	assert!(out.contains("nope: no such package"), "{out}");
	assert!(out.contains("installed jot 1"), "{out}");
	assert!(world.root.join("bin/jot").exists());
	assert_eq!(world.pkg(&["install"]).1, 2, "install needs a name");
}

#[test]
fn a_tampered_archive_is_refused() {
	let mut world = World::new("tampered");
	world.publish("jot", "1", b"j", "");
	let url = "https://packages.example/arm64/jot-1.tar.gz".to_owned();
	let mut bytes = world.files[&url].clone();
	let last = bytes.len() - 1;
	bytes[last] ^= 0xff;
	world.files.insert(url, bytes);
	let (out, status) = world.pkg(&["install", "jot"]);
	assert_eq!(status, 1);
	assert!(out.contains("SHA-256"), "{out}");
	assert!(!world.root.join("bin/jot").exists());
}

#[test]
fn a_failed_download_is_reported() {
	let mut world = World::new("download");
	world.publish("jot", "1", b"j", "");
	world.files.retain(|url, _| url == SOURCE);
	let (out, status) = world.pkg(&["install", "jot"]);
	assert_eq!(status, 1);
	assert!(out.contains("not found"), "{out}");
}

#[test]
fn upgrade_installs_only_newer_versions_of_what_is_installed() {
	let mut world = World::new("upgrade");
	world.publish("jot", "1", b"old", "");
	world.publish("hexdump", "1", b"h", "");
	world.pkg(&["install", "jot"]);
	assert_eq!(
		world.pkg(&["upgrade"]),
		("everything is up to date\n".to_owned(), 0)
	);
	world.publish("jot", "2", b"new", "");
	world.publish("hexdump", "2", b"h2", "");
	let (out, status) = world.pkg(&["upgrade"]);
	assert_eq!((out.as_str(), status), ("installed jot 2: jot\n", 0));
	assert_eq!(std::fs::read(world.root.join("bin/jot")).unwrap(), b"new");
	assert!(
		!world.root.join("bin/hexdump").exists(),
		"not installed, so not upgraded"
	);
}

#[test]
fn remove_takes_packages_away_and_names_those_it_cannot() {
	let mut world = World::new("remove");
	world.publish("jot", "1", b"j", "");
	world.pkg(&["install", "jot"]);
	let (out, status) = world.pkg(&["remove", "jot", "ghost"]);
	assert_eq!(status, 1);
	assert!(out.contains("removed jot"), "{out}");
	assert!(out.contains("ghost: not installed"), "{out}");
	assert!(!world.root.join("bin/jot").exists());
	assert_eq!(world.pkg(&["remove"]).1, 2, "remove needs a name");
}

#[test]
fn upgrade_reinstalls_a_package_rebuilt_under_the_same_version() {
	let mut world = World::new("rebuilt");
	world.publish("jot", "1", b"first build", "");
	world.pkg(&["install", "jot"]);
	world.publish("jot", "1", b"second build", "");
	let (out, status) = world.pkg(&["upgrade"]);
	assert_eq!((out.as_str(), status), ("installed jot 1: jot\n", 0));
	assert_eq!(
		std::fs::read(world.root.join("bin/jot")).unwrap(),
		b"second build"
	);
	assert_eq!(
		world.pkg(&["upgrade"]),
		("everything is up to date\n".to_owned(), 0),
		"the same archive again is no change"
	);
}

#[test]
fn an_install_recorded_before_checksums_were_is_reinstalled_once() {
	let mut world = World::new("unhashed");
	world.publish("jot", "1", b"j", "");
	world.pkg(&["install", "jot"]);
	// As pkg wrote the record before it kept checksums: name and version.
	std::fs::write(world.root.join("installed"), "jot 1\n").unwrap();
	assert_eq!(world.pkg(&["upgrade"]).0, "installed jot 1: jot\n");
	assert_eq!(world.pkg(&["upgrade"]).0, "everything is up to date\n");
	assert_eq!(world.pkg(&["list"]).1, 0, "and the record still reads");
}

#[test]
fn the_record_keeps_each_packages_checksum_through_other_installs() {
	let mut world = World::new("hashes");
	world.publish("jot", "1", b"j", "");
	world.publish("hexdump", "1", b"h", "");
	world.pkg(&["install", "jot"]);
	world.pkg(&["install", "hexdump"]);
	world.pkg(&["remove", "hexdump"]);
	let record = std::fs::read_to_string(world.root.join("installed")).unwrap();
	let index = String::from_utf8(world.files[SOURCE].clone()).unwrap();
	let listed = index.lines().find(|line| line.starts_with("jot ")).unwrap();
	let hash = listed.split(' ').nth(2).unwrap();
	assert_eq!(
		record,
		format!("jot 1 {hash}\n"),
		"the archive's, as the index lists it"
	);
	assert_eq!(world.pkg(&["upgrade"]).0, "everything is up to date\n");
}
