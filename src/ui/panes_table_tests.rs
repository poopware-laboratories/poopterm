//! Tables of split pane behavior over every arrangement and focus: where
//! focus moves, what a split and a close leave, and invariants that hold
//! after any sequence of them.
use super::panes::*;
use crate::graphics::Rect;

const AREA: Rect = Rect {
	x: 3,
	y: 7,
	width: 641,
	height: 487,
};
const CELL: (usize, usize) = (8, 17);
const DIVIDER: usize = 9;

/// Side by side: 1 | 2.
fn columns(focus: u64) -> Panes {
	let mut panes = Panes::new(1);
	assert!(panes.split(Split::Right, 2));
	panes.focus(focus);
	panes
}

/// Stacked: 1 over 2.
fn rows(focus: u64) -> Panes {
	let mut panes = Panes::new(1);
	assert!(panes.split(Split::Down, 2));
	panes.focus(focus);
	panes
}

/// Big 1 on the left; 2 over 3 on the right.
fn main_left(focus: u64) -> Panes {
	let mut panes = columns(2);
	assert!(panes.split(Split::Down, 3));
	panes.focus(focus);
	panes
}

/// Big 2 on the right; 1 over 3 on the left.
fn main_right(focus: u64) -> Panes {
	let mut panes = columns(1);
	assert!(panes.split(Split::Down, 3));
	panes.focus(focus);
	panes
}

fn after_move(mut panes: Panes, direction: Direction) -> (bool, u64) {
	let moved = panes.focus_toward(direction, AREA, CELL, DIVIDER);
	(moved, panes.focused())
}

#[test]
fn focus_moves_from_every_pane_in_every_direction() {
	use Direction::{Down, Left, Right, Up};
	// (panes, direction, where focus ends; the same pane when it stays)
	let table = [
		(Panes::new(1), Left, 1),
		(Panes::new(1), Right, 1),
		(Panes::new(1), Up, 1),
		(Panes::new(1), Down, 1),
		(columns(1), Left, 1),
		(columns(1), Right, 2),
		(columns(1), Up, 1),
		(columns(1), Down, 1),
		(columns(2), Left, 1),
		(columns(2), Right, 2),
		(columns(2), Up, 2),
		(columns(2), Down, 2),
		(rows(1), Left, 1),
		(rows(1), Right, 1),
		(rows(1), Up, 1),
		(rows(1), Down, 2),
		(rows(2), Left, 2),
		(rows(2), Right, 2),
		(rows(2), Up, 1),
		(rows(2), Down, 2),
		(main_left(1), Left, 1),
		(main_left(1), Right, 3),
		(main_left(1), Up, 1),
		(main_left(1), Down, 1),
		(main_left(2), Left, 1),
		(main_left(2), Right, 2),
		(main_left(2), Up, 2),
		(main_left(2), Down, 3),
		(main_left(3), Left, 1),
		(main_left(3), Right, 3),
		(main_left(3), Up, 2),
		(main_left(3), Down, 3),
		(main_right(2), Left, 3),
		(main_right(2), Right, 2),
		(main_right(2), Up, 2),
		(main_right(2), Down, 2),
		(main_right(1), Left, 1),
		(main_right(1), Right, 2),
		(main_right(1), Up, 1),
		(main_right(1), Down, 3),
		(main_right(3), Left, 3),
		(main_right(3), Right, 2),
		(main_right(3), Up, 1),
		(main_right(3), Down, 3),
	];
	for (panes, direction, expected) in table {
		let from = panes.focused();
		let what = format!("{:?} from {from} {direction:?}", panes.arrangement());
		assert_eq!(
			after_move(panes, direction),
			(expected != from, expected),
			"{what}"
		);
	}
}

