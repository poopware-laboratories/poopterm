//! Deterministic benchmark workloads: what programs actually send a terminal.
//!
//! Every workload is generated from a fixed seed, so the parser benchmark,
//! the headless `--benchmark` run (also the PGO training run) and the
//! end-to-end script (which reads `--bench-corpus` output) see the same bytes.
use super::images;

/// A small, fast, deterministic generator (xorshift64*).
pub(crate) struct Rng(u64);

impl Rng {
	pub(crate) fn new(seed: u64) -> Self {
		Self(seed | 1)
	}
	pub(crate) fn next(&mut self) -> u64 {
		self.0 ^= self.0 >> 12;
		self.0 ^= self.0 << 25;
		self.0 ^= self.0 >> 27;
		self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
	}
	/// A value in `0..n`.
	pub(crate) fn below(&mut self, n: usize) -> usize {
		(self.next() >> 33) as usize % n.max(1)
	}
	pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
		&items[self.below(items.len())]
	}
}

/// Grid size the workload targets (full-screen programs draw to it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Size {
	pub cols: usize,
	pub rows: usize,
}

impl Size {
	/// `cols`×`rows` kept within what a grid can be: 2 to 1000 each way.
	pub(crate) fn bounded(cols: usize, rows: usize) -> Self {
		let (cols, rows) = (cols.clamp(2, 1000), rows.clamp(2, 1000));
		Self { cols, rows }
	}
}

pub(crate) struct Workload {
	pub name: &'static str,
	pub about: &'static str,
	/// Appends one burst of output (a few KiB) to `out`.
	generate: fn(&mut Rng, Size, &mut Vec<u8>),
}

impl Workload {
	/// At least `bytes` of output, ending on a whole burst so no escape
	/// sequence or UTF-8 character is cut.
	pub(crate) fn build(&self, bytes: usize, size: Size) -> Vec<u8> {
		let mut rng = Rng::new(0x9e37_79b9_7f4a_7c15 ^ self.name.len() as u64);
		let mut out = Vec::with_capacity(bytes + (64 << 10));
		while out.len() < bytes {
			(self.generate)(&mut rng, size, &mut out);
		}
		out
	}
}

pub(crate) const WORKLOADS: &[Workload] = &[
	Workload {
		name: "log",
		about: "plain ASCII build log",
		generate: log,
	},
	Workload {
		name: "compiler",
		about: "colored diagnostics with hyperlinks",
		generate: compiler,
	},
	Workload {
		name: "ls",
		about: "ls --color listing",
		generate: ls,
	},
	Workload {
		name: "source",
		about: "syntax-highlighted source (bat)",
		generate: source,
	},
	Workload {
		name: "unicode",
		about: "accented, Greek and Cyrillic prose",
		generate: unicode,
	},
	Workload {
		name: "cjk",
		about: "wide CJK text",
		generate: cjk,
	},
	Workload {
		name: "emoji",
		about: "emoji, ZWJ sequences, combining marks",
		generate: emoji,
	},
	Workload {
		name: "tui",
		about: "full-screen redraw in sync blocks (htop)",
		generate: tui,
	},
	Workload {
		name: "editor",
		about: "scroll regions, insert/delete lines (vim)",
		generate: editor,
	},
	Workload {
		name: "progress",
		about: "carriage-return progress bars",
		generate: progress,
	},
	Workload {
		name: "truecolor",
		about: "24-bit half-block art (lolcat, chafa)",
		generate: truecolor,
	},
	Workload {
		name: "cursor",
		about: "random cursor motion and erases",
		generate: cursor,
	},
	Workload {
		name: "shell",
		about: "prompts with OSC 133/7/0/8",
		generate: shell,
	},
	Workload {
		name: "sixel",
		about: "Sixel images",
		generate: images::sixel,
	},
	Workload {
		name: "kitty-rgba",
		about: "kitty graphics, raw RGBA",
		generate: images::kitty_rgba,
	},
	Workload {
		name: "kitty-png",
		about: "kitty graphics, PNG",
		generate: images::kitty_png,
	},
	Workload {
		name: "kitty-zlib",
		about: "kitty graphics, zlib RGB",
		generate: images::kitty_zlib,
	},
];

