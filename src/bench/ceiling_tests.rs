//! The ceiling's model against a plain cell-by-cell terminal, its SIMD scans
//! against plain loops, and the timed traffic's bounds.
use super::{
	ceiling::{Counts, count, count_controls, rate, traffic},
	corpus::{Rng, Size},
};
use crate::{
	term::Cell,
	vt::{Handler, Params, Parser, Text},
};

/// The model written the slow way: a grid of characters, one at a time,
/// with lines scrolled into a list, fed by the same tokenizer.
struct Reference {
	cols: usize,
	rows: usize,
	grid: Vec<Vec<Option<char>>>,
	x: usize,
	y: usize,
	history: Vec<Vec<char>>,
	cells: usize,
}

impl Reference {
	fn run(data: &[u8], cols: usize, rows: usize) -> Self {
		let (cols, rows) = (cols.max(1), rows.max(1));
		let mut model = Self {
			cols,
			rows,
			grid: vec![vec![None; cols]; rows],
			x: 0,
			y: 0,
			history: Vec::new(),
			cells: 0,
		};
		Parser::new().advance(&mut model, data);
		model
	}

	fn put_char(&mut self, ch: char) {
		if self.x == self.cols {
			self.x = 0;
			self.line_feed();
		}
		self.grid[self.y][self.x] = Some(ch);
		self.x += 1;
		self.cells += 1;
	}

	fn line_feed(&mut self) {
		if self.y + 1 < self.rows {
			self.y += 1;
			return;
		}
		let top = self.grid.remove(0);
		// The used cells: up to the last one ever written in the row.
		let used = top
			.iter()
			.rposition(Option::is_some)
			.map_or(0, |last| last + 1);
		self
			.history
			.push(top[..used].iter().map(|ch| ch.unwrap_or(' ')).collect());
		self.grid.push(vec![None; self.cols]);
	}

	fn counts(&self) -> Counts {
		Counts {
			cells: self.cells,
			kept: self.history.iter().map(Vec::len).sum(),
			lines: self.history.len(),
		}
	}
}

impl Handler for Reference {
	fn print(&mut self, text: Text<'_>) {
		text.chars().for_each(|ch| self.put_char(ch));
	}
	fn execute(&mut self, byte: u8) {
		match byte {
			b'\n' | 0x0b | 0x0c => self.line_feed(),
			b'\r' => self.x = 0,
			_ => {}
		}
	}
	fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
	fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
	fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn put(&mut self, _: &[u8]) {}
	fn unhook(&mut self) {}
}

fn agrees(data: &[u8], cols: usize, rows: usize) {
	let expected = Reference::run(data, cols, rows).counts();
	let size = Size { cols, rows };
	assert_eq!(
		count(data, size),
		expected,
		"{cols}x{rows}: {:?}",
		String::from_utf8_lossy(data)
	);
}

#[test]
fn counts_match_the_reference_on_hand_picked_streams() {
	let streams: [&[u8]; 9] = [
		b"",
		b"abc\r\nde\x1b[1m\r\nf",
		b"abcdefg\x1b]0;title\x07",
		b"aaaa\rbb\r\n\r\n\r\n",
		"日本語 😀 é\r\n".as_bytes(),
		b"a\xffb\xc3\r\n",
		b"\x1bPq#0~-\x1b\\x\x1b_Gf=1;AA\x1b\\y\n",
		b"\x07\x08\x7f\x0b\x0c tail",
		b"\x1b[",
	];
	for stream in streams {
		for (cols, rows) in [(1, 1), (3, 1), (4, 2), (80, 24)] {
			agrees(stream, cols, rows);
		}
	}
}

#[test]
fn counts_match_the_reference_on_random_streams() {
	let pieces: [&[u8]; 12] = [
		b"word ",
		b"\r\n",
		b"\r",
		b"\n",
		b"\x1b[38;5;1m",
		b"\x1b]8;;http://x\x1b\\",
		"日本".as_bytes(),
		"é".as_bytes(),
		b"\xe6\x97",
		b"\x1b7",
		b"\x00\x7f",
		b"0123456789",
	];
	let mut rng = Rng::new(0xce11);
	for _ in 0..300 {
		let len = rng.below(60);
		let stream: Vec<u8> = (0..len)
			.flat_map(|_| rng.pick(&pieces).iter().copied())
			.collect();
		let (cols, rows) = (1 + rng.below(12), 1 + rng.below(5));
		agrees(&stream, cols, rows);
	}
}

