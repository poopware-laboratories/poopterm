//! Tests for split pane layout, hit-testing, focus moves and closing.
use super::panes::*;
use crate::graphics::Rect;

const AREA: Rect = Rect {
	x: 10,
	y: 40,
	width: 805,
	height: 600,
};
const CELL: (usize, usize) = (10, 20);
const DIVIDER: usize = 4;

fn rect(x: usize, y: usize, width: usize, height: usize) -> Rect {
	Rect {
		x,
		y,
		width,
		height,
	}
}

/// Panes of sessions 1, 2, … reached by splitting session 1's pane as
/// `splits` say, each time focusing the slot given first.
fn panes(splits: &[(usize, Split)]) -> Panes {
	let mut panes = Panes::new(1);
	for (n, &(slot, split)) in splits.iter().enumerate() {
		let id = panes.ids()[slot];
		panes.focus(id);
		assert!(panes.split(split, n as u64 + 2), "split {n}");
	}
	panes
}

fn rects(panes: &Panes) -> Vec<Rect> {
	(panes.places(AREA, CELL, DIVIDER).iter())
		.map(|place| place.rect)
		.collect()
}

fn columns() -> Panes {
	panes(&[(0, Split::Right)])
}

fn stacked() -> Panes {
	panes(&[(0, Split::Down)])
}

/// Big pane 1 on the left; 2 above 3 on the right.
fn main_left() -> Panes {
	panes(&[(0, Split::Right), (1, Split::Down)])
}

/// Big pane 2 on the right; 1 above 3 on the left.
fn main_right() -> Panes {
	panes(&[(0, Split::Right), (0, Split::Down)])
}

#[test]
fn one_pane_fills_the_area_with_its_whole_grid() {
	let panes = Panes::new(7);
	assert_eq!(panes.arrangement(), Arrangement::Single);
	assert!(!panes.is_split());
	assert_eq!(panes.focused(), 7);
	let places = panes.places(AREA, CELL, DIVIDER);
	assert_eq!(
		places,
		[Place {
			id: 7,
			rect: AREA,
			grid: (80, 30)
		}]
	);
	assert!(panes.dividers(AREA, CELL, DIVIDER).is_empty());
}

#[test]
fn side_by_side_panes_split_on_a_whole_cell_and_the_right_takes_the_rest() {
	let panes = columns();
	assert_eq!(panes.arrangement(), Arrangement::Columns);
	assert_eq!(panes.ids(), [1, 2]);
	// 801 pixels beside the divider: 400, down to whole cells, go left.
	assert_eq!(
		rects(&panes),
		[rect(10, 40, 400, 600), rect(414, 40, 401, 600)]
	);
	let grids: Vec<_> = (panes.places(AREA, CELL, DIVIDER).iter())
		.map(|place| place.grid)
		.collect();
	assert_eq!(grids, [(40, 30), (40, 30)]);
	assert_eq!(panes.dividers(AREA, CELL, DIVIDER), [rect(410, 40, 4, 600)]);
}

#[test]
fn stacked_panes_split_on_a_whole_row_and_the_bottom_takes_the_rest() {
	let panes = stacked();
	assert_eq!(panes.arrangement(), Arrangement::Rows);
	// 596 pixels beside the divider: 298, down to whole 20 pixel rows.
	assert_eq!(
		rects(&panes),
		[rect(10, 40, 805, 280), rect(10, 324, 805, 316)]
	);
	let grids: Vec<_> = (panes.places(AREA, CELL, DIVIDER).iter())
		.map(|place| place.grid)
		.collect();
	assert_eq!(grids, [(80, 14), (80, 15)]);
	assert_eq!(panes.dividers(AREA, CELL, DIVIDER), [rect(10, 320, 805, 4)]);
}

