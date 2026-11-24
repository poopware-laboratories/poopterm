//! A small shell for the in-process device, where iOS lets no other
//! program run: words with quotes, escapes, `~` and `$VAR`, commands
//! joined by `;`, `&&` and `||`, pipelines and redirections, the built-ins a shell
//! needs itself, and every other command a real program found on `$PATH`
//! (`native`).
use std::{
	collections::BTreeMap,
	fs,
	path::{Path, PathBuf},
	sync::Arc,
};

use super::{native, pipeline, terminal::Terminal};

/// The built-in commands, for `help`: what changes the shell itself, and
/// the few a shell answers on its own, as `sh` does.
pub(crate) const COMMANDS: [&str; 14] = [
	"cd", "clear", "echo", "exit", "export", "false", "help", "jit", "pkg", "pwd", "sshd", "true",
	"unset", "whoami",
];

/// Who programs run for: the app's own `$USER`, else the account
/// database's name for its user, else on iOS `mobile`, the one account
/// there (the simulator's database does not know the app's user).
pub(crate) fn user_name(
	process: Option<String>,
	account: Option<String>,
	ios: bool,
) -> Option<String> {
	let set = |name: &Option<String>| name.as_ref().is_some_and(|name| !name.is_empty());
	if set(&process) {
		process
	} else if set(&account) {
		account
	} else {
		ios.then(|| "mobile".to_owned())
	}
}

/// What the shell remembers between lines.
#[derive(Clone, Debug)]
pub(crate) struct Shell {
	pub cwd: PathBuf,
	pub home: PathBuf,
	/// Where the app keeps what the user does not see in their files:
	/// installed packages and their source list. `home` unless set (on iOS
	/// `home` is the Documents folder, which Files shows).
	pub support: PathBuf,
	pub env: BTreeMap<String, String>,
	/// The exit status of the last command, for `$?`.
	pub status: i32,
	/// The terminal's columns and rows, as programs are told.
	pub size: (u16, u16),
	/// The device's terminal: programs print to it as they run and read
	/// keys from it. Without one (tests), what they print is collected and
	/// their input is empty.
	pub terminal: Option<Arc<Terminal>>,
}

/// What running a line did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ran {
	/// What the commands printed, with `\n` line ends.
	pub output: Vec<u8>,
	/// What a program wrote to standard error while its output went to a
	/// file; shown after `output`.
	pub errors: Vec<u8>,
	/// `exit` was run.
	pub exit: bool,
}

/// Programs run one at a time, so none may start a second to run beside
/// it: no pager, and no git maintenance in the background (git reads its
/// `GIT_CONFIG_COUNT` pairs as configuration). Variables like any other,
/// which `export` replaces.
const ONE_PROGRAM_AT_A_TIME: [(&str, &str); 7] = [
	("PAGER", "cat"),
	("GIT_PAGER", "cat"),
	("GIT_CONFIG_COUNT", "2"),
	("GIT_CONFIG_KEY_0", "maintenance.auto"),
	("GIT_CONFIG_VALUE_0", "false"),
	("GIT_CONFIG_KEY_1", "gc.auto"),
	("GIT_CONFIG_VALUE_1", "0"),
];

impl Shell {
	pub(crate) fn new(home: PathBuf) -> Self {
		let mut env = BTreeMap::new();
		env.insert("HOME".to_owned(), home.display().to_string());
		env.insert("TERM".to_owned(), "xterm-256color".to_owned());
		env.insert("SHELL".to_owned(), "poopsh".to_owned());
		// What the desktop app's shells are told, and who they run for.
		env.insert("COLORTERM".to_owned(), "truecolor".to_owned());
		env.insert("TERM_PROGRAM".to_owned(), "poopterm".to_owned());
		let version = env!("CARGO_PKG_VERSION").to_owned();
		env.insert("TERM_PROGRAM_VERSION".to_owned(), version);
		for (key, value) in ONE_PROGRAM_AT_A_TIME {
			env.insert(key.to_owned(), value.to_owned());
		}
		let account = crate::sys::user::login(crate::sys::user::uid());
		let process = std::env::var("USER").ok();
		if let Some(user) = user_name(process, account, cfg!(target_os = "ios")) {
			env.insert("USER".to_owned(), user.clone());
			env.insert("LOGNAME".to_owned(), user);
		}
		Self {
			cwd: home.clone(),
			support: home.clone(),
			home,
			env,
			status: 0,
			size: (80, 24),
			terminal: None,
		}
	}