#[test]
fn a_move_lands_on_the_pane_sharing_most_of_the_edge_across_the_divider() {
	let all = [
		columns(1),
		columns(2),
		rows(1),
		rows(2),
		main_left(1),
		main_left(2),
		main_left(3),
		main_right(1),
		main_right(2),
		main_right(3),
	];
	for panes in all {
		let places = panes.places(AREA, CELL, DIVIDER);
		let from = (places.iter())
			.find(|place| place.id == panes.focused())
			.unwrap()
			.rect;
		for direction in [
			Direction::Left,
			Direction::Right,
			Direction::Up,
			Direction::Down,
		] {
			// The pixels just across the divider, along the whole edge.
			let across: Vec<(usize, usize)> = match direction {
				Direction::Left => (from.x.checked_sub(DIVIDER + 1).into_iter())
					.flat_map(|x| (from.y..from.y + from.height).map(move |y| (x, y)))
					.collect(),
				Direction::Right => (from.y..from.y + from.height)
					.map(|y| (from.x + from.width + DIVIDER, y))
					.collect(),
				Direction::Up => (from.y.checked_sub(DIVIDER + 1).into_iter())
					.flat_map(|y| (from.x..from.x + from.width).map(move |x| (x, y)))
					.collect(),
				Direction::Down => (from.x..from.x + from.width)
					.map(|x| (x, from.y + from.height + DIVIDER))
					.collect(),
			};
			let mut shares: Vec<(u64, usize)> = Vec::new();
			for id in across
				.into_iter()
				.filter_map(|p| panes.hit(AREA, CELL, DIVIDER, p))
			{
				match shares.iter_mut().find(|(seen, _)| *seen == id) {
					Some((_, count)) => *count += 1,
					None => shares.push((id, 1)),
				}
			}
			let (moved, to) = after_move(panes.clone(), direction);
			let what = format!(
				"{:?} from {} {direction:?}",
				panes.arrangement(),
				panes.focused()
			);
			assert_eq!(moved, !shares.is_empty(), "{what}");
			if let Some(&(_, most)) = shares.iter().max_by_key(|(_, count)| *count) {
				let reached = shares.iter().find(|(id, _)| *id == to);
				assert_eq!(reached.map(|(_, count)| *count), Some(most), "{what}");
			}
		}
	}
}

/// What a split leaves: the arrangement, the slots, the focused pane.
type Outcome = Option<(Arrangement, Vec<u64>, u64)>;

#[test]
fn every_split_from_every_focus() {
	let both = |left: Outcome, down: Outcome| [(Split::Right, left), (Split::Down, down)];
	let table: [(Panes, [(Split, Outcome); 2]); 10] = [
		(
			Panes::new(1),
			both(
				Some((Arrangement::Columns, vec![1, 9], 9)),
				Some((Arrangement::Rows, vec![1, 9], 9)),
			),
		),
		(
			columns(1),
			both(
				None,
				Some((Arrangement::Main(Side::Right), vec![2, 1, 9], 9)),
			),
		),
		(
			columns(2),
			both(
				None,
				Some((Arrangement::Main(Side::Left), vec![1, 2, 9], 9)),
			),
		),
		(rows(1), both(None, None)),
		(rows(2), both(None, None)),
		(main_left(1), both(None, None)),
		(main_left(2), both(None, None)),
		(main_left(3), both(None, None)),
		(main_right(2), both(None, None)),
		(main_right(3), both(None, None)),
	];
	for (panes, splits) in table {
		for (split, expected) in splits {
			let mut after = panes.clone();
			let done = after.split(split, 9);
			let got = done.then(|| (after.arrangement(), after.ids().to_vec(), after.focused()));
			assert_eq!(got, expected, "{panes:?} {split:?}");
			assert_eq!(panes.can_split(split), done, "{panes:?} {split:?}");
			if !done {
				assert_eq!(after, panes, "a refused split changes nothing");
			}
		}
	}
}