#[test]
fn a_big_pane_on_the_left_beside_two_stacked() {
	let panes = main_left();
	assert_eq!(panes.arrangement(), Arrangement::Main(Side::Left));
	assert_eq!(panes.ids(), [1, 2, 3]);
	assert_eq!(panes.focused(), 3);
	assert_eq!(
		rects(&panes),
		[
			rect(10, 40, 400, 600),
			rect(414, 40, 401, 280),
			rect(414, 324, 401, 316)
		]
	);
	assert_eq!(
		panes.dividers(AREA, CELL, DIVIDER),
		[rect(410, 40, 4, 600), rect(414, 320, 401, 4)]
	);
}

#[test]
fn a_big_pane_on_the_right_beside_two_stacked() {
	let panes = main_right();
	assert_eq!(panes.arrangement(), Arrangement::Main(Side::Right));
	// The left pane was split: the right one grew big; 1 stays on top.
	assert_eq!(panes.ids(), [2, 1, 3]);
	assert_eq!(panes.focused(), 3);
	assert_eq!(
		rects(&panes),
		[
			rect(414, 40, 401, 600),
			rect(10, 40, 400, 280),
			rect(10, 324, 400, 316)
		]
	);
	assert_eq!(
		panes.dividers(AREA, CELL, DIVIDER),
		[rect(410, 40, 4, 600), rect(10, 320, 400, 4)]
	);
}

#[test]
fn splits_with_no_arrangement_of_that_shape_change_nothing() {
	for (mut panes, split) in [
		(columns(), Split::Right),
		(stacked(), Split::Right),
		(stacked(), Split::Down),
		(main_left(), Split::Right),
		(main_left(), Split::Down),
		(main_right(), Split::Down),
	] {
		let before = panes.clone();
		assert!(!panes.can_split(split), "{before:?} {split:?}");
		assert!(!panes.split(split, 99));
		assert_eq!(panes, before);
	}
}

#[test]
fn can_split_says_what_split_does() {
	for split in [Split::Right, Split::Down] {
		assert!(Panes::new(1).can_split(split));
	}
	let mut panes = columns();
	assert!(panes.can_split(Split::Down));
	panes.focus(1);
	assert!(panes.can_split(Split::Down));
	assert!(!panes.can_split(Split::Right));
}

#[test]
fn a_session_already_shown_cannot_be_split_in_again() {
	let mut panes = columns();
	assert!(!panes.split(Split::Down, 1));
	assert!(!panes.split(Split::Down, 2));
	assert_eq!(panes, columns());
}

#[test]
fn the_new_pane_is_focused() {
	let mut panes = Panes::new(1);
	panes.split(Split::Down, 5);
	assert_eq!(panes.focused(), 5);
	assert!(panes.is_split());
}

#[test]
fn closing_one_of_two_gives_the_other_the_whole_area() {
	for (panes, closed, kept) in [
		(columns(), 1, 2),
		(columns(), 2, 1),
		(stacked(), 1, 2),
		(stacked(), 2, 1),
	] {
		let mut panes = panes;
		assert!(panes.close(closed));
		assert_eq!(panes.arrangement(), Arrangement::Single);
		assert_eq!(panes.ids(), [kept]);
		assert_eq!(panes.focused(), kept);
		assert_eq!(rects(&panes), [AREA]);
	}
}

#[test]
fn the_last_pane_and_strangers_are_not_closed() {
	let mut panes = Panes::new(1);
	assert!(!panes.close(1));
	assert_eq!(panes, Panes::new(1));
	let mut split = columns();
	assert!(!split.close(9));
	assert_eq!(split, columns());
}

#[test]
fn closing_the_big_pane_stacks_the_small_ones_across_the_area() {
	for mut panes in [main_left(), main_right()] {
		let (big, top, bottom) = (panes.ids()[0], panes.ids()[1], panes.ids()[2]);
		panes.focus(big);
		assert!(panes.close(big));
		assert_eq!(panes.arrangement(), Arrangement::Rows);
		assert_eq!(panes.ids(), [top, bottom]);
		// The top pane takes the room above, so it takes focus.
		assert_eq!(panes.focused(), top);
		assert_eq!(
			rects(&panes),
			[rect(10, 40, 805, 280), rect(10, 324, 805, 316)]
		);
	}
}

