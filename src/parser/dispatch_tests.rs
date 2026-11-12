//! The x86-64-v3 variants of the parse loop and of `print_text` must leave a
//! terminal exactly as the baseline variants do.
use crate::{
	bench::corpus::Rng,
	parser::Parser,
	term::{Cell, Term},
	theme::{Theme, ThemeKind},
};

/// Everything a stream can change that the tests compare: cursor, screen
/// with extras resolved, modes and replies.
type State = (
	(usize, usize, bool, Option<char>),
	Vec<Vec<(Cell, Vec<char>, u16)>>,
	(bool, bool, usize),
	Vec<Vec<u8>>,
);

fn state(term: &Term) -> State {
	let resolve = |cell: Cell| {
		let marks = term.extras.marks(cell.extra).to_vec();
		(
			Cell { extra: 0, ..cell },
			marks,
			term.extras.hyperlink(cell.extra),
		)
	};
	let history = term.scrollback.iter().map(<[Cell]>::to_vec);
	let rows = history.chain(term.grid.iter().map(|row| row.to_vec()));
	(
		(term.cx, term.cy, term.wrap_pending, term.last_printed),
		rows
			.map(|row| row.into_iter().map(resolve).collect())
			.collect(),
		(
			term.insert_mode,
			term.charsets[term.active_charset],
			term.active_charset,
		),
		term.responses.clone(),
	)
}