	/// The prompt: the directory, `~` for home.
	pub(crate) fn prompt(&self) -> String {
		let shown = match self.cwd.strip_prefix(&self.home) {
			Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
			Ok(rest) => format!("~/{}", rest.display()),
			Err(_) => self.cwd.display().to_string(),
		};
		format!("\x1b[1;34m{shown}\x1b[0m $ ")
	}

	/// Runs `line`: commands joined by `;` run in turn, by `&&` only
	/// while the previous one succeeded, by `||` only once it failed. A
	/// command skipped leaves the status as it was, so chains of both read
	/// left to right, as a POSIX shell's do.
	pub(crate) fn run(&mut self, line: &str) -> Ran {
		let mut ran = Ran::default();
		let Some(commands) = segments(line) else {
			ran.output.extend(b"poopsh: unfinished quote\n");
			self.status = 2;
			return ran;
		};
		let mut skip = false;
		for (command, op) in commands {
			if !skip {
				self.status = self.segment(&command, &mut ran);
				self.flush(&mut ran);
				if ran.exit {
					return ran;
				}
				if self.interrupted() {
					// ^C stops the rest of the line, as a shell's does.
					self.status = 130;
					return ran;
				}
			}
			skip = match op {
				"&&" => self.status != 0,
				"||" => self.status == 0,
				_ => false,
			};
		}
		ran
	}

	/// Shows what was collected so far on the terminal, if there is one,
	/// so it comes before what the next program prints there.
	fn flush(&self, ran: &mut Ran) {
		if let Some(terminal) = &self.terminal {
			let _ = terminal.write(&std::mem::take(&mut ran.output));
			let _ = terminal.write(&std::mem::take(&mut ran.errors));
		}
	}

	/// Whether ^C interrupted the program that ran last.
	fn interrupted(&self) -> bool {
		self
			.terminal
			.as_ref()
			.is_some_and(|terminal| terminal.take_interrupted())
	}

	/// Expands and runs one command or pipeline, now that those before it
	/// ran. Stages run in turn, each reading what the one before printed
	/// (or a `<` file) and printing to the next (or a `>` file); the last
	/// prints to the terminal. Its status is the last stage's.
	fn segment(&mut self, command: &str, ran: &mut Ran) -> i32 {
		let Some(words) = split(command, self) else {
			ran.output.extend(b"poopsh: unfinished quote\n");
			return 2;
		};
		let stages = match pipeline::stages(words) {
			Ok(stages) if stages.is_empty() => return self.status,
			Ok(stages) => stages,
			Err(message) => {
				ran
					.output
					.extend(format!("poopsh: {message}\n").into_bytes());
				return 2;
			}
		};
		let (last, mut piped, mut status) = (stages.len() - 1, None::<Vec<u8>>, 0);
		for (index, stage) in stages.into_iter().enumerate() {
			let failed = |ran: &mut Ran, file: &str, error: std::io::Error| {
				ran
					.output
					.extend(format!("poopsh: {file}: {error}\n").into_bytes());
			};
			let input = match &stage.input {
				Some(file) => match fs::read(self.path(file)) {
					Ok(bytes) => Some(bytes),
					Err(error) => {
						failed(ran, file, error);
						(piped, status) = (Some(Vec::new()), 1);
						continue;
					}
				},
				None => piped.take(),
			};
			// A file is opened before its command runs, as a shell opens
			// it first (`ls > list` lists `list`).
			let file = match &stage.output {
				Some((file, append)) => {
					let mut options = fs::OpenOptions::new();
					options
						.create(true)
						.write(true)
						.append(*append)
						.truncate(!append);
					match options.open(self.path(file)) {
						Ok(opened) => Some((opened, file)),
						Err(error) => {
							failed(ran, file, error);
							(piped, status) = (Some(Vec::new()), 1);
							continue;
						}
					}
				}
				None => None,
			};
			if stage.argv.is_empty() {
				continue;
			}
			let to_terminal = index == last && file.is_none();
			if to_terminal {
				self.flush(ran);
			}
			let mut captured = Ran::default();
			status = self.command(&stage.argv, input.as_deref(), &mut captured, to_terminal);
			ran.exit |= captured.exit;
			if to_terminal {
				ran.output.extend(captured.output);
				ran.output.extend(captured.errors);
			} else {
				ran.output.extend(captured.errors);
				match file {
					Some((mut opened, name)) => {
						if let Err(error) = std::io::Write::write_all(&mut opened, &captured.output) {
							failed(ran, name, error);
							status = 1;
						}
						piped = Some(Vec::new());
					}
					None => piped = Some(captured.output),
				}
			}
			if ran.exit {
				break;
			}
			if self
				.terminal
				.as_ref()
				.is_some_and(|terminal| terminal.interrupted())
			{
				status = 130;
				break;
			}
		}
		status
	}