#[test]
fn closing_a_small_pane_grows_the_other_beside_the_big_one() {
	// Big 1 on the left; 2 above 3.
	let mut panes = main_left();
	assert!(panes.close(3));
	assert_eq!(panes.arrangement(), Arrangement::Columns);
	assert_eq!(panes.ids(), [1, 2]);
	assert_eq!(panes.focused(), 2);
	let mut panes = main_left();
	assert!(panes.close(2));
	assert_eq!(panes.ids(), [1, 3]);
	// Focus stays where it was.
	assert_eq!(panes.focused(), 3);
	assert_eq!(
		rects(&panes),
		[rect(10, 40, 400, 600), rect(414, 40, 401, 600)]
	);
}

#[test]
fn the_big_pane_on_the_right_stays_on_the_right_when_a_small_one_closes() {
	// Big 2 on the right; 1 above 3 on the left.
	let mut panes = main_right();
	assert!(panes.close(1));
	assert_eq!(panes.arrangement(), Arrangement::Columns);
	assert_eq!(panes.ids(), [3, 2]);
	assert_eq!(panes.focused(), 3);
	let mut panes = main_right();
	assert!(panes.close(3));
	assert_eq!(panes.ids(), [1, 2]);
	// The focused pane closed: the one growing into its room takes focus.
	assert_eq!(panes.focused(), 1);
}

#[test]
fn closing_the_pane_just_split_off_restores_the_layout_before() {
	for splits in [
		&[(0, Split::Right)][..],
		&[(0, Split::Down)],
		&[(0, Split::Right), (1, Split::Down)],
		&[(0, Split::Right), (0, Split::Down)],
	] {
		let before = panes(&splits[..splits.len() - 1]);
		let mut after = panes(splits);
		// The pane split keeps the focus after the new one closes.
		let split_from = before.ids()[splits[splits.len() - 1].0];
		let new = *after.ids().last().unwrap();
		assert!(after.close(new));
		assert_eq!(after.arrangement(), before.arrangement(), "{splits:?}");
		assert_eq!(after.ids(), before.ids(), "{splits:?}");
		assert_eq!(after.focused(), split_from, "{splits:?}");
	}
}

#[test]
fn focusing_by_session() {
	let mut panes = main_left();
	assert!(panes.focus(1));
	assert_eq!(panes.focused(), 1);
	assert!(!panes.focus(1), "already focused");
	assert!(!panes.focus(42), "not a pane");
	assert_eq!(panes.focused(), 1);
}

#[test]
fn cycling_focus_wraps_both_ways() {
	let mut panes = main_left();
	assert_eq!(panes.focused(), 3);
	assert!(panes.cycle(1));
	assert_eq!(panes.focused(), 1);
	assert!(panes.cycle(-1));
	assert_eq!(panes.focused(), 3);
	assert!(panes.cycle(-2));
	assert_eq!(panes.focused(), 1);
	assert!(!Panes::new(1).cycle(1), "one pane has nowhere to go");
	assert!(!panes.cycle(3), "a whole turn ends where it began");
}

#[test]
fn focus_moves_to_the_pane_beside_in_each_direction() {
	let mut panes = main_left();
	let mut go = |direction| {
		let moved = panes.focus_toward(direction, AREA, CELL, DIVIDER);
		(moved, panes.focused())
	};
	// From 3, bottom right.
	assert_eq!(go(Direction::Right), (false, 3));
	assert_eq!(go(Direction::Down), (false, 3));
	assert_eq!(go(Direction::Up), (true, 2));
	assert_eq!(go(Direction::Up), (false, 2));
	assert_eq!(go(Direction::Left), (true, 1));
	assert_eq!(go(Direction::Left), (false, 1));
	assert_eq!(go(Direction::Up), (false, 1));
	// Both small panes are as near; the bottom shares more of the edge.
	assert_eq!(go(Direction::Right), (true, 3));
}

#[test]
fn focus_moves_across_a_big_pane_on_the_right() {
	let mut panes = main_right();
	panes.focus(1);
	assert!(panes.focus_toward(Direction::Right, AREA, CELL, DIVIDER));
	assert_eq!(panes.focused(), 2);
	assert!(panes.focus_toward(Direction::Left, AREA, CELL, DIVIDER));
	assert_eq!(panes.focused(), 3, "the bottom shares more of the edge");
	assert!(panes.focus_toward(Direction::Up, AREA, CELL, DIVIDER));
	assert_eq!(panes.focused(), 1);
}

