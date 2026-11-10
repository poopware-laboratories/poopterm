//! Tests of the pixel rows a GPU frame changed: the bars always, the grid's
//! damaged rows, all of it after a scroll, and the padding around it.
use super::present_bands::{Layout, changed_bands};
use crate::render::damage::GridDamage;

/// A 600-pixel window: tabs to 44, 30 rows of 18 from 48, padding from 46
/// to 590, the status bar from 576.
const LAYOUT: Layout = Layout {
	top: 48,
	ch: 18,
	rows: 30,
	height: 600,
	status_top: 576,
	padding: (46, 590),
};

fn damage(scroll: usize, rows: &[usize]) -> GridDamage {
	let mut damaged = vec![false; LAYOUT.rows];
	for &row in rows {
		damaged[row] = true;
	}
	GridDamage {
		scroll,
		rows: damaged,
	}
}

#[test]
fn a_layout_is_made_from_the_windows_parts_in_order() {
	let layout = Layout::new([48, 18, 30], 600, 576, (46, 590));
	let fields = (layout.top, layout.ch, layout.rows, layout.height);
	assert_eq!(fields, (48, 18, 30, 600));
	assert_eq!((layout.status_top, layout.padding), (576, (46, 590)));
	// The same bands as the layout written out.
	let damage = damage(0, &[3]);
	assert_eq!(
		changed_bands(&damage, &layout),
		changed_bands(&damage, &LAYOUT)
	);
}

#[test]
fn no_damage_still_names_the_bars_and_padding() {
	// The tab bar and padding merge into one band at the top.
	assert_eq!(
		changed_bands(&damage(0, &[]), &LAYOUT),
		[(0, 48), (576, 600)]
	);
}

#[test]
fn a_damaged_row_is_its_cell_height() {
	assert_eq!(
		changed_bands(&damage(0, &[2]), &LAYOUT),
		[(0, 48), (84, 102), (576, 600)]
	);
}

#[test]
fn neighbouring_rows_merge_into_one_band() {
	assert_eq!(
		changed_bands(&damage(0, &[2, 3, 5]), &LAYOUT),
		[(0, 48), (84, 120), (138, 156), (576, 600)]
	);
}

#[test]
fn a_scroll_changes_the_whole_grid() {
	// Kept rows move on the GPU, so every one of them changed.
	assert_eq!(changed_bands(&damage(1, &[29]), &LAYOUT), [(0, 600)]);
}

#[test]
fn bands_stay_inside_the_window() {
	// A grid taller than the window, as while it shrinks.
	let short = Layout {
		height: 300,
		status_top: 280,
		padding: (46, 296),
		..LAYOUT
	};
	let bands = changed_bands(&damage(0, &[20, 29]), &short);
	assert!(
		bands.iter().all(|&(start, end)| start < end && end <= 300),
		"{bands:?}"
	);
	assert_eq!(bands, [(0, 48), (280, 300)]);
}

#[test]
fn bands_are_sorted_and_never_overlap() {
	let mut state = 0x9e37_79b9_7f4a_7c15_u64;
	for case in 0..300 {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		let rows: Vec<usize> = (0..LAYOUT.rows)
			.filter(|row| state >> row & 1 == 1)
			.collect();
		let scroll = usize::from(case % 7 == 0);
		let bands = changed_bands(&damage(scroll, &rows), &LAYOUT);
		for pair in bands.windows(2) {
			assert!(pair[0].1 < pair[1].0, "case {case}: {bands:?}");
		}
		// Every damaged row lies in a band.
		for row in rows {
			let y = LAYOUT.top + row * LAYOUT.ch;
			let inside = bands
				.iter()
				.any(|&(start, end)| start <= y && y + LAYOUT.ch <= end);
			assert!(inside, "case {case}: row {row} in {bands:?}");
		}
	}
}