	/// Runs one command and returns its exit status, with `input` as its
	/// standard input (none: empty); `tty` when its output goes to the
	/// terminal.
	fn command(&mut self, argv: &[String], input: Option<&[u8]>, ran: &mut Ran, tty: bool) -> i32 {
		// `NAME=value` words first set variables: for good when alone, else
		// only for the command after them.
		let assigned = argv
			.iter()
			.take_while(|word| assignment(word).is_some())
			.count();
		if assigned > 0 {
			let saved: Vec<(String, Option<String>)> = argv[..assigned]
				.iter()
				.filter_map(|word| assignment(word))
				.map(|(name, value)| {
					(
						name.to_owned(),
						self.env.insert(name.to_owned(), value.to_owned()),
					)
				})
				.collect();
			if assigned == argv.len() {
				return 0;
			}
			let status = self.command(&argv[assigned..], input, ran, tty);
			for (name, old) in saved.into_iter().rev() {
				match old {
					Some(value) => self.env.insert(name, value),
					None => self.env.remove(&name),
				};
			}
			return status;
		}
		let out = &mut ran.output;
		let (name, args) = (argv[0].as_str(), &argv[1..]);
		let fail = |out: &mut Vec<u8>, message: String| {
			out.extend(format!("{name}: {message}\n").into_bytes());
			1
		};
		match name {
			"true" => 0,
			"false" => 1,
			"exit" => {
				ran.exit = true;
				args
					.first()
					.and_then(|code| code.parse().ok())
					.unwrap_or(self.status)
			}
			"help" => {
				let list = COMMANDS.join(" ");
				let text =
					format!("Built-in commands: {list}\nEvery other command is a program on $PATH.\n");
				out.extend(text.into_bytes());
				0
			}
			"echo" => {
				let (newline, words) = match args.first().map(String::as_str) {
					Some("-n") => (false, &args[1..]),
					_ => (true, args),
				};
				out.extend(words.join(" ").into_bytes());
				if newline {
					out.push(b'\n');
				}
				0
			}
			// id needs the system's directory service, which iOS keeps from
			// apps; who programs run for is $USER (`user_name`).
			"whoami" => match self.env.get("USER") {
				Some(user) => {
					out.extend(format!("{user}\n").into_bytes());
					0
				}
				None => fail(out, "no user name".to_owned()),
			},
			"pwd" => {
				out.extend(format!("{}\n", self.cwd.display()).into_bytes());
				0
			}
			"cd" => {
				let target = args
					.first()
					.map_or_else(|| self.home.clone(), |dir| self.path(dir));
				match target.canonicalize() {
					Ok(dir) if dir.is_dir() => {
						self.cwd = dir;
						0
					}
					Ok(_) => fail(out, format!("{}: not a directory", target.display())),
					Err(error) => fail(out, format!("{}: {error}", target.display())),
				}
			}
			"clear" => {
				out.extend(b"\x1b[H\x1b[2J\x1b[3J");
				0
			}
			"export" => {
				for pair in args {
					if let Some((key, value)) = pair.split_once('=') {
						self.env.insert(key.to_owned(), value.to_owned());
					}
				}
				0
			}
			"jit" => {
				let terminal = self.terminal.clone();
				let stop = move || {
					terminal
						.as_ref()
						.is_some_and(|terminal| terminal.interrupted())
				};
				let screen = self.terminal.clone();
				let say = move |text: &str| {
					if let Some(terminal) = &screen {
						let _ = terminal.write(text.as_bytes());
					}
				};
				let (text, status) = jit(args, &stop, &say);
				out.extend(text.into_bytes());
				status
			}
			"pkg" => {
				let root = crate::pkg::root(&self.support);
				let source = self.env.get("POOPTERM_PKG_SOURCE").map(String::as_str);
				let fetch = |url: &str| crate::pkg::fetch::fetch(url, crate::pkg::DOWNLOAD_LIMIT);
				let (text, status) = crate::pkg::command::run(args, &root, source, &fetch);
				out.extend(text.into_bytes());
				status
			}
			"sshd" => {
				let (text, status) = super::sshd::command(args, &self.home, &self.support, &self.env);
				out.extend(text.into_bytes());
				status
			}
			"unset" => {
				args.iter().for_each(|key| drop(self.env.remove(key)));
				0
			}
			_ => self.native(argv, input, ran, tty),
		}
	}