#[test]
fn counts_match_the_reference_on_every_workload() {
	let size = Size { cols: 40, rows: 6 };
	for workload in super::corpus::WORKLOADS {
		let data = workload.build(32 << 10, size);
		agrees(&data, size.cols, size.rows);
	}
}

#[test]
fn long_lines_wrap_into_full_rows_of_history() {
	let counts = count(b"abcdefgh", Size { cols: 3, rows: 1 });
	assert_eq!(
		counts,
		Counts {
			cells: 8,
			kept: 6,
			lines: 2
		}
	);
	// A zero-sized screen counts as one cell.
	let counts = count(b"ab", Size { cols: 0, rows: 0 });
	assert_eq!(
		counts,
		Counts {
			cells: 2,
			kept: 1,
			lines: 1
		}
	);
}

#[test]
fn controls_are_counted_like_a_plain_loop_at_every_length_and_position() {
	for len in 0..80 {
		for at in 0..=len {
			let mut bytes = vec![b'a'; len];
			if at < len {
				bytes[at] = if at % 2 == 0 { 0x1b } else { 0x7f };
			}
			let controls = bytes.iter().filter(|&&b| b < 0x20).count();
			assert_eq!(count_controls(&bytes), controls, "len {len}, at {at}");
		}
	}
	// DEL and bytes from 0x80 are not below 0x20.
	assert_eq!(count_controls(&[0x7f, 0x80, 0xff, 0x1f, 0x00]), 2);
}

#[test]
fn sequences_write_no_cells() {
	let streams: [&[u8]; 6] = [
		b"\x1b[1;31m\x1b[?25h\x1b[H",
		b"\x1b]0;a title\x07",
		b"\x1b]8;;https://x\x1b\\",
		b"\x1bPq#0;2;0;0;0~-\x1b\\",
		b"\x1b_Gf=32;AAAA\x1b\\",
		b"\x1b7\x1b8\x07\x08",
	];
	for stream in streams {
		assert_eq!(
			count(stream, Size { cols: 10, rows: 2 }),
			Counts::default(),
			"{stream:?}"
		);
	}
}

#[test]
fn traffic_writes_every_cell_and_copies_every_kept_one() {
	let mut grid = vec![
		Cell {
			ch: 'g',
			..Cell::default()
		};
		8
	];
	let mut history = vec![
		Cell {
			ch: '#',
			..Cell::default()
		};
		20
	];
	let counts = Counts {
		cells: 30,
		kept: 12,
		lines: 3,
	};
	assert_eq!(traffic(b"a\nb\x1b", counts, &mut grid, &mut history), 2);
	assert!(grid.iter().all(|cell| *cell == Cell::default()));
	assert_eq!(history.iter().filter(|cell| cell.ch == ' ').count(), 12);
	// More kept cells than history holds wrap around without overrunning it,
	// with a history smaller than one block of the grid.
	let mut small = vec![Cell::default(); 5];
	traffic(b"", Counts { kept: 23, ..counts }, &mut grid, &mut small);
	assert_eq!(small.len(), 5);
}

#[test]
fn rates_are_positive_for_every_kind_of_stream() {
	let size = Size { cols: 20, rows: 4 };
	let cjk = "日本\r\n".repeat(50);
	for stream in [&b"text"[..], b"\x1b[1m\x1b[0m", cjk.as_bytes()] {
		let rate = rate(stream, size);
		assert!(rate > 0.0, "{stream:?}: {rate}");
	}
}

/// Lines and cells a real terminal keeps in history for `data`.
fn kept_by_term(data: &[u8], cols: usize, rows: usize) -> (usize, usize) {
	use crate::theme::{Theme, ThemeKind};
	let mut term = crate::term::Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	term.scrollback_limit = 100_000;
	crate::parser::Parser::new().advance(&mut term, data);
	let lines = term.scrollback.len();
	let cells = (0..lines).map(|line| term.scrollback[line].len()).sum();
	(lines, cells)
}