#[test]
fn every_close_from_every_focus() {
	use Arrangement::{Columns, Rows, Single};
	// (panes, closed, arrangement, slots, focused after)
	let table = [
		(columns(1), 1, Single, vec![2], 2),
		(columns(1), 2, Single, vec![1], 1),
		(columns(2), 1, Single, vec![2], 2),
		(columns(2), 2, Single, vec![1], 1),
		(rows(1), 1, Single, vec![2], 2),
		(rows(1), 2, Single, vec![1], 1),
		(rows(2), 1, Single, vec![2], 2),
		(rows(2), 2, Single, vec![1], 1),
		// Big 1 on the left; 2 over 3 on the right.
		(main_left(1), 1, Rows, vec![2, 3], 2),
		(main_left(1), 2, Columns, vec![1, 3], 1),
		(main_left(1), 3, Columns, vec![1, 2], 1),
		(main_left(2), 1, Rows, vec![2, 3], 2),
		(main_left(2), 2, Columns, vec![1, 3], 3),
		(main_left(2), 3, Columns, vec![1, 2], 2),
		(main_left(3), 1, Rows, vec![2, 3], 3),
		(main_left(3), 2, Columns, vec![1, 3], 3),
		(main_left(3), 3, Columns, vec![1, 2], 2),
		// Big 2 on the right; 1 over 3 on the left.
		(main_right(2), 2, Rows, vec![1, 3], 1),
		(main_right(2), 1, Columns, vec![3, 2], 2),
		(main_right(2), 3, Columns, vec![1, 2], 2),
		(main_right(1), 2, Rows, vec![1, 3], 1),
		(main_right(1), 1, Columns, vec![3, 2], 3),
		(main_right(1), 3, Columns, vec![1, 2], 1),
		(main_right(3), 2, Rows, vec![1, 3], 3),
		(main_right(3), 1, Columns, vec![3, 2], 3),
		(main_right(3), 3, Columns, vec![1, 2], 1),
	];
	for (panes, closed, arrangement, slots, focused) in table {
		let what = format!("{panes:?} closing {closed}");
		let mut after = panes;
		assert!(after.close(closed), "{what}");
		assert_eq!(after.arrangement(), arrangement, "{what}");
		assert_eq!(after.ids(), slots, "{what}");
		assert_eq!(after.focused(), focused, "{what}");
	}
}

#[test]
fn a_closed_panes_room_goes_only_to_the_panes_that_touched_it() {
	// Every pane left keeps its own pixels and gains only the closed one's
	// and the divider's: none moves away from where it was.
	for panes in [main_left(1), main_right(2), columns(1), rows(2)] {
		let before = panes.places(AREA, CELL, DIVIDER);
		for closed in panes.ids().to_vec() {
			let mut after = panes.clone();
			after.close(closed);
			for place in after.places(AREA, CELL, DIVIDER) {
				let old = before.iter().find(|old| old.id == place.id).unwrap().rect;
				let new = place.rect;
				assert!(
					new.x <= old.x
						&& new.y <= old.y
						&& new.x + new.width >= old.x + old.width
						&& new.y + new.height >= old.y + old.height,
					"{:?} closing {closed}: {old:?} became {new:?}",
					panes.arrangement()
				);
			}
		}
	}
}

#[test]
fn cycling_visits_every_pane_once_in_slot_order() {
	for panes in [
		Panes::new(1),
		columns(1),
		rows(2),
		main_left(3),
		main_right(2),
	] {
		for offset in [1, -1] {
			let mut cycled = panes.clone();
			let mut seen = vec![cycled.focused()];
			for _ in 1..panes.ids().len() {
				assert!(cycled.cycle(offset));
				seen.push(cycled.focused());
			}
			let ids = panes.ids();
			let start = ids.iter().position(|&id| id == panes.focused()).unwrap();
			let expected: Vec<u64> = (0..ids.len() as isize)
				.map(|step| ids[(start as isize + offset * step).rem_euclid(ids.len() as isize) as usize])
				.collect();
			assert_eq!(seen, expected, "{panes:?} by {offset}");
			cycled.cycle(offset);
			assert_eq!(cycled.focused(), panes.focused(), "a full turn");
		}
	}
}

/// One step a user can take on a tab's panes.
#[derive(Clone, Copy, Debug)]
enum Step {
	Split(Split),
	Close(usize),
	Focus(usize),
	Move(Direction),
	Cycle(isize),
}

