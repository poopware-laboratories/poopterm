//! Tests of quick select's model: what counts as a target and where it
//! begins and ends, the labels given, and what typed letters pick.
use super::hints::{Hint, MAX_HINTS, Pick, Target, find_targets, label, pick, rows_of};
use crate::{
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

/// `lines` as rows of characters, each in its own column.
fn rows(lines: &[&str]) -> Vec<Vec<(char, usize)>> {
	lines
		.iter()
		.map(|line| line.chars().zip(0..).collect())
		.collect()
}

/// The texts of the targets found in `line`.
fn found(line: &str) -> Vec<String> {
	find_targets(&rows(&[line]))
		.into_iter()
		.map(|t| t.text)
		.collect()
}

#[test]
fn urls_of_any_scheme_are_targets() {
	assert_eq!(
		found("see https://example.com/a?b=1#c now"),
		["https://example.com/a?b=1#c"]
	);
	assert_eq!(found("ssh://git@host:22/repo"), ["ssh://git@host:22/repo"]);
	assert_eq!(found("git+ssh://x"), ["git+ssh://x"]);
	assert!(found("://nothing 1://digit http://").is_empty());
}

#[test]
fn paths_are_targets_and_slashes_alone_are_not() {
	assert_eq!(
		found("src/app/mod.rs:42 ~/notes.txt /etc/hosts ./run ../up"),
		[
			"src/app/mod.rs:42",
			"~/notes.txt",
			"/etc/hosts",
			"./run",
			"../up"
		]
	);
	// Fractions, lone slashes and comment markers are not paths.
	assert!(
		found("1/2 / // //comment a/").is_empty(),
		"{:?}",
		found("1/2 / // //comment a/")
	);
}

#[test]
fn commit_hashes_need_digits_and_letters() {
	assert_eq!(
		found("commit 9b9e490 and 173b8396e1d2"),
		["9b9e490", "173b8396e1d2"]
	);
	let full = "9b9e4908196c1c091938e3d21689aa603cbd6edd";
	assert_eq!(found(full), [full]);
	// Too short, too long, words, numbers, and upper-case hex are not.
	let long = "a1".repeat(21);
	for word in ["9b9e49", &long, "defaced", "1234567", "DEADBEEF1"] {
		assert!(found(word).is_empty(), "{word}");
	}
}

#[test]
fn ipv4_addresses_with_or_without_ports_are_targets() {
	assert_eq!(
		found("ping 192.168.1.10 or 10.0.0.1:8080"),
		["192.168.1.10", "10.0.0.1:8080"]
	);
	// A colon ending a sentence is punctuation, not a port.
	assert_eq!(found("reach 1.2.3.4: done"), ["1.2.3.4"]);
	for word in [
		"256.1.1.1",
		"1.2.3",
		"1.2.3.4.5",
		"1.2.3.4:70000",
		"1.2.3.4:x",
		"1..2.3",
		"v1.2.3.4",
	] {
		assert!(found(word).is_empty(), "{word}");
	}
}

#[test]
fn surrounding_punctuation_and_quotes_are_left_out() {
	assert_eq!(found("(see https://a.b/c)."), ["https://a.b/c"]);
	assert_eq!(
		found("\"/tmp/x\", '/tmp/y'; <http://z.io>"),
		["/tmp/x", "/tmp/y", "http://z.io"]
	);
	assert_eq!(found("[src/a.rs]: done!"), ["src/a.rs"]);
	// A bracket the text opened itself stays.
	assert_eq!(
		found("https://en.wikipedia.org/wiki/Rust_(language)."),
		["https://en.wikipedia.org/wiki/Rust_(language)"]
	);
	assert_eq!(found("{src/x/[id].rs}"), ["src/x/[id].rs"]);
}

#[test]
fn pipes_and_box_lines_split_words() {
	assert_eq!(found("│/usr/bin│9b9e490│"), ["/usr/bin", "9b9e490"]);
	assert_eq!(found("a/b|c/d"), ["a/b", "c/d"]);
}

#[test]
fn targets_know_their_row_and_columns() {
	let targets = find_targets(&rows(&["", "  ~/a/b x", "1.2.3.4"]));
	assert_eq!(
		targets,
		[
			Target {
				row: 1,
				cols: (2, 6),
				text: "~/a/b".into()
			},
			Target {
				row: 2,
				cols: (0, 6),
				text: "1.2.3.4".into()
			},
		]
	);
}

#[test]
fn wide_characters_keep_their_own_columns() {
	// A frame skips the second cell of a wide character, so columns jump.
	let mut term = Term::new(20, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	crate::parser::Parser::new().advance(&mut term, "漢字 /a/b".as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	let found = find_targets(&rows_of(&frame));
	assert_eq!(
		found,
		[Target {
			row: 0,
			cols: (5, 8),
			text: "/a/b".into()
		}]
	);
	assert_eq!(rows_of(&frame).len(), 2);
	assert_eq!(rows_of(&frame)[0][..3], [('漢', 0), ('字', 2), (' ', 4)]);
}

fn target(row: usize, text: &str) -> Target {
	Target {
		row,
		cols: (0, text.len() - 1),
		text: text.into(),
	}
}

fn labels(hints: &[Hint]) -> Vec<(&str, &str)> {
	hints
		.iter()
		.map(|hint| (hint.label.as_str(), hint.target.text.as_str()))
		.collect()
}

#[test]
fn the_targets_nearest_the_bottom_get_the_first_letters() {
	let hints = label(vec![
		target(0, "/top"),
		target(5, "/middle"),
		target(9, "/bottom"),
	]);
	assert_eq!(
		labels(&hints),
		[("d", "/top"), ("s", "/middle"), ("a", "/bottom")]
	);
}

#[test]
fn the_same_text_shares_one_label() {
	let hints = label(vec![target(0, "/a"), target(1, "/b"), target(2, "/a")]);
	assert_eq!(labels(&hints), [("a", "/a"), ("s", "/b"), ("a", "/a")]);
}

#[test]
fn more_targets_than_letters_all_get_two_letters() {
	let targets: Vec<Target> = (0..27).map(|i| target(i, &format!("/t{i}"))).collect();
	let hints = label(targets);
	assert_eq!(hints.len(), 27);
	assert!(
		hints.iter().all(|hint| hint.label.len() == 2),
		"no label is another's start"
	);
	assert_eq!(hints[26].label, "aa", "the bottom one");
	assert_eq!(hints[0].label, "sa", "the top one, 27th from the bottom");
	let mut seen: Vec<&str> = hints.iter().map(|hint| hint.label.as_str()).collect();
	seen.sort_unstable();
	seen.dedup();
	assert_eq!(seen.len(), 27);
}

#[test]
fn beyond_every_two_letter_label_the_topmost_go_unlabelled() {
	let targets: Vec<Target> = (0..MAX_HINTS + 3)
		.map(|i| target(i, &format!("/t{i}")))
		.collect();
	let hints = label(targets);
	assert_eq!(hints.len(), MAX_HINTS);
	assert_eq!(
		hints[0].target.text, "/t3",
		"the three at the top are left out"
	);
	assert_eq!(hints[MAX_HINTS - 1].label, "aa");
}

#[test]
fn no_targets_have_no_hints() {
	assert!(label(Vec::new()).is_empty());
	assert!(find_targets(&rows(&["plain words only", ""])).is_empty());
}

#[test]
fn a_whole_label_picks_its_text_and_a_start_narrows() {
	let targets: Vec<Target> = (0..30).map(|i| target(i, &format!("/t{i}"))).collect();
	let hints = label(targets);
	assert_eq!(pick(&hints, "aa"), Pick::Chosen("/t29"));
	assert_eq!(
		pick(&hints, "AS"),
		Pick::Chosen("/t28"),
		"a capital letter picks too"
	);
	match pick(&hints, "s") {
		Pick::Narrowed(left) => {
			let labels: Vec<&str> = left.iter().map(|hint| hint.label.as_str()).collect();
			// In screen order: the topmost has the last label.
			assert_eq!(labels, ["sf", "sd", "ss", "sa"]);
		}
		other => panic!("{other:?}"),
	}
	// A capital first letter narrows as a small one does.
	let capital = pick(&hints, "S");
	assert!(
		matches!(&capital, Pick::Narrowed(left) if left.len() == 4),
		"{capital:?}"
	);
	assert_eq!(pick(&hints, "z"), Pick::Nothing, "no label starts with z");
	assert_eq!(pick(&hints, "aaa"), Pick::Nothing);
	assert!(matches!(pick(&hints, ""), Pick::Narrowed(left) if left.len() == 30));
}

#[test]
fn a_single_letter_label_is_chosen_at_once() {
	let hints = label(vec![target(0, "/x"), target(1, "/y")]);
	assert_eq!(pick(&hints, "a"), Pick::Chosen("/y"));
	assert_eq!(pick(&hints, "s"), Pick::Chosen("/x"));
	assert_eq!(pick(&[], "a"), Pick::Nothing);
}

#[test]
fn signs_and_spaces_do_not_make_numbers() {
	// Rust's number parsing takes a leading `+`; an address part may not.
	for word in [
		"+1.2.3.4",
		"1.+2.3.4",
		"1.2.3.4:+80",
		"1.2.3.4:-1",
		"1. 2.3.4",
	] {
		let found = found(word);
		assert!(found.is_empty(), "{word}: {found:?}");
	}
}

#[test]
fn git_log_lines_give_their_hashes_and_nothing_from_the_subject() {
	assert_eq!(
		found("9b9e490 feat(fonts): draw layered COLR version 0 color glyphs (#363)"),
		["9b9e490"]
	);
	assert_eq!(
		found("* d145832 (HEAD -> main, origin/main) feat(pty): pixels (#365)"),
		["d145832", "origin/main"],
		"a remote branch reads as a path, as it can be checked out by it"
	);
}

#[test]
fn compiler_errors_give_the_place_with_its_line_and_column() {
	assert_eq!(
		found("  --> src/ui/hints.rs:132:9"),
		["src/ui/hints.rs:132:9"]
	);
	assert_eq!(
		found("error[E0308]: mismatched types in /home/me/x/main.rs:3:5:"),
		["/home/me/x/main.rs:3:5"],
		"the colon ending the sentence is left out"
	);
	assert_eq!(
		found("   = help: see https://doc.rust-lang.org/error_codes/E0308.html"),
		["https://doc.rust-lang.org/error_codes/E0308.html"]
	);
}

#[test]
fn listings_and_status_give_their_paths() {
	assert_eq!(
		found("-rw-r--r--  1 me  staff  1234 Nov 21 10:00 ./target/debug/poopterm"),
		["./target/debug/poopterm"],
		"permissions, sizes and times are not targets"
	);
	assert_eq!(found("\tmodified:   src/app/mod.rs"), ["src/app/mod.rs"]);
	assert_eq!(
		found("renamed: docs/old.md -> docs/new.md"),
		["docs/old.md", "docs/new.md"]
	);
}

#[test]
fn network_output_gives_its_addresses() {
	assert_eq!(
		found("64 bytes from 142.250.74.46: icmp_seq=0 ttl=117 time=12.3 ms"),
		["142.250.74.46"]
	);
	assert_eq!(
		found("Listening on http://127.0.0.1:3000/"),
		["http://127.0.0.1:3000/"]
	);
	assert_eq!(
		found("tcp 0 0 0.0.0.0:22 0.0.0.0:* LISTEN"),
		["0.0.0.0:22"],
		"`:*` is no port"
	);
}

#[test]
fn hash_lengths_and_ports_stop_exactly_at_their_limits() {
	let at = |n: usize| format!("{}{}", "a".repeat(n - 1), 1);
	assert_eq!(found(&at(7)), [at(7)], "the shortest hash");
	assert_eq!(found(&at(40)), [at(40)], "a whole SHA-1");
	assert!(found(&at(41)).is_empty(), "longer than any SHA-1");
	assert_eq!(found("1.2.3.4:65535"), ["1.2.3.4:65535"]);
	assert!(found("1.2.3.4:65536").is_empty());
	assert!(found("1.2.3.4:100000").is_empty(), "six digits");
	assert!(
		found("1.2.3.4:000080").is_empty(),
		"six digits, though a small number"
	);
	assert!(found("1.2.3.0001").is_empty(), "four digits in a part");
}

#[test]
fn urls_inside_markup_start_at_their_scheme() {
	assert_eq!(
		found("see [the docs](https://x.y/z) here"),
		["https://x.y/z"]
	);
	assert_eq!(found("url=https://a.b/c?d=1"), ["https://a.b/c?d=1"]);
	assert_eq!(found("<a href=https://a.b>"), ["https://a.b"]);
	assert_eq!(found("(git+ssh://h/r)."), ["git+ssh://h/r"]);
	// Its columns start at the scheme too.
	let targets = find_targets(&rows(&["[x](http://a.b)"]));
	assert_eq!(targets[0].cols, (4, 13));
}

#[test]
fn a_url_is_one_target_with_the_hashes_and_paths_inside_it() {
	let commit = "https://github.com/o/r/commit/9b9e4908196c";
	assert_eq!(found(commit), [commit]);
	assert_eq!(found("http://10.0.0.1:8080/x"), ["http://10.0.0.1:8080/x"]);
}

#[test]
fn paths_and_urls_may_hold_any_letters() {
	assert_eq!(
		found("open /home/josé/файл.txt now"),
		["/home/josé/файл.txt"]
	);
	assert_eq!(found("https://例え.jp/パス"), ["https://例え.jp/パス"]);
	// Digits alone between slashes are dates or ratios, not paths.
	assert!(found("2026/11/21 1/2/3").is_empty());
}

#[test]
fn rows_of_separators_or_brackets_alone_have_no_targets() {
	for line in ["   |  │ ", "()[]{}", "(((", ")))", ".,;:", "\"'`<>"] {
		assert!(find_targets(&rows(&[line])).is_empty(), "{line:?}");
	}
	assert!(find_targets(&[]).is_empty());
	assert!(find_targets(&[Vec::new(), Vec::new()]).is_empty());
}

#[test]
fn schemes_hold_dots_dashes_and_pluses_but_start_with_a_letter() {
	assert_eq!(
		found("[x](com.example-app+v2://open)"),
		["com.example-app+v2://open"]
	);
	assert_eq!(
		found("v=2https://a.b"),
		["https://a.b"],
		"a digit cannot start a scheme"
	);
	assert_eq!(found("=-https://a.b"), ["https://a.b"]);
}
