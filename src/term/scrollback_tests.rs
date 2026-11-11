//! Tests for the trimmed scrollback ring.
use super::{
	Cell,
	scrollback::{Scrollback, publish_streamed, stream_copy, trimmed_len},
};

const PAD: Cell = Cell {
	ch: ' ',
	fg: 7,
	bg: 0,
	attrs: 0,
	width: 1,
	extra: super::NO_EXTRA,
};

fn row(text: &str, cols: usize) -> Vec<Cell> {
	let mut cells: Vec<Cell> = text.chars().map(|ch| Cell { ch, ..PAD }).collect();
	cells.resize(cols, PAD);
	cells
}

/// Pushes a row without a known used end.
fn push(s: &mut Scrollback, row: &[Cell], pad: Cell, limit: usize) -> (usize, usize) {
	s.push(row, row.len(), pad, limit)
}

fn text(cells: &[Cell]) -> String {
	cells.iter().map(|cell| cell.ch).collect()
}

fn lines(scrollback: &Scrollback) -> Vec<String> {
	scrollback.iter().map(text).collect()
}

#[test]
fn trimming_matches_a_scalar_scan_for_every_length_and_position() {
	let other = [
		Cell { ch: 'x', ..PAD },
		Cell { fg: 8, ..PAD },
		Cell { extra: 1, ..PAD },
		Cell { width: 2, ..PAD },
	];
	for len in 0..=19 {
		assert_eq!(trimmed_len(&vec![PAD; len], PAD), 0, "blank row of {len}");
		for at in 0..len {
			for (variant, cell) in other.iter().enumerate() {
				let mut row = vec![PAD; len];
				row[at] = *cell;
				assert_eq!(
					trimmed_len(&row, PAD),
					at + 1,
					"len {len}, at {at}, variant {variant}"
				);
				// A later cell decides alone.
				row[len - 1] = *cell;
				assert_eq!(trimmed_len(&row, PAD), len);
			}
		}
	}
}

#[test]
fn rows_are_stored_without_trailing_padding() {
	let mut s = Scrollback::default();
	assert_eq!(push(&mut s, &row("ab", 8), PAD, 10), (0, 2));
	assert_eq!(push(&mut s, &row("", 8), PAD, 10), (0, 0));
	assert_eq!(lines(&s), ["ab", ""]);
	assert_eq!((s.len(), s.is_empty()), (2, false));
}

#[test]
fn only_the_used_cells_are_scanned_and_stored() {
	let mut s = Scrollback::default();
	// The caller vouches that cells from `used` on are padding; they are
	// neither scanned nor stored, even when that is not true.
	assert_eq!(s.push(&row("abcd", 8), 2, PAD, 10), (0, 2));
	assert_eq!(s.push(&row("ab", 8), 6, PAD, 10), (0, 2));
	assert_eq!(s.push(&row("abc", 3), 0, PAD, 10), (0, 0));
	assert_eq!(lines(&s), ["ab", "ab", ""]);
}

#[test]
fn styled_trailing_blanks_are_kept() {
	let mut s = Scrollback::default();
	let mut cells = row("a", 4);
	cells[2].bg = 3;
	assert_eq!(push(&mut s, &cells, PAD, 10), (0, 3));
	assert_eq!(s[0][2].bg, 3);
	assert_eq!(s[0][1], PAD);
}

#[test]
fn the_line_limit_evicts_the_oldest_rows() {
	let mut s = Scrollback::default();
	let evicted: usize = ["a", "b", "c", "d"]
		.iter()
		.map(|line| push(&mut s, &row(line, 4), PAD, 2).0)
		.sum();
	assert_eq!(evicted, 2);
	assert_eq!(lines(&s), ["c", "d"]);
}

#[test]
fn the_ring_wraps_without_splitting_rows_and_keeps_every_line_within_the_limit() {
	let mut s = Scrollback::default();
	let mut expected = std::collections::VecDeque::new();
	for i in 0..500 {
		let line: String = (0..i % 7)
			.map(|k| char::from(b'a' + ((i + k) % 26) as u8))
			.collect();
		push(&mut s, &row(&line, 6), PAD, 9);
		expected.push_back(line.chars().take(6).collect::<String>());
		if expected.len() > 9 {
			expected.pop_front();
		}
		assert_eq!(lines(&s), Vec::from(expected.clone()), "after line {i}");
	}
}

#[test]
fn full_width_rows_fill_the_limit_after_wrapping() {
	let mut s = Scrollback::default();
	for i in 0..100 {
		push(&mut s, &row(&format!("{:05}", i), 5), PAD, 7);
	}
	assert_eq!(s.len(), 7);
	assert_eq!(s.get(0).map(text).as_deref(), Some("00093"));
	assert_eq!(s.get(6).map(text).as_deref(), Some("00099"));
	assert_eq!(s.get(7), None);
}

