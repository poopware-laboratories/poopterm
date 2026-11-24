use std::fs;

use super::*;

#[test]
fn stars_and_question_marks_match_runs_and_single_characters() {
	for (pattern, name, expected) in [
		("*.o", "main.o", true),
		("*.o", ".o", true),
		("*.o", "main.c", false),
		("*", "", true),
		("a*b*c", "aXXbYYc", true),
		("a*b*c", "aXXbYY", false),
		("a?c", "abc", true),
		("a?c", "ac", false),
		("???", "héé", true),
		("", "", true),
		("", "x", false),
	] {
		assert_eq!(matches(pattern, name), expected, "{pattern} on {name}");
	}
}

#[test]
fn brackets_match_sets_ranges_and_negations() {
	for (pattern, name, expected) in [
		("[abc]", "b", true),
		("[abc]", "d", false),
		("[a-c]x", "bx", true),
		("[a-c]x", "dx", false),
		("[!a-c]", "d", true),
		("[^a-c]", "a", false),
		("[]a]", "]", true),
		("[a-]", "-", true),
		("[\\]]", "]", true),
		("file[0-9][0-9]", "file42", true),
		("file[0-9][0-9]", "file4", false),
	] {
		assert_eq!(matches(pattern, name), expected, "{pattern} on {name}");
	}
}

#[test]
fn an_unclosed_bracket_and_a_trailing_backslash_are_plain_characters() {
	assert!(matches("[abc", "[abc"));
	assert!(!matches("[abc", "a"));
	assert!(matches("a\\", "a\\"));
	assert!(matches("a\\*", "a*"));
	assert!(!matches("a\\*", "abc"));
	assert!(matches("\\?", "?"));
}

#[test]
fn only_unescaped_pattern_characters_make_a_pattern() {
	assert!(is_pattern("*.c") && is_pattern("a?") && is_pattern("[x]"));
	assert!(!is_pattern("plain") && !is_pattern("a\\*b") && !is_pattern("trailing\\"));
	assert_eq!(unescape("a\\*b\\\\c"), "a*b\\c");
	assert_eq!(unescape("end\\"), "end\\");
}

fn scratch(name: &str) -> std::path::PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-glob-{name}-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(dir.join("src/deep")).unwrap();
	for file in [
		"b.o",
		"a.o",
		"a.c",
		".hidden.o",
		"src/x.o",
		"src/y.c",
		"src/deep/z.o",
		"star*.o",
	] {
		fs::write(dir.join(file), "").unwrap();
	}
	dir.canonicalize().unwrap()
}

#[test]
fn expansion_lists_sorted_matches_and_hides_dotfiles() {
	let dir = scratch("sorted");
	assert_eq!(expand("*.o", &dir), ["a.o", "b.o", "star*.o"]);
	assert_eq!(expand(".*.o", &dir), [".hidden.o"]);
	assert_eq!(expand("?.c", &dir), ["a.c"]);
	assert!(expand("*.h", &dir).is_empty(), "no match, no words");
	fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn expansion_walks_directories_in_the_pattern() {
	let dir = scratch("walk");
	assert_eq!(expand("src/*.o", &dir), ["src/x.o"]);
	assert_eq!(expand("*/*.o", &dir), ["src/x.o"]);
	assert_eq!(expand("src/*/z.o", &dir), ["src/deep/z.o"]);
	assert_eq!(
		expand("src/deep/z.o", &dir),
		["src/deep/z.o"],
		"a plain path that exists"
	);
	assert!(
		expand("src/deep/nope.o", &dir).is_empty(),
		"a plain path that does not"
	);
	assert_eq!(expand("nodir/*.o", &dir), Vec::<String>::new());
	let absolute = format!("{}/src/*.c", dir.display());
	assert_eq!(
		expand(&absolute, Path::new("/nowhere")),
		[format!("{}/src/y.c", dir.display())]
	);
	fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_escaped_character_names_a_file_literally() {
	let dir = scratch("escape");
	assert_eq!(expand("star\\*.o", &dir), ["star*.o"]);
	assert_eq!(expand("star[*].o", &dir), ["star*.o"]);
	fs::remove_dir_all(&dir).unwrap();
}