#[test]
fn the_model_keeps_what_a_real_terminal_keeps_for_plain_text() {
	// Text, line ends and sequences that leave the colors alone, with no
	// trailing blanks and no wide characters, so the terminal's history holds
	// exactly the used cells. (A row scrolled in while a color is set is
	// blanked in that color, and the terminal keeps it whole.)
	let pieces: [&[u8]; 8] = [
		b"word",
		b"\r\n",
		b"\x1b[1mbold\x1b[0m",
		b"0123456789abcdef",
		"é".as_bytes(),
		b"\r",
		b"x",
		b"\x1b]0;title\x07",
	];
	let mut rng = Rng::new(0x7e41);
	for _ in 0..200 {
		let len = rng.below(80);
		let mut stream: Vec<u8> = (0..len)
			.flat_map(|_| rng.pick(&pieces).iter().copied())
			.collect();
		stream.extend_from_slice(b"\r\n");
		let (cols, rows) = (2 + rng.below(20), 1 + rng.below(6));
		let counts = count(&stream, Size { cols, rows });
		let (lines, cells) = kept_by_term(&stream, cols, rows);
		let text = String::from_utf8_lossy(&stream);
		assert_eq!(
			(counts.lines, counts.kept),
			(lines, cells),
			"{cols}x{rows}: {text:?}"
		);
	}
}

#[test]
fn counts_stay_within_their_bounds_on_every_workload_and_size() {
	for (cols, rows) in [(1, 1), (7, 3), (80, 24), (300, 2)] {
		let size = Size { cols, rows };
		for workload in super::corpus::WORKLOADS {
			let data = workload.build(16 << 10, size);
			let counts = count(&data, size);
			let name = workload.name;
			assert!(
				counts.kept <= counts.cells,
				"{name} {cols}x{rows}: {counts:?}"
			);
			assert!(
				counts.kept <= counts.lines * cols,
				"{name} {cols}x{rows}: {counts:?}"
			);
			assert!(
				counts.cells <= data.len(),
				"{name}: a character takes a byte at least"
			);
		}
	}
}

#[test]
fn history_is_written_in_order_and_wraps_to_its_start() {
	let mut grid: Vec<Cell> = (0..4)
		.map(|i| Cell {
			fg: i,
			..Cell::default()
		})
		.collect();
	let fill = Cell {
		fg: 99,
		..Cell::default()
	};
	let mut history = vec![fill; 10];
	// No cells written, so the grid keeps its marks. Kept cells are copied
	// in blocks of up to 4 from the grid's start, one after another.
	let counts = Counts {
		cells: 0,
		kept: 10,
		lines: 3,
	};
	traffic(b"", counts, &mut grid, &mut history);
	let fg: Vec<u32> = history.iter().map(|cell| cell.fg).collect();
	assert_eq!(fg, [0, 1, 2, 3, 0, 1, 2, 3, 0, 1], "blocks of 4, 4 and 2");
	// A block that does not fit at the end is not split: it starts over at
	// the beginning and leaves the end as it was.
	for kept in [11, 12, 15] {
		let mut history = vec![fill; 10];
		traffic(b"", Counts { kept, ..counts }, &mut grid, &mut history);
		let fg: Vec<u32> = history.iter().map(|cell| cell.fg).collect();
		assert_eq!(fg[8..], [99, 99], "{kept} kept");
		assert_eq!(fg[..8], [0, 1, 2, 3, 0, 1, 2, 3], "{kept} kept");
	}
}

#[test]
fn a_wider_screen_than_any_line_keeps_each_line_once() {
	let counts = count(b"ab\r\ncde\r\nf\r\n", Size { cols: 200, rows: 1 });
	assert_eq!(
		counts,
		Counts {
			cells: 6,
			kept: 6,
			lines: 3
		}
	);
	// One column: every character is a line of its own.
	let counts = count(b"abc", Size { cols: 1, rows: 1 });
	assert_eq!(
		counts,
		Counts {
			cells: 3,
			kept: 2,
			lines: 2
		}
	);
}

#[test]
fn wide_text_asks_for_no_more_than_a_real_terminal_keeps() {
	// The terminal keeps two cells per wide character and the model one, so
	// on CJK and emoji the ceiling stays an upper bound: it never moves more
	// than the terminal must.
	let lines = [
		"日本語のテキスト",
		"한국어 텍스트",
		"😀😃😄 emoji",
		"mixed 中文 text",
	];
	for (cols, rows) in [(10, 2), (20, 3), (80, 1)] {
		for line in lines {
			let stream = format!("{line}\r\n").repeat(30);
			let counts = count(stream.as_bytes(), Size { cols, rows });
			let (lines, cells) = kept_by_term(stream.as_bytes(), cols, rows);
			assert!(
				counts.kept <= cells,
				"{line:?} at {cols}x{rows}: {counts:?} vs {cells}"
			);
			assert!(
				counts.lines <= lines,
				"{line:?} at {cols}x{rows}: {counts:?} vs {lines}"
			);
			assert_eq!(counts.cells, 30 * line.chars().count(), "{line:?}");
		}
	}
}