#[test]
fn draining_the_front_and_clearing_keep_later_pushes_intact() {
	let mut s = Scrollback::default();
	for line in ["a", "bb", "ccc"] {
		push(&mut s, &row(line, 4), PAD, 10);
	}
	s.drain_front(2);
	assert_eq!(lines(&s), ["ccc"]);
	s.drain_front(5);
	assert!(s.is_empty());
	push(&mut s, &row("d", 4), PAD, 10);
	s.clear();
	assert!(s.is_empty());
	push(&mut s, &row("e", 4), PAD, 10);
	assert_eq!(lines(&s), ["e"]);
}

#[test]
fn truncating_columns_shortens_only_longer_rows() {
	let mut s = Scrollback::default();
	for line in ["abcdef", "ab"] {
		push(&mut s, &row(line, 6), PAD, 10);
	}
	s.truncate_cols(3);
	assert_eq!(lines(&s), ["abc", "ab"]);
	// Wider rows pushed after a resize still fit in the ring.
	for _ in 0..30 {
		push(&mut s, &row("0123456789", 10), PAD, 10);
	}
	assert_eq!(s.len(), 10);
	assert_eq!(s[9].len(), 10);
}

#[test]
fn recoloring_reaches_every_stored_cell() {
	let mut s = Scrollback::default();
	let mut cells = row("ab", 4);
	cells[3].fg = 9;
	push(&mut s, &cells, PAD, 10);
	for cell in s.cells_mut() {
		cell.fg = 1;
	}
	assert!(s[0].iter().all(|cell| cell.fg == 1));
}

#[test]
fn clones_are_independent() {
	let mut s = Scrollback::default();
	push(&mut s, &row("a", 2), PAD, 4);
	let copy = s.clone();
	push(&mut s, &row("b", 2), PAD, 4);
	assert_eq!(lines(&copy), ["a"]);
	assert_eq!(lines(&s), ["a", "b"]);
	assert_eq!(s.iter().count(), 2);
}

/// A naive model: every row trimmed and kept in its own vector.
struct Model {
	rows: std::collections::VecDeque<Vec<Cell>>,
}

impl Model {
	fn push(&mut self, row: &[Cell], limit: usize) -> (usize, usize) {
		let len = row
			.iter()
			.rposition(|cell| *cell != PAD)
			.map_or(0, |last| last + 1);
		let evicted = (self.rows.len() + 1).saturating_sub(limit.max(1));
		self.rows.drain(..evicted);
		self.rows.push_back(row[..len].to_vec());
		(evicted, len)
	}
}

/// xorshift64: deterministic pseudo-random numbers for the differential test.
fn next(state: &mut u64) -> usize {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	*state as usize
}

#[test]
fn random_pushes_resizes_and_drains_match_the_naive_model() {
	for seed in 1..=40u64 {
		let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
		let (mut s, mut model) = (
			Scrollback::default(),
			Model {
				rows: Default::default(),
			},
		);
		// Odd seeds use limits large enough for the ring to grow in steps.
		let most = if seed % 2 == 0 { 20 } else { 300 };
		let (mut cols, mut limit) = (1 + next(&mut state) % 12, 1 + next(&mut state) % most);
		for step in 0..1500 {
			match next(&mut state) % 50 {
				0 => {
					cols = 1 + next(&mut state) % 12;
					s.truncate_cols(cols);
					for row in &mut model.rows {
						row.truncate(cols);
					}
				}
				1 => limit = 1 + next(&mut state) % most,
				2 => {
					let count = next(&mut state) % 4;
					s.drain_front(count);
					model.rows.drain(..count.min(model.rows.len()));
				}
				3 => {
					s.clear();
					model.rows.clear();
				}
				_ => {
					let mut row = vec![PAD; cols];
					for cell in row.iter_mut().take(next(&mut state) % (cols + 1)) {
						let value = next(&mut state);
						// Some cells equal the padding, also before the last kept one.
						if value % 3 != 0 {
							*cell = Cell {
								ch: char::from(b'a' + (value % 26) as u8),
								..PAD
							};
						}
					}
					assert_eq!(push(&mut s, &row, PAD, limit), model.push(&row, limit));
				}
			}
			assert_eq!(s.len(), model.rows.len(), "seed {seed}, step {step}");
			assert!(
				s.iter().eq(model.rows.iter().map(Vec::as_slice)),
				"seed {seed}, step {step}"
			);
		}
	}
}

#[test]
fn growth_keeps_every_row_until_the_line_limit() {
	let mut s = Scrollback::default();
	for i in 0..1000 {
		let line = "x".repeat(i % 50);
		assert_eq!(
			push(&mut s, &row(&line, 50), PAD, 1000),
			(0, i % 50),
			"row {i}"
		);
	}
	assert_eq!(s.len(), 1000);
	assert!(s.iter().enumerate().all(|(i, cells)| cells.len() == i % 50));
	assert_eq!(push(&mut s, &row("last", 50), PAD, 1000), (1, 4));
	assert_eq!(s[999].len(), 4);
	assert_eq!(s[0].len(), 1);
}

#[test]
fn a_limit_of_zero_still_keeps_the_newest_row() {
	let mut s = Scrollback::default();
	push(&mut s, &row("a", 3), PAD, 0);
	assert_eq!(push(&mut s, &row("b", 3), PAD, 0), (1, 1));
	assert_eq!(lines(&s), ["b"]);
}

