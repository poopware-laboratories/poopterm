//! Tests of how the atlas grows when a frame lacked room in it: more pages
//! when they ran out, larger pages for a bitmap wider than one, always
//! within the device's limits and the memory budget, and never in vain.
use super::atlas::{BUDGET, Wanted, grown};

/// A device making images 16384 pixels wide and 2048 layers deep.
const LIMITS: [u32; 2] = [16384, 2048];
const PAGES: Wanted = Wanted {
	side: 0,
	pages: true,
};

fn side(side: usize) -> Wanted {
	Wanted { side, pages: false }
}

#[test]
fn an_atlas_that_lacked_nothing_does_not_grow() {
	assert_eq!(grown((1024, 4), Wanted::default(), LIMITS, BUDGET), None);
	// A bitmap the pages already hold was refused for want of pages only.
	assert_eq!(grown((1024, 4), side(1024), LIMITS, BUDGET), None);
}

#[test]
fn pages_that_ran_out_double_until_the_budget_is_reached() {
	assert_eq!(grown((1024, 4), PAGES, LIMITS, BUDGET), Some((1024, 8)));
	// 64 pages of 1024 square are the 256 MiB budget.
	assert_eq!(grown((1024, 32), PAGES, LIMITS, BUDGET), Some((1024, 64)));
	assert_eq!(grown((1024, 64), PAGES, LIMITS, BUDGET), None);
	// Short of doubling, as many as the budget holds.
	assert_eq!(grown((1024, 40), PAGES, LIMITS, BUDGET), Some((1024, 64)));
}

#[test]
fn pages_never_outnumber_the_layers_the_device_makes() {
	let limits = [16384, 6];
	assert_eq!(grown((1024, 4), PAGES, limits, BUDGET), Some((1024, 6)));
	assert_eq!(grown((1024, 6), PAGES, limits, BUDGET), None);
}

#[test]
fn a_bitmap_wider_than_a_page_makes_pages_the_next_power_of_two_holding_it() {
	for (wide, grows_to) in [(1025, 2048), (2000, 2048), (2048, 2048), (2049, 4096)] {
		let grew = grown((1024, 4), side(wide), LIMITS, BUDGET);
		assert_eq!(grew, Some((grows_to, 4)), "{wide}");
	}
}

#[test]
fn larger_pages_keep_fewer_of_them_within_the_budget() {
	// A page 8192 square is the whole budget.
	assert_eq!(
		grown((1024, 4), side(5000), LIMITS, BUDGET),
		Some((8192, 1))
	);
	// One 16384 square would take a GiB.
	assert_eq!(grown((1024, 4), side(9000), LIMITS, BUDGET), None);
}

#[test]
fn pages_are_never_wider_than_the_device_makes() {
	let limits = [3000, 2048];
	assert_eq!(
		grown((1024, 4), side(2500), limits, BUDGET),
		Some((3000, 4))
	);
	assert_eq!(grown((1024, 4), side(3001), limits, BUDGET), None);
}

#[test]
fn a_bitmap_too_wide_with_pages_run_out_grows_both() {
	let both = Wanted {
		side: 2000,
		pages: true,
	};
	assert_eq!(grown((1024, 4), both, LIMITS, BUDGET), Some((2048, 8)));
}

#[test]
fn growing_again_and_again_ends_within_the_budget() {
	let (mut atlas, mut steps) = ((64, 2), 0);
	while let Some(next) = grown(atlas, PAGES, LIMITS, BUDGET) {
		assert!(next.0 * next.0 * 4 * next.1 <= BUDGET, "{next:?}");
		(atlas, steps) = (next, steps + 1);
	}
	// 2 pages of 64 square doubled up to the 2048 layers the device makes,
	// well within the budget.
	assert_eq!((atlas, steps), ((64, 2048), 10));
}