#[test]
fn focus_moves_between_stacked_and_side_by_side_panes() {
	let mut panes = stacked();
	assert!(!panes.focus_toward(Direction::Left, AREA, CELL, DIVIDER));
	assert!(panes.focus_toward(Direction::Up, AREA, CELL, DIVIDER));
	assert_eq!(panes.focused(), 1);
	let mut panes = columns();
	assert!(!panes.focus_toward(Direction::Down, AREA, CELL, DIVIDER));
	assert!(panes.focus_toward(Direction::Left, AREA, CELL, DIVIDER));
	assert_eq!(panes.focused(), 1);
	assert!(!Panes::new(1).focus_toward(Direction::Up, AREA, CELL, DIVIDER));
}

#[test]
fn panes_too_small_to_share_an_edge_are_not_beside() {
	// In a one pixel area the panes have no height to share.
	let tiny = rect(0, 0, 1, 1);
	let mut panes = main_left();
	panes.focus(1);
	assert!(!panes.focus_toward(Direction::Down, tiny, CELL, DIVIDER));
}

#[test]
fn points_hit_the_pane_under_them_but_not_dividers_or_the_outside() {
	let panes = main_left();
	let hit = |x, y| panes.hit(AREA, CELL, DIVIDER, (x, y));
	assert_eq!(hit(10, 40), Some(1));
	assert_eq!(hit(409, 639), Some(1));
	assert_eq!(hit(410, 100), None, "vertical divider");
	assert_eq!(hit(413, 100), None, "vertical divider");
	assert_eq!(hit(414, 40), Some(2));
	assert_eq!(hit(814, 319), Some(2));
	assert_eq!(hit(600, 320), None, "horizontal divider");
	assert_eq!(hit(600, 323), None, "horizontal divider");
	assert_eq!(hit(600, 324), Some(3));
	assert_eq!(hit(814, 639), Some(3));
	assert_eq!(hit(815, 100), None, "right of the area");
	assert_eq!(hit(600, 640), None, "below the area");
	assert_eq!(hit(9, 100), None, "left of the area");
	assert_eq!(hit(100, 39), None, "above the area");
}

#[test]
fn a_single_pane_is_hit_anywhere_in_the_area() {
	let panes = Panes::new(4);
	assert_eq!(panes.hit(AREA, CELL, DIVIDER, (10, 40)), Some(4));
	assert_eq!(panes.hit(AREA, CELL, DIVIDER, (814, 639)), Some(4));
	assert_eq!(panes.hit(AREA, CELL, DIVIDER, (815, 639)), None);
}

/// Every arrangement there is, with sessions 1 to 3.
fn every_arrangement() -> Vec<Panes> {
	vec![
		Panes::new(1),
		columns(),
		stacked(),
		main_left(),
		main_right(),
	]
}

#[test]
fn panes_and_dividers_tile_the_area_exactly_at_any_size() {
	let cell = (3, 5);
	for panes in every_arrangement() {
		for width in [0, 1, 2, 3, 7, 8, 9, 30, 31] {
			for height in [0, 1, 4, 5, 11, 12, 13, 41] {
				let area = rect(2, 3, width, height);
				let mut covered = vec![0u8; width * height];
				let parts = (panes.places(area, cell, 2).into_iter())
					.map(|place| place.rect)
					.chain(panes.dividers(area, cell, 2));
				for part in parts {
					for y in part.y..part.y + part.height {
						for x in part.x..part.x + part.width {
							assert!(x >= 2 && y >= 3, "{part:?} outside {area:?}");
							covered[(y - 3) * width + (x - 2)] += 1;
						}
					}
				}
				let message = format!("{:?} in {area:?}", panes.arrangement());
				assert!(covered.iter().all(|&count| count == 1), "{message}");
			}
		}
	}
}

