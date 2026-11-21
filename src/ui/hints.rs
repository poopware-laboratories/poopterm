//! Quick select: the URLs, paths, commit hashes and IPv4 addresses on the
//! screen, each labelled with letters to type, nearest the bottom first.
use crate::term::Frame;

/// Label letters, home row first.
const ALPHABET: &[u8; 26] = b"asdfghjklqwertyuiopzxcvbnm";
/// The most hints shown: every two-letter label.
pub(crate) const MAX_HINTS: usize = 26 * 26;

/// Something on the screen worth selecting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
	/// Screen row, from the top.
	pub row: usize,
	/// First and last column it covers.
	pub cols: (usize, usize),
	pub text: String,
}

/// A target with the letters that pick it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hint {
	pub label: String,
	pub target: Target,
}

/// Each visible row of `frame` as its characters with their columns,
/// wide characters once.
pub(crate) fn rows_of(frame: &Frame) -> Vec<Vec<(char, usize)>> {
	let cells = |row| {
		frame
			.row(row)
			.iter()
			.enumerate()
			.filter(|(_, cell)| cell.width != 0)
	};
	(0..frame.rows)
		.map(|row| cells(row).map(|(col, cell)| (cell.ch, col)).collect())
		.collect()
}

/// Characters that end a word: spaces, quotes, angle brackets and pipes.
fn separates(ch: char) -> bool {
	ch.is_whitespace() || ch.is_control() || "\"'`<>|│".contains(ch)
}

/// The targets in `rows`, top to bottom and left to right.
pub(crate) fn find_targets(rows: &[Vec<(char, usize)>]) -> Vec<Target> {
	let mut targets = Vec::new();
	for (row, chars) in rows.iter().enumerate() {
		let mut start = 0;
		while start < chars.len() {
			let end = (start..chars.len())
				.find(|&i| separates(chars[i].0))
				.unwrap_or(chars.len());
			let word = &chars[start..end];
			let word = &word[scheme_start(word)..];
			if let Some((first, last)) = trimmed(word) {
				let word = &word[first..=last];
				let text: String = word.iter().map(|&(ch, _)| ch).collect();
				if is_target(&text) {
					let cols = (word[0].1, word[word.len() - 1].1);
					targets.push(Target { row, cols, text });
				}
			}
			start = end + 1;
		}
	}
	targets
}

/// Where a URL inside `word` begins, as in `[docs](https://…)` or
/// `url=https://…`: at the scheme before its `://`; 0 for other words.
fn scheme_start(word: &[(char, usize)]) -> usize {
	let text: Vec<char> = word.iter().map(|&(ch, _)| ch).collect();
	let Some(at) = text.windows(3).position(|three| three == [':', '/', '/']) else {
		return 0;
	};
	let scheme = |ch: &char| ch.is_ascii_alphanumeric() || "+.-".contains(*ch);
	let start = text[..at].iter().rposition(|ch| !scheme(ch)).unwrap_or(0);
	let letter = text[start..at].iter().position(char::is_ascii_alphabetic);
	letter.map_or(0, |i| start + i)
}

/// Brackets that may close inside a target, with what opens them.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// The word without the punctuation around it: opening brackets before,
/// closing ones and sentence marks after, keeping a `)` that closes a `(`
/// inside, as in a link to a page about "Rust (language)".
fn trimmed(word: &[(char, usize)]) -> Option<(usize, usize)> {
	let text: Vec<char> = word.iter().map(|&(ch, _)| ch).collect();
	let first = text.iter().position(|ch| !"([{".contains(*ch))?;
	let count = |ch: char, end: usize| text[first..end].iter().filter(|&&c| c == ch).count();
	let mut last = text.len() - 1;
	while last > first {
		let strip = match PAIRS.iter().find(|&&(_, close)| close == text[last]) {
			Some(&(open, close)) => count(open, last) <= count(close, last),
			None => ".,;:!?".contains(text[last]),
		};
		if !strip {
			break;
		}
		last -= 1;
	}
	Some((first, last))
}

fn is_target(text: &str) -> bool {
	is_url(text) || is_path(text) || is_hash(text) || is_ipv4(text)
}

fn is_url(text: &str) -> bool {
	let Some((scheme, rest)) = text.split_once("://") else {
		return false;
	};
	let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
		&& scheme
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
	scheme_ok && !rest.is_empty()
}

/// A path: a slash and a letter, not only slashes, a `//` comment, or a
/// URL that is not one.
fn is_path(text: &str) -> bool {
	text.contains('/')
		&& !text.starts_with("//")
		&& !text.contains("://")
		&& text.chars().any(|c| c.is_alphabetic())
		&& text.chars().count() >= 3
}

/// A commit hash: 7 to 40 lower-case hex digits, with a digit and a letter.
fn is_hash(text: &str) -> bool {
	let hex = text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
	let (digit, letter) = (
		text.bytes().any(|b| b.is_ascii_digit()),
		text.bytes().any(|b| b > b'9'),
	);
	(7..=40).contains(&text.len()) && hex && digit && letter
}

/// Four numbers up to 255 between dots, with an optional port.
fn is_ipv4(text: &str) -> bool {
	let (address, port) = text.split_once(':').unwrap_or((text, "0"));
	let fits = |part: &str, digits: usize, most: u32| {
		let digits_only = part.bytes().all(|b| b.is_ascii_digit());
		(1..=digits).contains(&part.len())
			&& digits_only
			&& part.parse::<u32>().is_ok_and(|n| n <= most)
	};
	let parts = address.split('.');
	parts.clone().count() == 4
		&& parts.into_iter().all(|part| fits(part, 3, 255))
		&& fits(port, 5, 65535)
}

/// Labels for `targets`, the same text sharing one, those nearest the
/// bottom (where the cursor usually is) labelled first: single letters
/// while they suffice, two letters each beyond that.
pub(crate) fn label(targets: Vec<Target>) -> Vec<Hint> {
	let mut texts: Vec<&str> = Vec::new();
	for target in targets.iter().rev() {
		if !texts.contains(&target.text.as_str()) {
			texts.push(&target.text);
		}
	}
	texts.truncate(MAX_HINTS);
	let letters: u32 = if texts.len() > ALPHABET.len() { 2 } else { 1 };
	let name = |i: usize| -> String {
		(0..letters)
			.rev()
			.map(|place| char::from(ALPHABET[i / 26usize.pow(place) % 26]))
			.collect()
	};
	let index = |target: &Target| texts.iter().position(|&text| text == target.text);
	let labels: Vec<Option<String>> = targets
		.iter()
		.map(|target| index(target).map(name))
		.collect();
	(targets.into_iter().zip(labels))
		.filter_map(|(target, label)| {
			Some(Hint {
				label: label?,
				target,
			})
		})
		.collect()
}

/// What typing `typed` picks among `hints`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick<'a> {
	/// A whole label: its target's text.
	Chosen(&'a str),
	/// The start of some labels; those hints stay.
	Narrowed(Vec<&'a Hint>),
	/// No label starts so.
	Nothing,
}

pub(crate) fn pick<'a>(hints: &'a [Hint], typed: &str) -> Pick<'a> {
	let typed = typed.to_ascii_lowercase();
	if let Some(hint) = hints.iter().find(|hint| hint.label == typed) {
		return Pick::Chosen(&hint.target.text);
	}
	let left: Vec<&Hint> = hints
		.iter()
		.filter(|hint| hint.label.starts_with(&typed))
		.collect();
	if left.is_empty() {
		Pick::Nothing
	} else {
		Pick::Narrowed(left)
	}
}