pub(crate) fn find(name: &str) -> Option<&'static Workload> {
	WORKLOADS.iter().find(|workload| workload.name == name)
}

const WORDS: &[&str] = &[
	"error",
	"warning",
	"note",
	"compiling",
	"finished",
	"release",
	"target",
	"debug",
	"src",
	"main",
	"parser",
	"render",
	"value",
	"expected",
	"found",
	"struct",
	"function",
	"module",
	"borrow",
	"lifetime",
	"trait",
	"impl",
	"unused",
	"variable",
	"test",
	"passed",
	"ok",
];

fn words(rng: &mut Rng, count: usize, out: &mut Vec<u8>) {
	for i in 0..count {
		if i > 0 {
			out.push(b' ');
		}
		out.extend_from_slice(rng.pick(WORDS).as_bytes());
	}
}

fn log(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	for _ in 0..32 {
		let seconds = rng.below(100_000);
		out.extend_from_slice(format!("[{:>5}.{:03}] ", seconds / 1000, seconds % 1000).as_bytes());
		let count = 4 + rng.below(14);
		words(rng, count, out);
		out.extend_from_slice(b"\r\n");
	}
}

fn compiler(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	for _ in 0..8 {
		let (line, col) = (1 + rng.below(900), 1 + rng.below(80));
		let file = format!("src/{}/{}.rs", rng.pick(WORDS), rng.pick(WORDS));
		out.extend_from_slice(
			b"\x1b[1m\x1b[38;5;9merror[E0308]\x1b[0m\x1b[1m: mismatched types\x1b[0m\r\n",
		);
		out.extend_from_slice(
			format!(
				"  \x1b[1m\x1b[38;5;12m-->\x1b[0m \x1b]8;;file:///home/me/{file}\x1b\\{file}:{line}:{col}\x1b]8;;\x1b\\\r\n"
			)
			.as_bytes(),
		);
		out
			.extend_from_slice(format!("\x1b[1m\x1b[38;5;12m{line:>4} |\x1b[0m     let x = ").as_bytes());
		words(rng, 5, out);
		out.extend_from_slice(b";\r\n     \x1b[1m\x1b[38;5;12m|\x1b[0m             \x1b[1m\x1b[38;5;9m^^^^^\x1b[0m \x1b[1m\x1b[38;5;9mexpected `u32`\x1b[0m\r\n\r\n");
	}
}

fn ls(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	const KINDS: &[(&str, &str)] = &[
		("01;34", "/"),
		("01;32", "*"),
		("0", ""),
		("01;36", "@"),
		("01;31", ""),
		("0", ""),
	];
	for _ in 0..48 {
		let (color, suffix) = rng.pick(KINDS);
		let size = rng.below(1 << 24);
		out.extend_from_slice(
			format!(
				"-rw-r--r--. 1 me me {size:>9} Oct {:>2} 12:{:02} \x1b[{color}m",
				1 + rng.below(30),
				rng.below(60)
			)
			.as_bytes(),
		);
		out.extend_from_slice(rng.pick(WORDS).as_bytes());
		out.extend_from_slice(format!("_{}\x1b[0m{suffix}\r\n", rng.below(999)).as_bytes());
	}
}

fn source(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	const TOKENS: &[&str] = &[
		"\x1b[38;5;204mfn\x1b[0m",
		"\x1b[38;5;81mlet\x1b[0m",
		"\x1b[38;2;230;219;116m\"text\"\x1b[0m",
		"\x1b[38;2;117;113;94m// comment: naïve café\x1b[0m",
		"\x1b[38;5;141m42\x1b[0m",
		"self",
		"(",
		")",
		"{",
		"}",
		";",
		"\x1b[38;5;148mVec\x1b[0m",
		"->",
		"&mut",
		"=",
	];
	for line in 0..40 {
		out.extend_from_slice(
			format!(
				"\x1b[38;5;238m{:>5}\x1b[0m \x1b[38;5;238m│\x1b[0m ",
				line + 1
			)
			.as_bytes(),
		);
		out.extend(std::iter::repeat_n(b' ', 4 * rng.below(4)));
		for _ in 0..3 + rng.below(9) {
			out.extend_from_slice(rng.pick(TOKENS).as_bytes());
			out.push(b' ');
		}
		out.extend_from_slice(b"\r\n");
	}
}

