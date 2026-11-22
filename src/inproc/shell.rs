//! A small shell for the in-process device, where iOS lets no other
//! program run: words with quotes, escapes, `~` and `$VAR`, commands
//! joined by `;` and `&&`, and built-in commands over the app's own files.
use std::{
	collections::BTreeMap,
	fmt::Write as _,
	fs,
	path::{Path, PathBuf},
};

/// Most bytes a command may print, so `cat` of a huge file stays bounded.
pub(crate) const MAX_OUTPUT: usize = 16 << 20;

/// The built-in commands, for `help`.
pub(crate) const COMMANDS: [&str; 22] = [
	"cat", "cd", "clear", "cp", "date", "echo", "env", "exit", "export", "false", "help", "ls",
	"mkdir", "mv", "pwd", "rm", "rmdir", "seq", "touch", "true", "uname", "unset",
];

/// What the shell remembers between lines.
#[derive(Clone, Debug)]
pub(crate) struct Shell {
	pub cwd: PathBuf,
	pub home: PathBuf,
	pub env: BTreeMap<String, String>,
	/// The exit status of the last command, for `$?`.
	pub status: i32,
}

/// What running a line did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ran {
	/// What the commands printed, with `\n` line ends.
	pub output: Vec<u8>,
	/// `exit` was run.
	pub exit: bool,
}

impl Shell {
	pub(crate) fn new(home: PathBuf) -> Self {
		let mut env = BTreeMap::new();
		env.insert("HOME".to_owned(), home.display().to_string());
		env.insert("TERM".to_owned(), "xterm-256color".to_owned());
		env.insert("SHELL".to_owned(), "poopsh".to_owned());
		Self {
			cwd: home.clone(),
			home,
			env,
			status: 0,
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
	/// while the previous one succeeded.
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
				if ran.exit {
					return ran;
				}
			}
			skip = op == "&&" && self.status != 0;
		}
		ran
	}

	/// Expands and runs one command, now that those before it ran, with
	/// its output sent to a file for `>` (or `>>`, to its end).
	fn segment(&mut self, command: &str, ran: &mut Ran) -> i32 {
		let Some(words) = split(command, self) else {
			ran.output.extend(b"poopsh: unfinished quote\n");
			return 2;
		};
		let (mut argv, mut target) = (Vec::new(), None);
		let mut words = words.into_iter();
		while let Some(word) = words.next() {
			match word {
				Word::Text(text) => argv.push(text),
				Word::Op(op) => match words.next() {
					Some(Word::Text(file)) => target = Some((self.path(&file), op == ">>")),
					_ => {
						ran.output.extend(b"poopsh: a file must follow > or >>\n");
						return 2;
					}
				},
			}
		}
		if argv.is_empty() {
			return self.status;
		}
		let Some((file, append)) = target else {
			return self.command(&argv, ran);
		};
		let mut captured = Ran::default();
		let status = self.command(&argv, &mut captured);
		ran.exit = captured.exit;
		let mut options = fs::OpenOptions::new();
		options
			.create(true)
			.write(true)
			.append(append)
			.truncate(!append);
		let written = options
			.open(&file)
			.and_then(|mut f| std::io::Write::write_all(&mut f, &captured.output));
		match written {
			Ok(()) => status,
			Err(error) => {
				ran
					.output
					.extend(format!("poopsh: {}: {error}\n", file.display()).into_bytes());
				1
			}
		}
	}

	/// Runs one built-in command and returns its exit status.
	fn command(&mut self, argv: &[String], ran: &mut Ran) -> i32 {
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
				out.extend(format!("Built-in commands: {}\n", COMMANDS.join(" ")).into_bytes());
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
			"env" => {
				for (key, value) in &self.env {
					out.extend(format!("{key}={value}\n").into_bytes());
				}
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
			"unset" => {
				args.iter().for_each(|key| drop(self.env.remove(key)));
				0
			}
			"uname" => {
				let all = args.iter().any(|arg| arg == "-a");
				out.extend(
					if all {
						"Poopterm iOS arm64\n"
					} else {
						"Poopterm\n"
					}
					.as_bytes(),
				);
				0
			}
			"date" => {
				let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
				let [y, mo, d, h, mi, se] = crate::ui::export::utc(now.map_or(0, |since| since.as_secs()));
				out.extend(format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{se:02} UTC\n").into_bytes());
				0
			}
			"seq" => {
				let numbers: Vec<i64> = args.iter().filter_map(|arg| arg.parse().ok()).collect();
				let (first, last) = match numbers[..] {
					[last] => (1, last),
					[first, last] => (first, last),
					_ => return fail(out, "usage: seq [first] last".to_owned()),
				};
				for number in first..=last {
					if out.len() > MAX_OUTPUT {
						break;
					}
					let _ = writeln!(Printed(out), "{number}");
				}
				0
			}
			"ls" => self.ls(args, out),
			"cat" => {
				let mut status = 0;
				for file in args {
					match fs::read(self.path(file)) {
						Ok(bytes) => {
							out.extend(&bytes[..bytes.len().min(MAX_OUTPUT.saturating_sub(out.len()))])
						}
						Err(error) => status = fail(out, format!("{file}: {error}")),
					}
				}
				status
			}
			"mkdir" | "rmdir" | "touch" | "rm" => {
				let (parents, files): (Vec<&String>, Vec<&String>) =
					args.iter().partition(|arg| arg.starts_with('-'));
				let recursive = parents
					.iter()
					.any(|flag| flag.contains('p') || flag.contains('r'));
				let mut status = 0;
				for file in files {
					let path = self.path(file);
					let result = match name {
						"mkdir" if recursive => fs::create_dir_all(&path),
						"mkdir" => fs::create_dir(&path),
						"rmdir" => fs::remove_dir(&path),
						"touch" => fs::OpenOptions::new()
							.create(true)
							.append(true)
							.open(&path)
							.map(drop),
						_ if recursive && path.is_dir() => fs::remove_dir_all(&path),
						_ => fs::remove_file(&path),
					};
					if let Err(error) = result {
						status = fail(out, format!("{file}: {error}"));
					}
				}
				status
			}
			"cp" | "mv" => match args {
				[from, to] => {
					let (from_path, mut to_path) = (self.path(from), self.path(to));
					if to_path.is_dir() {
						to_path.push(from_path.file_name().unwrap_or_default());
					}
					let result = match name {
						"cp" => fs::copy(&from_path, &to_path).map(drop),
						_ => fs::rename(&from_path, &to_path),
					};
					result.map_or_else(|error| fail(out, format!("{from}: {error}")), |()| 0)
				}
				_ => fail(out, format!("usage: {name} from to")),
			},
			_ => {
				out.extend(format!("poopsh: {name}: command not found\n").into_bytes());
				127
			}
		}
	}

	/// `ls`: the entries of each directory given (the current one by
	/// default), sorted, directories marked with `/`; `-a` shows hidden ones.
	fn ls(&self, args: &[String], out: &mut Vec<u8>) -> i32 {
		let all = args
			.iter()
			.any(|arg| arg.starts_with('-') && arg.contains('a'));
		let dirs: Vec<&String> = args.iter().filter(|arg| !arg.starts_with('-')).collect();
		let dirs = if dirs.is_empty() {
			vec![None]
		} else {
			dirs.into_iter().map(Some).collect()
		};
		let mut status = 0;
		for dir in &dirs {
			let path = dir.map_or_else(|| self.cwd.clone(), |dir| self.path(dir));
			let entries = match fs::read_dir(&path) {
				Ok(entries) => entries,
				Err(_) if path.is_file() => {
					out.extend(format!("{}\n", dir.map_or("", String::as_str)).into_bytes());
					continue;
				}
				Err(error) => {
					out.extend(format!("ls: {}: {error}\n", path.display()).into_bytes());
					status = 1;
					continue;
				}
			};
			let mut names: Vec<String> = entries
				.flatten()
				.map(|entry| {
					let name = entry.file_name().to_string_lossy().into_owned();
					if entry.path().is_dir() {
						format!("{name}/")
					} else {
						name
					}
				})
				.filter(|name| all || !name.starts_with('.'))
				.collect();
			names.sort();
			if dirs.len() > 1 {
				out.extend(format!("{}:\n", dir.map_or("", String::as_str)).into_bytes());
			}
			for name in names {
				out.extend(format!("{name}\n").into_bytes());
			}
		}
		status
	}

	/// `path` from the current directory, `~` for home.
	fn path(&self, path: &str) -> PathBuf {
		match path.strip_prefix('~') {
			Some(rest) => self.home.join(rest.trim_start_matches('/')),
			None => self.cwd.join(Path::new(path)),
		}
	}
}

