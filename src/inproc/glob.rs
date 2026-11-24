//! Filename patterns (`*`, `?`, `[a-z]`, `[!x]`, `\x`) for the in-process
//! shell: matching one name, and expanding a pattern against a directory.
use std::path::Path;

/// Whether `pattern` has a character a name could be matched by.
pub(crate) fn is_pattern(pattern: &str) -> bool {
	let mut chars = pattern.chars();
	while let Some(ch) = chars.next() {
		match ch {
			'*' | '?' | '[' => return true,
			'\\' => {
				chars.next();
			}
			_ => {}
		}
	}
	false
}

/// `pattern` without its escapes: the name it spells when it has no
/// pattern characters.
pub(crate) fn unescape(pattern: &str) -> String {
	let mut out = String::new();
	let mut chars = pattern.chars();
	while let Some(ch) = chars.next() {
		out.push(if ch == '\\' {
			chars.next().unwrap_or('\\')
		} else {
			ch
		});
	}
	out
}

/// Whether `name` matches `pattern`.
pub(crate) fn matches(pattern: &str, name: &str) -> bool {
	let pattern: Vec<char> = pattern.chars().collect();
	let name: Vec<char> = name.chars().collect();
	matches_from(&pattern, &name)
}

fn matches_from(pattern: &[char], name: &[char]) -> bool {
	let Some((&first, rest)) = pattern.split_first() else {
		return name.is_empty();
	};
	match first {
		// Any run of characters: try every length, shortest first.
		'*' => (0..=name.len()).any(|skip| matches_from(rest, &name[skip..])),
		'?' => !name.is_empty() && matches_from(rest, &name[1..]),
		'[' => {
			let Some((set, after)) = bracket(rest) else {
				// No closing bracket: a plain `[`.
				return name.first() == Some(&'[') && matches_from(rest, &name[1..]);
			};
			name.first().is_some_and(|&ch| set.contains(ch)) && matches_from(after, &name[1..])
		}
		'\\' => match rest.split_first() {
			Some((&literal, rest)) => name.first() == Some(&literal) && matches_from(rest, &name[1..]),
			None => name == ['\\'],
		},
		literal => name.first() == Some(&literal) && matches_from(rest, &name[1..]),
	}
}

/// A bracket expression's characters.
struct Set {
	negated: bool,
	ranges: Vec<(char, char)>,
}

impl Set {
	fn contains(&self, ch: char) -> bool {
		self
			.ranges
			.iter()
			.any(|&(low, high)| low <= ch && ch <= high)
			!= self.negated
	}
}

/// The set after a `[` and what follows its `]`, or None without one.
fn bracket(rest: &[char]) -> Option<(Set, &[char])> {
	let negated = matches!(rest.first(), Some('!' | '^'));
	let start = usize::from(negated);
	let mut ranges = Vec::new();
	let mut at = start;
	while let Some(&ch) = rest.get(at) {
		// A `]` first in the set is a member.
		if ch == ']' && at > start {
			return Some((Set { negated, ranges }, &rest[at + 1..]));
		}
		let low = if ch == '\\' {
			at += 1;
			*rest.get(at)?
		} else {
			ch
		};
		if rest.get(at + 1) == Some(&'-') && rest.get(at + 2).is_some_and(|&end| end != ']') {
			ranges.push((low, rest[at + 2]));
			at += 3;
		} else {
			ranges.push((low, low));
			at += 1;
		}
	}
	None
}

/// The paths matching `pattern`, relative to `cwd` unless it is absolute,
/// sorted, written as the pattern writes them. Hidden names match only
/// a segment that starts with a dot. Empty when nothing matches.
pub(crate) fn expand(pattern: &str, cwd: &Path) -> Vec<String> {
	let absolute = pattern.starts_with('/');
	let segments: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();
	let mut found = vec![String::new()];
	for (index, segment) in segments.iter().enumerate() {
		let last = index + 1 == segments.len();
		let mut next = Vec::new();
		for base in &found {
			let join = |name: &str| {
				if base.is_empty() && !absolute {
					name.to_owned()
				} else {
					format!("{base}/{name}")
				}
			};
			if !is_pattern(segment) {
				let path = join(&unescape(segment));
				if last && std::fs::symlink_metadata(resolve(&path, cwd, absolute)).is_err() {
					continue;
				}
				next.push(path);
				continue;
			}
			let Ok(entries) = std::fs::read_dir(resolve(base, cwd, absolute)) else {
				continue;
			};
			let mut names: Vec<String> = entries
				.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
				.filter(|name| {
					(!name.starts_with('.') || segment.starts_with('.')) && matches(segment, name)
				})
				.collect();
			names.sort();
			next.extend(names.iter().map(|name| join(name)));
		}
		found = next;
	}
	found
}

/// `path` as a path from `cwd`; empty is the directory itself, or the root
/// for an absolute pattern's first step.
fn resolve(path: &str, cwd: &Path, absolute: bool) -> std::path::PathBuf {
	if path.is_empty() {
		if absolute { "/".into() } else { cwd.to_owned() }
	} else {
		cwd.join(path)
	}
}

#[cfg(test)]
#[path = "glob_tests.rs"]
mod tests;
