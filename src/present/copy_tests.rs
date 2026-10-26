//! Tests of which pixel rows a frame copies into a window buffer: only
//! what changed since the frame the buffer holds, as told by its age, and
//! everything when that is unknown.
use super::copy::{History, Rows};

const ROWS: usize = 100;

fn some(bands: &[(usize, usize)]) -> Rows {
	Rows::Some(bands.to_vec())
}

#[test]
fn a_new_buffer_or_a_full_redraw_copies_every_row() {
	let history = History::new();
	assert_eq!(history.copy(0, Some(&[(4, 8)]), ROWS), Rows::All);
	assert_eq!(history.copy(1, None, ROWS), Rows::All);
}

#[test]
fn a_buffer_holding_the_last_frame_copies_only_what_changed() {
	let mut history = History::new();
	history.shown(None);
	assert_eq!(
		history.copy(1, Some(&[(4, 8), (20, 22)]), ROWS),
		some(&[(4, 8), (20, 22)])
	);
}

#[test]
fn an_older_buffer_also_copies_what_changed_in_the_frames_it_missed() {
	let mut history = History::new();
	history.shown(None);
	history.shown(Some(&[(10, 12)]));
	history.shown(Some(&[(50, 60)]));
	// Age 2 holds the frame before last: it missed the last frame's rows.
	assert_eq!(
		history.copy(2, Some(&[(4, 8)]), ROWS),
		some(&[(4, 8), (50, 60)])
	);
	assert_eq!(
		history.copy(3, Some(&[(4, 8)]), ROWS),
		some(&[(4, 8), (10, 12), (50, 60)])
	);
	// Past the full redraw, what it holds is unknown.
	assert_eq!(history.copy(4, Some(&[(4, 8)]), ROWS), Rows::All);
	// A buffer that missed a full redraw takes every row.
	history.shown(None);
	history.shown(Some(&[(1, 2)]));
	assert_eq!(
		history.copy(2, Some(&[(4, 8)]), ROWS),
		some(&[(1, 2), (4, 8)])
	);
	assert_eq!(history.copy(3, Some(&[(4, 8)]), ROWS), Rows::All);
}

#[test]
fn ages_older_than_the_history_copy_every_row() {
	let mut history = History::new();
	for start in 0..10 {
		history.shown(Some(&[(start, start + 1)]));
	}
	assert_eq!(history.copy(4, Some(&[]), ROWS), some(&[(7, 10)]));
	assert_eq!(history.copy(5, Some(&[]), ROWS), Rows::All);
	assert_eq!(history.copy(255, Some(&[]), ROWS), Rows::All);
}

#[test]
fn rows_are_merged_sorted_and_kept_inside_the_buffer() {
	let mut history = History::new();
	history.shown(None);
	history.shown(Some(&[(30, 40), (5, 9)]));
	let bands = [(38, 45), (9, 12), (90, 140), (200, 300), (60, 60), (70, 65)];
	let copied = history.copy(2, Some(&bands), ROWS);
	// Touching and overlapping bands join; empty and backwards ones and
	// rows past the buffer are dropped.
	assert_eq!(copied, some(&[(5, 12), (30, 45), (90, 100)]));
}

#[test]
fn nothing_changed_copies_nothing() {
	let mut history = History::new();
	history.shown(None);
	assert_eq!(history.copy(1, Some(&[]), ROWS), some(&[]));
}

#[test]
fn clearing_forgets_every_frame_as_after_a_resize() {
	let mut history = History::new();
	history.shown(None);
	history.shown(Some(&[(1, 2)]));
	history.clear();
	assert_eq!(history.copy(1, Some(&[(4, 8)]), ROWS), Rows::All);
	history.shown(None);
	assert_eq!(history.copy(1, Some(&[(4, 8)]), ROWS), some(&[(4, 8)]));
}

#[test]
fn every_row_copied_is_the_same_as_a_full_copy_for_any_age() {
	// The guarantee, by brute force: after copying the planned rows into a
	// buffer of each age, it equals the newest frame row for row.
	let mut rng = crate::bench::corpus::Rng::new(0x5eed_c0b1);
	let mut frames: Vec<Vec<u32>> = vec![vec![0; ROWS]];
	let mut history = History::new();
	history.shown(None);
	for frame in 1..200u32 {
		let mut next = frames.last().unwrap().clone();
		let bands: Vec<_> = (0..rng.below(4))
			.map(|_| {
				let start = rng.below(ROWS + 10);
				(start, start + rng.below(12))
			})
			.collect();
		// One frame in eight is redrawn whole.
		let whole = rng.below(8) == 0;
		for &(start, end) in &bands {
			next[start.min(ROWS)..end.min(ROWS)].fill(frame);
		}
		if whole {
			next.fill(frame);
		}
		let changed = (!whole).then_some(&bands[..]);
		for age in 1..=frames.len().min(6) {
			let mut buffer = frames[frames.len() - age].clone();
			match history.copy(age as u8, changed, ROWS) {
				Rows::All => buffer.copy_from_slice(&next),
				Rows::Some(rows) => {
					for (start, end) in rows {
						buffer[start..end].copy_from_slice(&next[start..end]);
					}
				}
			}
			assert_eq!(buffer, next, "frame {frame}, age {age}");
		}
		history.shown(changed);
		frames.push(next);
	}
}