fn terminal(cols: usize) -> Term {
	let mut term = Term::new(cols, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
	term.scrollback_limit = 40;
	term
}

const WORDS: &[&str] = &[
	"hello ",
	"x",
	"Größe ",
	"кириллица ",
	"ελληνικά",
	"─┼│",
	"日本語",
	"한국어",
	"🙂",
	"👍🏽",
	"👨\u{200d}👩\u{200d}👧",
	"❤\u{fe0f}",
	"e\u{301}",
	"a\u{308}\u{323}",
	"\u{301}",
	"\u{200d}",
	"\u{fe0f}",
	"\u{1100}\u{1161}\u{11a8}",
	"lqqk",
	"\t",
	"\r\n",
	"\x08",
];

const SEQUENCES: &[&str] = &[
	"\x1b[H",
	"\x1b[2;5H",
	"\x1b[3C",
	"\x1b[2D",
	"\x1b[A",
	"\x1b[K",
	"\x1b[2J",
	"\x1b[3J",
	"\x1b[1;31m",
	"\x1b[38;2;10;20;30m",
	"\x1b[0m",
	"\x1b[4h",
	"\x1b[4l",
	"\x1b[2@",
	"\x1b[P",
	"\x1b[L",
	"\x1b[M",
	"\x1b[?7l",
	"\x1b[?7h",
	"\x1b[?2026h",
	"\x1b[?2026l",
	"\x1b[?25$p",
	"\x1b[4$p",
	"\x1b[6n",
	"\x1b[2;3r",
	"\x1b[r",
	"\x1b(0",
	"\x1b(B",
	"\x0e",
	"\x0f",
	"\x1b)0",
	"\x1b7",
	"\x1b8",
	"\x1bM",
	"\x1b]8;;http://x\x1b\\",
	"\x1b]8;;\x1b\\",
	"\x1bP$qm\x1b\\",
	"\x1b[5b",
];

/// A random stream of text, controls and escape sequences.
fn stream(rng: &mut Rng) -> String {
	let mut out = String::new();
	for _ in 0..1 + rng.below(30) {
		out.push_str(if rng.below(3) == 0 {
			rng.pick(SEQUENCES)
		} else {
			rng.pick(WORDS)
		});
	}
	out
}

#[test]
fn the_v3_parse_loop_matches_the_baseline_on_random_streams_split_at_random() {
	let Some(cpu) = crate::cpu::V3::detect() else {
		return;
	};
	let mut rng = Rng::new(0xd15);
	for _ in 0..3_000 {
		let bytes = stream(&mut rng).into_bytes();
		let mut cuts: Vec<usize> = (0..rng.below(4))
			.map(|_| rng.below(bytes.len() + 1))
			.collect();
		cuts.sort_unstable();
		cuts.push(bytes.len());
		let cols = 1 + rng.below(12);
		let (mut baseline, mut v3) = (terminal(cols), terminal(cols));
		let (mut baseline_parser, mut v3_parser) = (Parser::new(), Parser::new());
		let mut start = 0;
		for &end in &cuts {
			let read = &bytes[start..end];
			baseline_parser.0.advance_with(&mut baseline, read);
			crate::cpu::run!(cpu, v3_parser.0.advance_with(&mut v3, read));
			start = end;
		}
		assert_eq!(
			state(&v3),
			state(&baseline),
			"{cols} columns, cut at {cuts:?}: {:?}",
			String::from_utf8_lossy(&bytes)
		);
	}
}

#[test]
fn v3_printing_matches_the_baseline_in_every_mode_and_width() {
	let Some(cpu) = crate::cpu::V3::detect() else {
		return;
	};
	// Sets up insert mode, the DEC graphics charset, a pending wrap, or none.
	let setups: [&[u8]; 4] = [b"", b"\x1b[4h", b"\x1b(0", b"\x1b[999Cx"];
	let mut rng = Rng::new(0x7e57);
	for _ in 0..400 {
		let text: String = (0..1 + rng.below(12)).map(|_| *rng.pick(WORDS)).collect();
		let text = text.replace(['\t', '\r', '\n', '\x08'], "");
		for cols in 1..=12 {
			for setup in setups {
				let mut baseline = terminal(cols);
				Parser::new().advance(&mut baseline, setup);
				let mut v3 = baseline.clone();
				baseline.print_text_with(text.as_str().into());
				crate::cpu::run!(cpu, v3.print_text_with(text.as_str().into()));
				assert_eq!(
					state(&v3),
					state(&baseline),
					"{cols} columns after {setup:?}: {text:?}"
				);
			}
		}
	}
}

/// Guards the tests above: they must reach both variants, so a broken v3
/// variant cannot hide behind an unexercised path.
#[test]
fn the_parse_loop_prints_through_both_paths_on_the_same_stream() {
	let Some(cpu) = crate::cpu::V3::detect() else {
		return;
	};
	let mut term = terminal(20);
	crate::cpu::run!(cpu, {
		Parser::new()
			.0
			.advance_with(&mut term, "ab\x1b[1mé日".as_bytes())
	});
	let row: String = term.grid.iter().next().unwrap()[..5]
		.iter()
		.map(|cell| char::from_u32(cell.ch as u32).unwrap_or(' '))
		.collect();
	assert_eq!(row, "abé日 ");
	assert_eq!((term.cx, term.cy), (5, 0));
}

/// Every benchmark workload, real-world output from logs to kitty images,
/// leaves the same terminal through both variants, read in pieces that cut
/// sequences and characters.
#[test]
fn every_benchmark_workload_parses_the_same_through_both_variants() {
	let Some(cpu) = crate::cpu::V3::detect() else {
		return;
	};
	let size = crate::bench::corpus::Size::bounded(40, 12);
	let fresh = || {
		let theme = Theme::from_kind(ThemeKind::Poop);
		let mut term = Term::new(size.cols, size.rows, theme, "/".into());
		term.scrollback_limit = 40;
		term
	};
	for workload in crate::bench::corpus::WORKLOADS {
		let bytes = workload.build(24 << 10, size);
		let (mut baseline, mut v3) = (fresh(), fresh());
		let (mut baseline_parser, mut v3_parser) = (Parser::new(), Parser::new());
		for read in bytes.chunks(4_093) {
			baseline_parser.0.advance_with(&mut baseline, read);
			crate::cpu::run!(cpu, v3_parser.0.advance_with(&mut v3, read));
		}
		assert!(
			state(&v3) == state(&baseline),
			"{}: the variants left different terminals",
			workload.name
		);
		assert_eq!(v3.title, baseline.title, "{}", workload.name);
	}
}