const STEPS: [Step; 13] = [
	Step::Split(Split::Right),
	Step::Split(Split::Down),
	Step::Close(0),
	Step::Close(1),
	Step::Close(2),
	Step::Focus(0),
	Step::Focus(2),
	Step::Move(Direction::Left),
	Step::Move(Direction::Right),
	Step::Move(Direction::Up),
	Step::Move(Direction::Down),
	Step::Cycle(1),
	Step::Cycle(-1),
];

/// Takes `step`, the next new session being `next`.
fn take(panes: &mut Panes, step: Step, next: &mut u64) {
	let slot = |slot: usize, panes: &Panes| panes.ids().get(slot).copied();
	match step {
		Step::Split(split) => {
			if panes.split(split, *next) {
				*next += 1;
			}
		}
		Step::Close(at) => {
			if let Some(id) = slot(at, panes) {
				let alone = !panes.is_split();
				assert_eq!(panes.close(id), !alone, "only the last pane stays");
			}
		}
		Step::Focus(at) => {
			if let Some(id) = slot(at, panes) {
				panes.focus(id);
			}
		}
		Step::Move(direction) => {
			panes.focus_toward(direction, AREA, CELL, DIVIDER);
		}
		Step::Cycle(offset) => {
			panes.cycle(offset);
		}
	}
}

/// What must hold of any panes, however they came to be.
fn check(panes: &Panes, steps: &[Step]) {
	let ids = panes.ids();
	let expected_count = match panes.arrangement() {
		Arrangement::Single => 1,
		Arrangement::Columns | Arrangement::Rows => 2,
		Arrangement::Main(_) => 3,
	};
	assert_eq!(ids.len(), expected_count, "{steps:?}");
	assert_eq!(panes.is_split(), ids.len() > 1, "{steps:?}");
	assert!(ids.contains(&panes.focused()), "{steps:?}");
	let mut unique = ids.to_vec();
	unique.sort_unstable();
	unique.dedup();
	assert_eq!(unique.len(), ids.len(), "{steps:?}: a session shown twice");
	let places = panes.places(AREA, CELL, DIVIDER);
	assert_eq!(
		places.iter().map(|place| place.id).collect::<Vec<_>>(),
		ids,
		"{steps:?}"
	);
	assert_eq!(
		panes.dividers(AREA, CELL, DIVIDER).len(),
		ids.len() - 1,
		"{steps:?}"
	);
	let area: usize = (places.iter())
		.map(|place| place.rect.width * place.rect.height)
		.chain((panes.dividers(AREA, CELL, DIVIDER).iter()).map(|gap| gap.width * gap.height))
		.sum();
	assert_eq!(
		area,
		AREA.width * AREA.height,
		"{steps:?}: panes tile the area"
	);
}

#[test]
fn any_three_steps_from_one_pane_leave_a_valid_layout() {
	for first in STEPS {
		for second in STEPS {
			for third in STEPS {
				let steps = [first, second, third];
				let (mut panes, mut next) = (Panes::new(1), 2);
				for (taken, &step) in steps.iter().enumerate() {
					take(&mut panes, step, &mut next);
					check(&panes, &steps[..=taken]);
				}
			}
		}
	}
}

#[test]
fn any_two_steps_from_each_three_pane_layout_leave_a_valid_layout() {
	for start in [main_left(1), main_left(3), main_right(2), main_right(1)] {
		for first in STEPS {
			for second in STEPS {
				let steps = [first, second];
				let (mut panes, mut next) = (start.clone(), 4);
				for (taken, &step) in steps.iter().enumerate() {
					take(&mut panes, step, &mut next);
					check(&panes, &steps[..=taken]);
				}
			}
		}
	}
}

#[test]
fn grids_fill_their_panes_with_whole_cells() {
	for panes in [columns(1), rows(1), main_left(1), main_right(1)] {
		for place in panes.places(AREA, CELL, DIVIDER) {
			let (cols, rows) = place.grid;
			assert!(cols * CELL.0 <= place.rect.width, "{place:?}");
			assert!(rows * CELL.1 <= place.rect.height, "{place:?}");
			assert!((cols + 1) * CELL.0 > place.rect.width, "{place:?}");
			assert!((rows + 1) * CELL.1 > place.rect.height, "{place:?}");
		}
	}
}