#[test]
fn dividers_sit_after_whole_cells_from_the_area_start() {
	for panes in every_arrangement() {
		for place in panes.places(AREA, CELL, DIVIDER) {
			// Every pane but the last in a row or column holds whole cells.
			let r = place.rect;
			if r.x + r.width < AREA.x + AREA.width {
				assert_eq!(r.width % CELL.0, 0, "{place:?}");
			}
			if r.y + r.height < AREA.y + AREA.height {
				assert_eq!(r.height % CELL.1, 0, "{place:?}");
			}
			assert_eq!(place.grid, grid(r, CELL));
		}
	}
}

#[test]
fn every_point_in_a_pane_hits_it() {
	for panes in every_arrangement() {
		for place in panes.places(AREA, CELL, DIVIDER) {
			let r = place.rect;
			for (x, y) in [
				(r.x, r.y),
				(r.x + r.width - 1, r.y),
				(r.x, r.y + r.height - 1),
				(r.x + r.width - 1, r.y + r.height - 1),
				(r.x + r.width / 2, r.y + r.height / 2),
			] {
				assert_eq!(panes.hit(AREA, CELL, DIVIDER, (x, y)), Some(place.id));
			}
		}
	}
}

#[test]
fn grids_are_at_least_two_by_two() {
	assert_eq!(grid(rect(0, 0, 0, 0), CELL), (2, 2));
	assert_eq!(grid(rect(0, 0, 25, 45), CELL), (2, 2));
	assert_eq!(grid(rect(0, 0, 39, 79), CELL), (3, 3));
	assert_eq!(grid(rect(0, 0, 40, 80), (0, 0)), (40, 80), "no zero divide");
}

#[test]
fn an_area_too_small_for_a_divider_gives_the_last_pane_all_it_has() {
	let panes = columns();
	let places = panes.places(rect(0, 0, 3, 10), CELL, DIVIDER);
	assert_eq!(places[0].rect, rect(0, 0, 0, 10));
	assert_eq!(places[1].rect, rect(3, 0, 0, 10));
	assert_eq!(
		panes.dividers(rect(0, 0, 3, 10), CELL, DIVIDER),
		[rect(0, 0, 3, 10)]
	);
}

#[test]
fn edge_panes_reach_into_the_window_padding_inner_edges_do_not() {
	let padding = rect(1, 30, 823, 619);
	let rects = rects(&main_left());
	// The big pane touches the left, top and bottom.
	assert_eq!(pane_padding(rects[0], AREA, padding), rect(1, 30, 409, 619));
	// The top small one touches the top and right.
	assert_eq!(
		pane_padding(rects[1], AREA, padding),
		rect(414, 30, 410, 290)
	);
	// The bottom small one touches the bottom and right.
	assert_eq!(
		pane_padding(rects[2], AREA, padding),
		rect(414, 324, 410, 325)
	);
	assert_eq!(pane_padding(AREA, AREA, padding), padding);
}

#[test]
fn dividers_grow_with_the_scale_and_stay_odd() {
	assert_eq!(divider_width(0), 9);
	assert_eq!(divider_width(1), 9);
	assert_eq!(divider_width(2), 18);
	assert_eq!(divider_width(3) % 2, 1);
}

#[test]
fn divider_lines_run_down_the_middle_of_their_gap() {
	assert_eq!(
		divider_line(rect(410, 40, 9, 600), 1),
		rect(414, 40, 1, 600)
	);
	assert_eq!(
		divider_line(rect(410, 40, 18, 600), 2),
		rect(418, 40, 2, 600)
	);
	assert_eq!(
		divider_line(rect(10, 320, 805, 9), 1),
		rect(10, 324, 805, 1)
	);
	// A line never leaves a gap thinner than itself.
	assert_eq!(divider_line(rect(5, 0, 1, 50), 3), rect(5, 0, 1, 50));
	assert_eq!(divider_line(rect(0, 7, 50, 0), 3), rect(0, 7, 50, 0));
	assert_eq!(divider_line(rect(0, 0, 4, 4), 0), rect(0, 1, 4, 1));
}
