//! The package index: its lines, the names and fields it accepts, and the
//! URLs packages may come from.
use super::index::{Package, allowed, archive_url, parse, valid_name};

const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn line(name: &str, version: &str, summary: &str) -> String {
	format!("{name} {version} {HASH} 1234 {name}-{version}.tar.gz {summary}\n")
}

#[test]
fn each_line_is_a_package_sorted_by_name() {
	let text = format!(
		"# Poopterm packages\n\n{}{}",
		line("jot", "1.0", "Print sequential or random data"),
		line("hexdump", "2.1-apple", "Show a file's bytes"),
	);
	let packages = parse(&text).unwrap();
	let names: Vec<_> = packages
		.iter()
		.map(|package| package.name.as_str())
		.collect();
	assert_eq!(names, ["hexdump", "jot"]);
	let jot = &packages[1];
	assert_eq!(
		jot,
		&Package {
			name: "jot".into(),
			version: "1.0".into(),
			sha256: [
				0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9,
				0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52,
				0xb8, 0x55
			],
			size: 1234,
			archive: "jot-1.0.tar.gz".into(),
			summary: "Print sequential or random data".into(),
		}
	);
}

#[test]
fn a_package_without_a_summary_has_an_empty_one() {
	let text = format!("jot 1 {HASH} 10 jot.tar.gz\n");
	assert_eq!(parse(&text).unwrap()[0].summary, "");
	let tabs = format!("jot\t1\t{HASH}\t10\tjot.tar.gz\tTabs too\n");
	assert_eq!(parse(&tabs).unwrap()[0].summary, "Tabs too");
}

#[test]
fn an_empty_index_has_no_packages() {
	assert_eq!(parse("").unwrap(), []);
	assert_eq!(parse("# nothing yet\n\n").unwrap(), []);
}

#[test]
fn a_bad_line_is_reported_with_its_number() {
	let cases = [
		(format!("jot 1 {HASH}\n"), "missing"),
		(format!("jot 1 {} 10 a.tar.gz\n", &HASH[..63]), "hash"),
		(format!("jot 1 {}g 10 a.tar.gz\n", &HASH[..63]), "hash"),
		(format!("jot 1 {HASH} ten a.tar.gz\n"), "size"),
		(format!("Jot 1 {HASH} 10 a.tar.gz\n"), "name"),
		(format!("../x 1 {HASH} 10 a.tar.gz\n"), "name"),
	];
	for (text, word) in cases {
		let text = format!("# header\n{text}");
		let error = parse(&text).unwrap_err();
		assert!(error.starts_with("line 2:"), "{error}");
		assert!(error.contains(word), "{error} should mention {word}");
	}
}

#[test]
fn a_package_listed_twice_is_refused() {
	let text = line("jot", "1", "a") + &line("jot", "2", "b");
	assert!(parse(&text).unwrap_err().contains("jot"));
}

#[test]
fn names_are_lower_case_words_a_file_can_be_named_after() {
	for name in ["jot", "fastfetch", "c++filt", "py3.12", "x-y_z", "7z"] {
		assert!(valid_name(name), "{name}");
	}
	let long = "a".repeat(65);
	for name in [
		"",
		"Jot",
		"-rf",
		".hidden",
		"a/b",
		"a b",
		"..",
		"é",
		long.as_str(),
	] {
		assert!(!valid_name(name), "{name}");
	}
}

#[test]
fn an_archive_path_is_taken_from_the_indexs_directory() {
	let index = "https://example.org/poopterm/arm64/index";
	assert_eq!(
		archive_url(index, "jot-1.tar.gz"),
		"https://example.org/poopterm/arm64/jot-1.tar.gz"
	);
	assert_eq!(
		archive_url(index, "https://cdn.example.org/jot.tar.gz"),
		"https://cdn.example.org/jot.tar.gz"
	);
	assert_eq!(
		archive_url("file:///tmp/repo/index", "a.tar.gz"),
		"file:///tmp/repo/a.tar.gz"
	);
}

#[test]
fn packages_come_over_https_files_or_this_machine_only() {
	for url in [
		"https://example.org/index",
		"file:///var/mobile/repo/index",
		"http://localhost:8000/index",
		"http://127.0.0.1/index",
	] {
		assert!(allowed(url), "{url}");
	}
	for url in [
		"http://example.org/index",
		"http://localhost.evil.org/index",
		"ftp://example.org/index",
		"index",
		"",
	] {
		assert!(!allowed(url), "{url}");
	}
}
