//! `--app-id`: the Wayland app id and X11 class a window is opened with, so
//! compositor rules can tell one Poopterm from another.
use super::{
	Command, MAX_APP_ID, Paint, documented, error, parse, property_tests::Random, suggest,
};

fn run(list: &[&str]) -> Result<Command, String> {
	parse(list.iter().map(|arg| arg.to_string()))
}

fn with_id(id: &str) -> Result<Command, String> {
	Ok(Command::Run {
		app_id: Some(id.into()),
	})
}

#[test]
fn an_app_id_opens_the_window_with_it() {
	assert_eq!(run(&["--app-id", "dropdown"]), with_id("dropdown"));
	assert_eq!(
		run(&["--app-id", "org.example.Scratch_2"]),
		with_id("org.example.Scratch_2")
	);
	assert_eq!(run(&["--app-id", "a-b"]), with_id("a-b"));
	assert_eq!(run(&["--app-id", "7"]), with_id("7"));
	assert_eq!(run(&[]), Ok(Command::Run { app_id: None }));
}

#[test]
fn an_app_id_may_be_given_with_an_equals_sign() {
	assert_eq!(run(&["--app-id=dropdown"]), with_id("dropdown"));
	assert!(run(&["--app-id="]).is_err());
}

#[test]
fn an_app_id_is_at_most_255_characters() {
	let longest = "a".repeat(MAX_APP_ID);
	assert_eq!(MAX_APP_ID, 255);
	assert_eq!(run(&["--app-id", &longest]), with_id(&longest));
	let message = run(&["--app-id", &format!("{longest}a")]).unwrap_err();
	assert!(message.contains("255"), "{message}");
}

#[test]
fn an_app_id_holds_only_letters_digits_dots_underscores_and_dashes() {
	for bad in [
		"",
		"two words",
		"a/b",
		"tab\tid",
		"é",
		"a;b",
		"$HOME",
		"a\u{0}b",
		".hidden",
		"-x",
		"--help",
	] {
		let message = run(&["--app-id", bad]).unwrap_err();
		assert!(message.starts_with("--app-id "), "{bad:?}: {message}");
	}
}

#[test]
fn a_bad_app_id_says_what_is_allowed() {
	let message = run(&["--app-id", "a b"]).unwrap_err();
	assert_eq!(
		error(&message, Paint(false)),
		"error: --app-id \"a b\" is not an app id\n  \
		 Use 1 to 255 letters, digits, dots, underscores and dashes, such as dropdown.\n"
	);
}

#[test]
fn an_app_id_needs_a_value() {
	let message = run(&["--app-id"]).unwrap_err();
	assert_eq!(
		error(&message, Paint(false)),
		"error: --app-id needs a name\n  \
		 For example: poopterm --app-id dropdown\n"
	);
}

#[test]
fn nothing_may_follow_the_app_id() {
	let message = run(&["--app-id", "a", "--help"]).unwrap_err();
	assert!(
		message.starts_with("unexpected argument --help"),
		"{message}"
	);
}

#[test]
fn values_that_look_like_options_are_not_app_ids() {
	// `-h` after --app-id is a bad name, not a request for help.
	for value in ["-h", "--version", "--app-id=x", "-"] {
		assert!(run(&["--app-id", value]).is_err(), "{value}");
	}
	assert!(run(&["--app-id=--help"]).is_err());
	assert!(run(&["--app-id=a=b"]).is_err(), "`=` is not allowed");
	// Only the exact option takes a value: a longer name is unknown.
	assert!(
		run(&["--app-ids", "x"])
			.unwrap_err()
			.starts_with("unknown option")
	);
}

#[test]
fn mistyped_app_id_options_are_suggested() {
	let options = documented("OPTIONS");
	for typo in ["--appid", "--app_id", "--APP-ID", "--app-di", "-app-id"] {
		assert_eq!(suggest(typo, &options), Some("--app-id"), "{typo}");
	}
}

/// The rule, said another way: a name of allowed characters that a
/// letter, digit or underscore starts.
fn allowed(id: &str) -> bool {
	let first_ok = id
		.chars()
		.next()
		.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
	let rest_ok = id
		.bytes()
		.all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
	first_ok && rest_ok && id.len() <= MAX_APP_ID
}

#[test]
fn random_names_are_accepted_exactly_when_the_rule_allows_them() {
	let alphabet = [
		"a", "Z", "0", "9", "_", "-", ".", " ", "/", "é", "=", "\t", ":",
	];
	let mut random = Random(0x5eed_a991d);
	let (mut accepted, mut refused) = (0, 0);
	for _ in 0..5000 {
		let length = [0, 1, 2, 5, 254, 255, 256][random.below(7)];
		let id: String = (0..length)
			.map(|_| alphabet[random.below(alphabet.len())])
			.collect();
		let parsed = run(&["--app-id", &id]);
		assert_eq!(parsed.is_ok(), allowed(&id), "{id:?}");
		if parsed.is_ok() {
			assert_eq!(parsed, with_id(&id));
			accepted += 1;
		} else {
			refused += 1;
		}
	}
	// Both sides of the rule were exercised.
	assert!(
		accepted > 50 && refused > 50,
		"{accepted} accepted, {refused} refused"
	);
}