fn prose(rng: &mut Rng, vocabulary: &[&str], line_words: usize, out: &mut Vec<u8>) {
	for _ in 0..24 {
		for i in 0..line_words / 2 + rng.below(line_words / 2) {
			if i > 0 {
				out.push(b' ');
			}
			out.extend_from_slice(rng.pick(vocabulary).as_bytes());
		}
		out.extend_from_slice(b"\r\n");
	}
}

fn unicode(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	const VOCABULARY: &[&str] = &[
		"héllo",
		"wörld",
		"naïve",
		"façade",
		"señor",
		"Ærøskøbing",
		"λόγος",
		"αλήθεια",
		"привет",
		"мир",
		"Straße",
		"ţară",
		"ĳssel",
		"→",
		"«quoted»",
		"—",
		"…",
		"ok",
	];
	prose(rng, VOCABULARY, 14, out);
}

fn cjk(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	const VOCABULARY: &[&str] = &[
		"日本語",
		"の",
		"テキスト",
		"端末",
		"中文",
		"字符",
		"한국어",
		"문자",
		"漢字",
		"表示",
		"、",
		"。",
	];
	prose(rng, VOCABULARY, 12, out);
}

fn emoji(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	const VOCABULARY: &[&str] = &[
		"🙂",
		"🚀",
		"👍🏽",
		"👨‍👩‍👧",
		"❤️",
		"🏳️‍🌈",
		"e\u{301}",
		"a\u{308}\u{323}",
		"✔",
		"⚠️",
		"🦀",
		"ok",
	];
	prose(rng, VOCABULARY, 10, out);
}

fn tui(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	out.extend_from_slice(b"\x1b[?2026h\x1b[H");
	let bar = size.cols.saturating_sub(30).max(10);
	for row in 1..=size.rows {
		out.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
		if row <= 4 {
			let fill = rng.below(bar);
			out.extend_from_slice(format!("\x1b[1;36m{row:>3}\x1b[0m[\x1b[32m").as_bytes());
			out.extend(std::iter::repeat_n(b'|', fill));
			out.extend_from_slice(b"\x1b[31m");
			out.extend(std::iter::repeat_n(b' ', bar - fill));
			out.extend_from_slice(format!("\x1b[0m{:>5.1}%]", rng.below(1000) as f32 / 10.0).as_bytes());
		} else {
			let selected = row == 5 + rng.below(size.rows.saturating_sub(4));
			if selected {
				out.extend_from_slice(b"\x1b[30;46m");
			}
			out.extend_from_slice(
				format!(
					"{:>7} me       20   0 {:>6}M {:>5}M S {:>4.1} {:>4.1} 0:{:02}.{:02} ",
					rng.below(99_999),
					rng.below(9999),
					rng.below(999),
					rng.below(1000) as f32 / 10.0,
					rng.below(1000) as f32 / 10.0,
					rng.below(60),
					rng.below(100)
				)
				.as_bytes(),
			);
			out.extend_from_slice(rng.pick(WORDS).as_bytes());
			out.extend_from_slice(b"\x1b[K\x1b[0m");
		}
	}
	out.extend_from_slice(b"\x1b[?2026l");
}

fn editor(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let bottom = size.rows.saturating_sub(2).max(2);
	out.extend_from_slice(format!("\x1b[1;{bottom}r").as_bytes());
	for _ in 0..16 {
		match rng.below(4) {
			// Scroll down one line and draw it (j at the bottom of the window).
			0 => out.extend_from_slice(format!("\x1b[{bottom};1H\r\n\x1b[K").as_bytes()),
			// Scroll up (k at the top).
			1 => out.extend_from_slice(b"\x1b[1;1H\x1bM\x1b[K"),
			// Open or delete a line (o, dd).
			2 => out.extend_from_slice(format!("\x1b[{};1H\x1b[L", 1 + rng.below(bottom)).as_bytes()),
			_ => out.extend_from_slice(format!("\x1b[{};1H\x1b[M", 1 + rng.below(bottom)).as_bytes()),
		}
		out.extend_from_slice(b"\x1b[38;5;81m    let\x1b[0m ");
		words(rng, 6, out);
		out.extend_from_slice(
			format!("\x1b[{};1H\x1b[7m-- INSERT --\x1b[0m\x1b[K", size.rows).as_bytes(),
		);
	}
	out.extend_from_slice(b"\x1b[r");
}

