//! Pipelines: stages split at `|`, redirections kept with their stage, and
//! the mistakes a shell refuses.
use super::*;

fn text(word: &str) -> Word {
	Word::Text(word.into())
}

fn stage(argv: &[&str]) -> Stage {
	Stage {
		argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
		..Stage::default()
	}
}

#[test]
fn a_command_alone_is_one_stage() {
	assert_eq!(
		stages(vec![text("ls"), text("-l")]),
		Ok(vec![stage(&["ls", "-l"])])
	);
}

#[test]
fn no_words_are_no_stages() {
	assert_eq!(stages(Vec::new()), Ok(Vec::new()));
}

#[test]
fn bars_split_stages_in_order() {
	let words = vec![
		text("cat"),
		text("f"),
		Word::Op("|"),
		text("sort"),
		Word::Op("|"),
		text("uniq"),
	];
	assert_eq!(
		stages(words),
		Ok(vec![
			stage(&["cat", "f"]),
			stage(&["sort"]),
			stage(&["uniq"])
		])
	);
}

#[test]
fn redirections_belong_to_their_stage_wherever_they_stand() {
	let words = vec![
		Word::Op("<"),
		text("in"),
		text("sort"),
		Word::Op("|"),
		text("uniq"),
		Word::Op(">>"),
		text("out"),
		text("-c"),
	];
	let got = stages(words).unwrap();
	assert_eq!(got[0].argv, ["sort"]);
	assert_eq!(got[0].input.as_deref(), Some("in"));
	assert_eq!(got[0].output, None);
	assert_eq!(got[1].argv, ["uniq", "-c"]);
	assert_eq!(got[1].output, Some(("out".to_owned(), true)));
	assert_eq!(got[1].input, None);
}

#[test]
fn the_last_redirection_of_a_kind_wins() {
	let words = vec![
		text("echo"),
		Word::Op(">"),
		text("a"),
		Word::Op(">"),
		text("b"),
	];
	assert_eq!(
		stages(words).unwrap()[0].output,
		Some(("b".to_owned(), false))
	);
}

#[test]
fn a_redirection_alone_still_runs_to_create_the_file() {
	let got = stages(vec![Word::Op(">"), text("empty")]).unwrap();
	assert_eq!(got.len(), 1);
	assert!(got[0].argv.is_empty());
	assert_eq!(got[0].output, Some(("empty".to_owned(), false)));
}

#[test]
fn an_operator_needs_its_file() {
	for op in ["<", ">", ">>"] {
		assert_eq!(
			stages(vec![text("cat"), Word::Op(op)]),
			Err("a file must follow <, > or >>")
		);
		let followed = vec![text("cat"), Word::Op(op), Word::Op("|"), text("x")];
		assert_eq!(
			stages(followed),
			Err("a file must follow <, > or >>"),
			"{op}"
		);
	}
}

#[test]
fn a_bar_needs_a_command_on_each_side() {
	assert_eq!(
		stages(vec![Word::Op("|"), text("ls")]),
		Err("a command must come before |")
	);
	assert_eq!(
		stages(vec![text("ls"), Word::Op("|")]),
		Err("a command must follow |")
	);
	let doubled = vec![text("ls"), Word::Op("|"), Word::Op("|"), text("wc")];
	assert_eq!(stages(doubled), Err("a command must come before |"));
}
