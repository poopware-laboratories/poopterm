//! A command's words grouped into the stages of a pipeline (`a | b | c`),
//! each with the file it reads (`<`) and the file it writes (`>`, `>>`).
//! The in-process shell runs stages one after another, each reading what
//! the one before printed, since only one program runs at a time.
use super::shell::Word;

/// One program of a pipeline and its redirections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stage {
	pub argv: Vec<String>,
	/// `< file`: standard input from a file instead of the stage before.
	pub input: Option<String>,
	/// `> file` or `>> file` (true: append).
	pub output: Option<(String, bool)>,
}

/// The stages of `words`, none for no words; an operator without its file
/// or a `|` without a command on each side is an error to show.
pub(crate) fn stages(words: Vec<Word>) -> Result<Vec<Stage>, &'static str> {
	let mut stages = vec![Stage::default()];
	let mut words = words.into_iter();
	while let Some(word) = words.next() {
		let stage = stages.last_mut().unwrap_or_else(|| unreachable!());
		match word {
			Word::Text(text) => stage.argv.push(text),
			Word::Op("|") => {
				if stage.argv.is_empty() {
					return Err("a command must come before |");
				}
				stages.push(Stage::default());
			}
			Word::Op(op) => {
				let Some(Word::Text(file)) = words.next() else {
					return Err("a file must follow <, > or >>");
				};
				if op == "<" {
					stage.input = Some(file);
				} else {
					stage.output = Some((file, op == ">>"));
				}
			}
		}
	}
	match &stages[..] {
		[only] if only.argv.is_empty() && only.input.is_none() && only.output.is_none() => {
			Ok(Vec::new())
		}
		[_, .., last] if last.argv.is_empty() => Err("a command must follow |"),
		_ => Ok(stages),
	}
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