fn progress(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let width = size.cols.saturating_sub(24).clamp(10, 60);
	for step in 0..=20 {
		let done = width * step / 20;
		out.extend_from_slice(format!("\r\x1b[K\x1b[1;32m{:>3}%\x1b[0m [", step * 5).as_bytes());
		out.extend(std::iter::repeat_n("█".as_bytes(), done).flatten());
		out.extend(std::iter::repeat_n(b' ', width - done));
		out.extend_from_slice(format!("] {} MB/s", rng.below(999)).as_bytes());
	}
	out.extend_from_slice(b"\r\n");
}

fn truecolor(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	let phase = rng.below(256);
	for row in 0..size.rows.min(24) {
		for col in 0..size.cols {
			let (r, g) = ((col * 4 + phase) % 256, (row * 10 + phase) % 256);
			let b = (r + g) / 2;
			out.extend_from_slice(format!("\x1b[38;2;{r};{g};{b};48;2;{b};{r};{g}m▀").as_bytes());
		}
		out.extend_from_slice(b"\x1b[0m\r\n");
	}
}

fn cursor(rng: &mut Rng, size: Size, out: &mut Vec<u8>) {
	for _ in 0..256 {
		out.extend_from_slice(
			format!(
				"\x1b[{};{}Htext\x1b[K\x1b[2A\x1b[3C",
				1 + rng.below(size.rows),
				1 + rng.below(size.cols)
			)
			.as_bytes(),
		);
	}
}

fn shell(rng: &mut Rng, _: Size, out: &mut Vec<u8>) {
	for _ in 0..8 {
		let dir = rng.pick(WORDS);
		out.extend_from_slice(
			format!("\x1b]133;A\x07\x1b]7;file://host/home/me/{dir}\x07\x1b]0;me@host: ~/{dir}\x07")
				.as_bytes(),
		);
		out.extend_from_slice(
			format!("\x1b[32mme@host\x1b[0m:\x1b[34m~/{dir}\x1b[0m$ \x1b]133;B\x07").as_bytes(),
		);
		out.extend_from_slice(b"git status\r\n\x1b]133;C\x07");
		out
			.extend_from_slice(b"On branch \x1b]8;;https://example.com/main\x1b\\main\x1b]8;;\x1b\\\r\n");
		words(rng, 8, out);
		out.extend_from_slice(format!("\r\n\x1b]133;D;{}\x07", rng.below(2)).as_bytes());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn workloads_are_deterministic_and_end_on_whole_sequences() {
		let size = Size {
			cols: 100,
			rows: 30,
		};
		for workload in WORKLOADS {
			let data = workload.build(64 << 10, size);
			assert!(data.len() >= 64 << 10, "{}", workload.name);
			assert_eq!(data, workload.build(64 << 10, size), "{}", workload.name);
			let last_esc = data.iter().rposition(|&b| b == 0x1b);
			if let Some(esc) = last_esc {
				// The final sequence is complete: a final byte or ST follows.
				assert!(data.len() > esc + 1, "{}", workload.name);
			}
		}
	}

	#[test]
	fn sizes_are_kept_within_what_a_grid_can_be() {
		assert_eq!(Size::bounded(80, 24), Size { cols: 80, rows: 24 });
		// Serial consoles and some multiplexers report 0x0.
		assert_eq!(Size::bounded(0, 0), Size { cols: 2, rows: 2 });
		assert_eq!(
			Size::bounded(1001, 5000),
			Size {
				cols: 1000,
				rows: 1000
			}
		);
		assert_eq!(
			Size::bounded(1000, 2),
			Size {
				cols: 1000,
				rows: 2
			}
		);
	}
}
