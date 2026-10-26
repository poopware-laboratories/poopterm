//! Tests of which pixel rows a frame copies into a window buffer: only
//! what changed since the frame the buffer holds, as told by its age, and
//! everything when that is unknown.
use super::copy::{History, Rows};

const ROWS: usize = 100;

fn some(bands: &[(usize, usize)]) -> Rows {
	Rows::Some(bands.to_vec())
}

/// Shows a frame that changed `bands`, or every row.
fn show(history: &mut History, bands: Option<&[(usize, usize)]>) {
	history.changed(bands);
	history.shown();
}

/// What a frame changing `bands` would copy into a buffer of age `age`.
fn copy(history: &History, age: u8, bands: Option<&[(usize, usize)]>) -> Rows {
	let mut history = history.clone();
	history.changed(bands);
	history.copy(age, ROWS)
}

#[test]
fn a_new_buffer_or_a_full_redraw_copies_every_row() {
	let history = History::new();
	assert_eq!(copy(&history, 0, Some(&[(4, 8)])), Rows::All);
	assert_eq!(copy(&history, 1, None), Rows::All);
}

#[test]
fn a_buffer_holding_the_last_frame_copies_only_what_changed() {
	let mut history = History::new();
	show(&mut history, None);
	assert_eq!(
		copy(&history, 1, Some(&[(4, 8), (20, 22)])),
		some(&[(4, 8), (20, 22)])
	);
}

#[test]
fn an_older_buffer_also_copies_what_changed_in_the_frames_it_missed() {
	let mut history = History::new();
	show(&mut history, None);
	show(&mut history, Some(&[(10, 12)]));
	show(&mut history, Some(&[(50, 60)]));
	// Age 2 holds the frame before last: it missed the last frame's rows.
	assert_eq!(
		copy(&history, 2, Some(&[(4, 8)])),
		some(&[(4, 8), (50, 60)])
	);
	assert_eq!(
		copy(&history, 3, Some(&[(4, 8)])),
		some(&[(4, 8), (10, 12), (50, 60)])
	);
	// Past the full redraw, what it holds is unknown.
	assert_eq!(copy(&history, 4, Some(&[(4, 8)])), Rows::All);
	// A buffer that missed a full redraw takes every row.
	show(&mut history, None);
	show(&mut history, Some(&[(1, 2)]));
	assert_eq!(copy(&history, 2, Some(&[(4, 8)])), some(&[(1, 2), (4, 8)]));
	assert_eq!(copy(&history, 3, Some(&[(4, 8)])), Rows::All);
}

#[test]
fn ages_older_than_the_history_copy_every_row() {
	let mut history = History::new();
	for start in 0..10 {
		show(&mut history, Some(&[(start, start + 1)]));
	}
	assert_eq!(copy(&history, 4, Some(&[])), some(&[(7, 10)]));
	assert_eq!(copy(&history, 5, Some(&[])), Rows::All);
	assert_eq!(copy(&history, 255, Some(&[])), Rows::All);
}

#[test]
fn rows_are_merged_sorted_and_kept_inside_the_buffer() {
	let mut history = History::new();
	show(&mut history, None);
	show(&mut history, Some(&[(30, 40), (5, 9)]));
	let bands = [(38, 45), (9, 12), (90, 140), (200, 300), (60, 60), (70, 65)];
	let copied = copy(&history, 2, Some(&bands));
	// Touching and overlapping bands join; empty and backwards ones and
	// rows past the buffer are dropped.
	assert_eq!(copied, some(&[(5, 12), (30, 45), (90, 100)]));
}

#[test]
fn nothing_changed_copies_nothing() {
	let mut history = History::new();
	show(&mut history, None);
	assert_eq!(copy(&history, 1, Some(&[])), some(&[]));
}

#[test]
fn clearing_forgets_every_frame_as_after_a_resize() {
	let mut history = History::new();
	show(&mut history, None);
	show(&mut history, Some(&[(1, 2)]));
	history.clear();
	assert_eq!(copy(&history, 1, Some(&[(4, 8)])), Rows::All);
	show(&mut history, None);
	assert_eq!(copy(&history, 1, Some(&[(4, 8)])), some(&[(4, 8)]));
}

#[test]
fn every_row_copied_is_the_same_as_a_full_copy_for_any_age() {
	// The guarantee, by brute force: after copying the planned rows into a
	// buffer of each age, it equals the newest frame row for row.
	let mut rng = crate::bench::corpus::Rng::new(0x5eed_c0b1);
	let mut frames: Vec<Vec<u32>> = vec![vec![0; ROWS]];
	let mut history = History::new();
	show(&mut history, None);
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
			match copy(&history, age as u8, changed) {
				Rows::All => buffer.copy_from_slice(&next),
				Rows::Some(rows) => {
					for (start, end) in rows {
						buffer[start..end].copy_from_slice(&next[start..end]);
					}
				}
			}
			assert_eq!(buffer, next, "frame {frame}, age {age}");
		}
		show(&mut history, changed);
		frames.push(next);
	}
}

#[test]
fn rows_changed_while_no_frame_was_shown_carry_over_to_the_next() {
	let mut history = History::new();
	show(&mut history, None);
	// Every buffer was held: these frames were drawn but never shown.
	history.changed(Some(&[(10, 12)]));
	history.changed(Some(&[(40, 41)]));
	history.changed(Some(&[(11, 20)]));
	assert_eq!(history.copy(1, ROWS), some(&[(10, 20), (40, 41)]));
	assert_eq!(history.damage(ROWS), some(&[(10, 20), (40, 41)]));
	history.shown();
	// They count as one frame for older buffers.
	history.changed(Some(&[(1, 2)]));
	assert_eq!(history.copy(2, ROWS), some(&[(1, 2), (10, 20), (40, 41)]));
	assert_eq!(history.damage(ROWS), some(&[(1, 2)]));
}

#[test]
fn a_full_redraw_while_no_frame_was_shown_damages_every_row() {
	let mut history = History::new();
	show(&mut history, None);
	history.changed(Some(&[(10, 12)]));
	history.changed(None);
	history.changed(Some(&[(40, 41)]));
	assert_eq!(history.copy(1, ROWS), Rows::All);
	assert_eq!(history.damage(ROWS), Rows::All);
	history.shown();
	// And older buffers missed it whole.
	history.changed(Some(&[(1, 2)]));
	assert_eq!(history.copy(1, ROWS), some(&[(1, 2)]));
	assert_eq!(history.copy(2, ROWS), Rows::All);
}

#[test]
fn the_damage_told_the_compositor_is_the_changed_rows_inside_the_buffer() {
	let mut history = History::new();
	// Nothing shown yet: the whole buffer is new.
	assert_eq!(history.damage(ROWS), Rows::All);
	show(&mut history, None);
	assert_eq!(history.damage(ROWS), some(&[]));
	history.changed(Some(&[(95, 120), (3, 4), (2, 3)]));
	assert_eq!(history.damage(ROWS), some(&[(2, 4), (95, 100)]));
	history.clear();
	assert_eq!(history.damage(ROWS), Rows::All);
}
