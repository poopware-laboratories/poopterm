//! Property tests of the pixel rows a GPU frame changed: for random layouts
//! and damage, the bands cover exactly the pixels a per-pixel model says
//! changed, sorted, apart, and inside the window.
use super::present_bands::{Layout, changed_bands};
use crate::render::damage::GridDamage;

struct Random(u64);

impl Random {
	fn below(&mut self, n: usize) -> usize {
		self.0 ^= self.0 << 13;
		self.0 ^= self.0 >> 7;
		self.0 ^= self.0 << 17;
		(self.0 % n as u64) as usize
	}
}

/// Whether pixel row `y` changed, decided for that row alone.
fn changed(y: usize, damage: &GridDamage, layout: &Layout) -> bool {
	let grid_bottom = (layout.top + layout.rows * layout.ch).min(layout.height);
	let in_grid = (layout.top..grid_bottom).contains(&y);
	let row = y.checked_sub(layout.top).map(|offset| offset / layout.ch);
	let damaged = in_grid && row.is_some_and(|row| damage.rows[row]);
	y >= layout.status_top
		|| y < layout.top
		|| (grid_bottom..layout.padding.1).contains(&y)
		|| (in_grid && damage.scroll > 0)
		|| damaged
}

#[test]
fn the_bands_cover_exactly_the_changed_pixels_in_order() {
	let mut random = Random(0x3c6e_f372_fe94_f82b);
	for case in 0..3000 {
		let height = 1 + random.below(300);
		let (top, ch, rows) = (random.below(40), 1 + random.below(24), random.below(20));
		let status_top = random.below(height + 8);
		let padding = (random.below(top + 1), random.below(height + 8));
		let layout = Layout::new([top, ch, rows], height, status_top, padding);
		let damage = GridDamage {
			scroll: random.below(3) / 2,
			rows: (0..rows).map(|_| random.below(4) == 0).collect(),
		};
		let bands = changed_bands(&damage, &layout);
		let mut covered = vec![false; height];
		for &(start, end) in &bands {
			assert!(start < end && end <= height, "case {case}: {bands:?}");
			covered[start..end].fill(true);
		}
		for pair in bands.windows(2) {
			assert!(pair[0].1 < pair[1].0, "case {case}: not merged: {bands:?}");
		}
		for (y, &named) in covered.iter().enumerate() {
			let want = changed(y, &damage, &layout);
			assert_eq!(named, want, "case {case}: row {y} of {layout:?}, {bands:?}");
		}
	}
}