	/// Runs a real program: the command library `argv[0]` names on `$PATH`.
	fn native(&mut self, argv: &[String], input: Option<&[u8]>, ran: &mut Ran, tty: bool) -> i32 {
		let name = argv[0].as_str();
		let Some(library) = native::find(name, self.env.get("PATH").map(String::as_str), &self.cwd)
		else {
			ran
				.output
				.extend(format!("poopsh: {name}: command not found\n").into_bytes());
			return 127;
		};
		let run = native::Run {
			argv,
			env: &self.env,
			cwd: &self.cwd,
			input,
			tty: tty.then_some(self.size),
			merge: tty,
			terminal: self.terminal.as_ref(),
		};
		match native::run(&library, &run) {
			Ok(done) => {
				ran.output.extend(done.output);
				ran.errors.extend(done.errors);
				done.status
			}
			Err(error) => {
				ran
					.errors
					.extend(format!("poopsh: {name}: {error}\n").into_bytes());
				126
			}
		}
	}

	/// `path` from the current directory, `~` for home.
	fn path(&self, path: &str) -> PathBuf {
		match path.strip_prefix('~') {
			Some(rest) => self.home.join(rest.trim_start_matches('/')),
			None => self.cwd.join(Path::new(path)),
		}
	}
}

/// The name and value of a `NAME=value` word.
fn assignment(word: &str) -> Option<(&str, &str)> {
	let (name, value) = word.split_once('=')?;
	let mut chars = name.chars();
	let first = chars.next()?;
	(first.is_alphabetic() || first == '_')
		.then_some(())
		.filter(|()| chars.all(|ch| ch.is_alphanumeric() || ch == '_'))?;
	Some((name, value))
}

/// A word of a command, or a redirection (`>`, `>>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Word {
	Text(String),
	Op(&'static str),
}

/// The commands of `line` and the operator after each (`;`, `&&` or `||`; `;`
/// after the last), split outside quotes, not yet expanded. `None` for an
/// `jit`: where code iOS did not sign can run.
#[cfg(target_vendor = "apple")]
fn jit(args: &[String], stop: &dyn Fn() -> bool, say: &dyn Fn(&str)) -> (String, i32) {
	crate::jit::command_with(args, stop, say)
}

#[cfg(not(target_vendor = "apple"))]
fn jit(_args: &[String], _stop: &dyn Fn() -> bool, _say: &dyn Fn(&str)) -> (String, i32) {
	(
		"jit: not needed here; this system runs any program\n".to_owned(),
		0,
	)
}

/// unfinished quote.
pub(crate) fn segments(line: &str) -> Option<Vec<(String, &'static str)>> {
	let mut out = Vec::new();
	let (mut current, mut quote, mut escaped) = (String::new(), None::<char>, false);
	let mut chars = line.chars().peekable();
	while let Some(ch) = chars.next() {
		match (quote, ch) {
			_ if escaped => {
				escaped = false;
				current.push(ch);
				continue;
			}
			(Some('\''), '\'') | (Some('"'), '"') => quote = None,
			(None, '\'' | '"') => quote = Some(ch),
			(q, '\\') if q != Some('\'') => escaped = true,
			(None, ';') => {
				out.push((std::mem::take(&mut current), ";"));
				continue;
			}
			(None, '&') if chars.peek() == Some(&'&') => {
				chars.next();
				out.push((std::mem::take(&mut current), "&&"));
				continue;
			}
			(None, '|') if chars.peek() == Some(&'|') => {
				chars.next();
				out.push((std::mem::take(&mut current), "||"));
				continue;
			}
			_ => {}
		}
		current.push(ch);
	}
	if quote.is_some() || escaped {
		return None;
	}
	out.push((current, ";"));
	Some(out)
}