#[test]
fn blank_rows_take_no_ring_space() {
	let mut s = Scrollback::default();
	push(&mut s, &row("keep", 4), PAD, 1000);
	for _ in 0..500 {
		push(&mut s, &row("", 4), PAD, 1000);
	}
	assert_eq!(s.len(), 501);
	assert_eq!(s.get(0).map(text).as_deref(), Some("keep"));
	assert!(s.iter().skip(1).all(<[Cell]>::is_empty));
}

#[test]
#[should_panic(expected = "scrollback row out of range")]
fn indexing_past_the_newest_row_panics() {
	let mut s = Scrollback::default();
	push(&mut s, &row("a", 2), PAD, 4);
	let _ = &s[1];
}

#[test]
fn streamed_copies_match_plain_copies_at_every_length_and_alignment() {
	let source: Vec<Cell> = (0..23u32)
		.map(|i| Cell {
			ch: char::from_u32(0x41 + i).unwrap(),
			fg: i,
			bg: !i,
			attrs: i as u8,
			width: (i % 3) as u8,
			extra: i as u16,
		})
		.collect();
	// Four offsets cover every position of a row within a 64-byte line.
	for offset in 0..4 {
		for len in 0..=source.len() {
			let mut copied = vec![PAD; source.len() + 8];
			stream_copy(&mut copied[offset..offset + len], &source[..len]);
			publish_streamed();
			let mut expected = vec![PAD; source.len() + 8];
			expected[offset..offset + len].copy_from_slice(&source[..len]);
			assert_eq!(copied, expected, "{len} cells at {offset}");
		}
	}
}

#[test]
#[should_panic]
fn streamed_copies_of_different_lengths_panic() {
	stream_copy(&mut [PAD; 3], &[PAD; 2]);
}

#[test]
fn pushed_rows_keep_every_field_through_wraps() {
	let mut s = Scrollback::default();
	for n in 0..300u32 {
		let len = (n as usize * 7) % 13;
		let styled: Vec<Cell> = (0..len as u32)
			.map(|i| Cell {
				ch: 'x',
				fg: n,
				bg: i,
				attrs: 1,
				width: 1,
				extra: n as u16,
			})
			.collect();
		let mut padded = styled.clone();
		padded.resize(12, PAD);
		push(&mut s, &padded, PAD, 40);
		assert_eq!(&s[s.len() - 1], &styled[..len.min(12)], "row {n}");
	}
	assert_eq!(s.len(), 40);
}

#[test]
fn streamed_copies_from_every_source_offset_land_whole() {
	let source: Vec<Cell> = ('a'..='z').map(|ch| Cell { ch, ..PAD }).collect();
	// Source and target chunks fall out of step unless both offsets match.
	for from in 0..4 {
		for to in 0..4 {
			let len = source.len() - from;
			let mut target = vec![PAD; len + to + 3];
			stream_copy(&mut target[to..to + len], &source[from..]);
			publish_streamed();
			assert_eq!(
				text(&target[to..to + len]),
				text(&source[from..]),
				"{from} to {to}"
			);
			assert!(
				target[..to].iter().all(|&cell| cell == PAD),
				"{from} to {to}"
			);
			assert!(
				target[to + len..].iter().all(|&cell| cell == PAD),
				"{from} to {to}"
			);
		}
	}
}

/// `trimmed_len` one cell at a time: the oracle for the four-cell scan.
fn trimmed_slowly(row: &[Cell], pad: Cell) -> usize {
	row
		.iter()
		.rposition(|&cell| cell != pad)
		.map_or(0, |last| last + 1)
}

#[test]
fn trimming_matches_a_cell_by_cell_scan_at_every_length_and_edge() {
	// Cells equal to the pad but for one field must never be trimmed.
	let near = [
		Cell { ch: 'x', ..PAD },
		Cell { fg: 8, ..PAD },
		Cell { bg: 1, ..PAD },
		Cell { attrs: 1, ..PAD },
		Cell { width: 2, ..PAD },
		Cell { extra: 1, ..PAD },
	];
	for len in 0..=13 {
		for at in 0..len {
			for &odd in &near {
				let mut cells = vec![PAD; len];
				cells[at] = odd;
				assert_eq!(
					trimmed_len(&cells, PAD),
					trimmed_slowly(&cells, PAD),
					"{len} {at} {odd:?}"
				);
				assert_eq!(trimmed_len(&cells, PAD), at + 1);
			}
		}
	}
}

#[test]
fn trimming_random_rows_matches_the_slow_scan() {
	let mut rng = crate::bench::corpus::Rng::new(41);
	for _ in 0..2000 {
		let len = rng.below(40);
		let cells: Vec<Cell> = (0..len)
			.map(|_| {
				if rng.below(3) == 0 {
					Cell { ch: 'a', ..PAD }
				} else {
					PAD
				}
			})
			.collect();
		assert_eq!(
			trimmed_len(&cells, PAD),
			trimmed_slowly(&cells, PAD),
			"{cells:?}"
		);
	}
}