/// Formats into a byte buffer.
struct Printed<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Printed<'_> {
	fn write_str(&mut self, text: &str) -> std::fmt::Result {
		self.0.extend_from_slice(text.as_bytes());
		Ok(())
	}
}

/// A word of a command, or a redirection (`>`, `>>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Word {
	Text(String),
	Op(&'static str),
}

/// The commands of `line` and the operator after each (`;` or `&&`; `;`
/// after the last), split outside quotes, not yet expanded. `None` for an
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
	// The word being read, and whether it stays even if empty (quoted).
	let mut word: Option<(String, bool)> = None;
	let finish = |word: &mut Option<(String, bool)>, words: &mut Vec<Word>| {
		if let Some((text, kept)) = word.take()
			&& (kept || !text.is_empty())
		{
			words.push(Word::Text(text));
		}
	};
	let mut chars = line.chars().peekable();
	let mut quote: Option<char> = None;
	while let Some(ch) = chars.next() {
		match (quote, ch) {
			(Some(q), _) if ch == q => quote = None,
			(None, '\'' | '"') => {
				quote = Some(ch);
				word.get_or_insert_default().1 = true;
			}
			(Some('\''), _) => word.get_or_insert_default().0.push(ch),
			(_, '\\') => {
				let next = chars.next()?;
				let (text, kept) = word.get_or_insert_default();
				text.push(next);
				*kept = true;
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
				let (text, kept) = word.get_or_insert_default();
				text.push_str(&value);
				*kept |= quote.is_some() || name.is_empty();
			}
			(None, ' ' | '\t') => finish(&mut word, &mut words),
			(None, '>') => {
				finish(&mut word, &mut words);
				let append = chars.peek() == Some(&'>');
				if append {
					chars.next();
				}
				words.push(Word::Op(if append { ">>" } else { ">" }));
			}
			(None, '~') if word.is_none() && matches!(chars.peek(), None | Some('/' | ' ')) => {
				word = Some((shell.home.display().to_string(), true));
			}
			_ => {
				let (text, kept) = word.get_or_insert_default();
				text.push(ch);
				*kept = true;
			}
		}
	}
	if quote.is_some() {
		return None;
	}
	finish(&mut word, &mut words);
	Some(words)
}