/// Splits one command into words and redirections: quotes group, `\`
/// escapes, `$NAME` and `$?` expand (not inside single quotes), a leading
/// `~` is home, and a word that was only an unquoted, empty expansion
/// disappears. `None` for an unfinished quote.
pub(crate) fn split(line: &str, shell: &Shell) -> Option<Vec<Word>> {
	let mut words = Vec::new();
	let mut word: Option<Partial> = None;
	let finish = |word: &mut Option<Partial>, words: &mut Vec<Word>| {
		let Some(partial) = word.take() else {
			return;
		};
		if partial.glob {
			let found = super::glob::expand(&partial.pattern, &shell.cwd);
			if !found.is_empty() {
				words.extend(found.into_iter().map(Word::Text));
				return;
			}
		}
		if partial.kept || !partial.text.is_empty() {
			words.push(Word::Text(partial.text));
		}
	};
	let mut chars = line.chars().peekable();
	let mut quote: Option<char> = None;
	while let Some(ch) = chars.next() {
		match (quote, ch) {
			(Some(q), _) if ch == q => quote = None,
			(None, '\'' | '"') => {
				quote = Some(ch);
				word.get_or_insert_default().kept = true;
			}
			(Some('\''), _) => word.get_or_insert_default().push(ch, true),
			(_, '\\') => {
				let next = chars.next()?;
				let partial = word.get_or_insert_default();
				partial.push(next, true);
				partial.kept = true;
			}
			(_, '$') => {
				let mut name = String::new();
				if chars.peek() == Some(&'?') {
					chars.next();
					name.push('?');
				} else {
					while let Some(&c) = chars.peek().filter(|c| c.is_alphanumeric() || **c == '_') {
						name.push(c);
						chars.next();
					}
				}
				let value = match name.as_str() {
					"" => "$".to_owned(),
					"?" => shell.status.to_string(),
					_ => shell.env.get(&name).cloned().unwrap_or_default(),
				};
				let partial = word.get_or_insert_default();
				value.chars().for_each(|c| partial.push(c, true));
				partial.kept |= quote.is_some() || name.is_empty();
			}
			(None, ' ' | '\t') => finish(&mut word, &mut words),
			(None, '|') => {
				finish(&mut word, &mut words);
				words.push(Word::Op("|"));
			}
			(None, '<') => {
				finish(&mut word, &mut words);
				words.push(Word::Op("<"));
			}
			(None, '>') => {
				finish(&mut word, &mut words);
				let append = chars.peek() == Some(&'>');
				if append {
					chars.next();
				}
				words.push(Word::Op(if append { ">>" } else { ">" }));
			}
			(None, '~') if word.is_none() && matches!(chars.peek(), None | Some('/' | ' ')) => {
				let mut partial = Partial::default();
				shell
					.home
					.display()
					.to_string()
					.chars()
					.for_each(|c| partial.push(c, true));
				partial.kept = true;
				word = Some(partial);
			}
			_ => {
				let partial = word.get_or_insert_default();
				partial.push(ch, quote.is_some());
				partial.kept = true;
			}
		}
	}
	if quote.is_some() {
		return None;
	}
	finish(&mut word, &mut words);
	Some(words)
}

/// A word being read: its text, whether it stays even if empty (quoted),
/// and the same as a filename pattern, where quoted and expanded
/// characters are escaped, with whether any pattern character is left.
#[derive(Default)]
struct Partial {
	text: String,
	kept: bool,
	pattern: String,
	glob: bool,
}

impl Partial {
	/// Adds `ch`; a `literal` one never acts as a pattern character.
	fn push(&mut self, ch: char, literal: bool) {
		self.text.push(ch);
		let special = matches!(ch, '*' | '?' | '[');
		if literal && (special || ch == '\\') {
			self.pattern.push('\\');
		}
		self.pattern.push(ch);
		self.glob |= !literal && special;
	}
}
