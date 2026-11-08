//! Tests for a tab's pane sessions: the focused session always matches the
//! layout's focus, splits and closes hand sessions in and out, and none is
//! lost or shown twice.
use super::{
	pane_sessions::{Closed, Shown, TabPanes},
	panes::{Direction, Split},
};
use crate::graphics::Rect;

/// A stand-in for a shell session: its id and a name to tell it apart.
#[derive(Debug, PartialEq, Eq)]
struct Shell(u64, &'static str);

impl Shown for Shell {
	fn id(&self) -> u64 {
		self.0
	}
}

const AREA: Rect = Rect {
	x: 0,
	y: 30,
	width: 800,
	height: 600,
};
const GEOMETRY: (Rect, (usize, usize), usize) = (AREA, (10, 20), 9);

/// A tab showing shell 1 split right into shell 2, focused on 2.
fn two() -> (TabPanes<Shell>, Shell) {
	let (mut tab, mut focused) = (TabPanes::new(1), Shell(1, "one"));
	assert!(
		tab
			.split(&mut focused, Split::Right, Shell(2, "two"))
			.is_ok()
	);
	(tab, focused)
}

/// [`two`], with 2 split down into 3: big 1 left, 2 over 3 right.
fn three() -> (TabPanes<Shell>, Shell) {
	let (mut tab, mut focused) = two();
	assert!(
		tab
			.split(&mut focused, Split::Down, Shell(3, "three"))
			.is_ok()
	);
	(tab, focused)
}

/// The focused session is the layout's, and every pane's session is held
/// once: the focused one outside, the rest inside.
fn assert_consistent(tab: &TabPanes<Shell>, focused: &Shell) {
	assert_eq!(focused.id(), tab.layout().focused());
	let mut held: Vec<u64> = std::iter::once(focused.0)
		.chain(tab.others().iter().map(|shell| shell.0))
		.collect();
	held.sort_unstable();
	let mut shown = tab.layout().ids().to_vec();
	shown.sort_unstable();
	assert_eq!(held, shown);
}

#[test]
fn a_new_tab_holds_no_other_sessions() {
	let tab: TabPanes<Shell> = TabPanes::new(5);
	let focused = Shell(5, "five");
	assert!(tab.others().is_empty());
	assert!(!tab.layout().is_split());
	assert_consistent(&tab, &focused);
	assert_eq!(tab.find(&focused, 5), Some(&focused));
	assert_eq!(tab.find(&focused, 6), None);
}

#[test]
fn a_split_focuses_the_new_session_and_keeps_the_old_one() {
	let (tab, focused) = two();
	assert_eq!(focused, Shell(2, "two"));
	assert_eq!(tab.others(), [Shell(1, "one")]);
	assert_consistent(&tab, &focused);
	assert_eq!(tab.find(&focused, 1), Some(&Shell(1, "one")));
	assert_eq!(tab.find(&focused, 2), Some(&Shell(2, "two")));
}

#[test]
fn a_refused_split_hands_the_new_session_back_and_changes_nothing() {
	let (mut tab, mut focused) = two();
	let refused = tab.split(&mut focused, Split::Right, Shell(9, "nine"));
	assert_eq!(refused, Err(Shell(9, "nine")));
	assert_eq!(focused, Shell(2, "two"));
	assert_eq!(tab.others(), [Shell(1, "one")]);
	let shown_twice = tab.split(&mut focused, Split::Down, Shell(1, "again"));
	assert_eq!(shown_twice, Err(Shell(1, "again")));
	assert_consistent(&tab, &focused);
}

#[test]
fn focusing_swaps_the_sessions() {
	let (mut tab, mut focused) = three();
	assert!(tab.focus(&mut focused, 1));
	assert_eq!(focused, Shell(1, "one"));
	assert_consistent(&tab, &focused);
	assert!(!tab.focus(&mut focused, 1), "already focused");
	assert!(!tab.focus(&mut focused, 7), "not a pane");
	assert_eq!(focused, Shell(1, "one"));
	assert_consistent(&tab, &focused);
}

#[test]
fn focus_moves_by_direction_and_cycle() {
	let (mut tab, mut focused) = three();
	let (area, cell, divider) = GEOMETRY;
	let left =
		|panes: &mut super::panes::Panes| panes.focus_toward(Direction::Left, area, cell, divider);
	assert!(tab.focus_by(&mut focused, left));
	assert_eq!(focused.0, 1);
	assert!(!tab.focus_by(&mut focused, left), "nothing further left");
	assert_eq!(focused.0, 1);
	assert!(tab.focus_by(&mut focused, |panes| panes.cycle(-1)));
	assert_eq!(focused.0, 3);
	assert_consistent(&tab, &focused);
}

#[test]
fn a_refused_step_leaves_the_layout_alone() {
	let (mut tab, mut focused) = three();
	let before = tab.layout().clone();
	// A step that changes the layout's copy but says it did not move.
	assert!(!tab.focus_by(&mut focused, |panes| {
		panes.cycle(1);
		false
	}));
	assert_eq!(tab.layout(), &before);
	assert_eq!(focused.0, 3);
}

#[test]
fn closing_a_focused_pane_hands_focus_to_its_heir_and_its_session_back() {
	let (mut tab, mut focused) = three();
	// 3, below 2, closes: 2 grows into its room.
	assert_eq!(tab.close(&mut focused, 3), Closed::Pane(Shell(3, "three")));
	assert_eq!(focused, Shell(2, "two"));
	assert_consistent(&tab, &focused);
	assert_eq!(tab.close(&mut focused, 2), Closed::Pane(Shell(2, "two")));
	assert_eq!(focused, Shell(1, "one"));
	assert!(tab.others().is_empty());
	assert_eq!(tab.close(&mut focused, 1), Closed::Last);
	assert_eq!(
		focused,
		Shell(1, "one"),
		"the last stays until the tab closes"
	);
}

#[test]
fn closing_another_pane_keeps_the_focus() {
	let (mut tab, mut focused) = three();
	assert_eq!(tab.close(&mut focused, 1), Closed::Pane(Shell(1, "one")));
	assert_eq!(focused, Shell(3, "three"));
	assert_eq!(tab.others(), [Shell(2, "two")]);
	assert_consistent(&tab, &focused);
}

#[test]
fn closing_a_stranger_does_nothing() {
	let (mut tab, mut focused) = two();
	assert_eq!(tab.close(&mut focused, 42), Closed::Missing);
	assert_eq!(focused, Shell(2, "two"));
	assert_eq!(tab.others(), [Shell(1, "one")]);
}

#[test]
fn every_close_order_ends_with_one_session_and_loses_none() {
	for order in [
		[1, 2, 3],
		[1, 3, 2],
		[2, 1, 3],
		[2, 3, 1],
		[3, 1, 2],
		[3, 2, 1],
	] {
		for start in [1, 2, 3] {
			let (mut tab, mut focused) = three();
			tab.focus(&mut focused, start);
			let mut stopped = Vec::new();
			for id in order {
				match tab.close(&mut focused, id) {
					Closed::Pane(shell) => stopped.push(shell.0),
					Closed::Last => assert_eq!(focused.0, id, "{order:?}"),
					Closed::Missing => panic!("{id} went missing in {order:?}"),
				}
				assert_consistent(&tab, &focused);
			}
			assert_eq!(stopped, order[..2], "{order:?} from {start}");
			assert_eq!(focused.0, order[2]);
			assert!(tab.into_others().is_empty());
		}
	}
}

#[test]
fn a_split_place_is_where_the_split_puts_the_new_pane() {
	let (mut tab, mut focused) = two();
	let place = tab.split_place(Split::Down, 9, GEOMETRY).unwrap();
	assert_eq!(place.id, 9);
	assert!(
		tab
			.split(&mut focused, Split::Down, Shell(9, "nine"))
			.is_ok()
	);
	let (area, cell, divider) = GEOMETRY;
	let placed = tab.layout().places(area, cell, divider);
	assert_eq!(placed.iter().find(|p| p.id == 9), Some(&place));
	assert_eq!(tab.split_place(Split::Down, 10, GEOMETRY), None);
	assert_eq!(tab.split_place(Split::Right, 10, GEOMETRY), None);
}

#[test]
fn a_split_place_leaves_the_tab_alone() {
	let (tab, _) = two();
	let before = tab.layout().clone();
	assert!(tab.split_place(Split::Down, 9, GEOMETRY).is_some());
	assert_eq!(tab.layout(), &before);
}

#[test]
fn presses_find_other_panes_only() {
	let (tab, _) = three();
	// Big 1 on the left, 2 over 3 on the right; 3 is focused.
	assert_eq!(tab.other_at(GEOMETRY, (5, 100)), Some(1));
	assert_eq!(tab.other_at(GEOMETRY, (600, 40)), Some(2));
	assert_eq!(tab.other_at(GEOMETRY, (600, 600)), None, "focused");
	assert_eq!(tab.other_at(GEOMETRY, (5, 10)), None, "above the area");
	let single: TabPanes<Shell> = TabPanes::new(1);
	assert_eq!(single.other_at(GEOMETRY, (5, 100)), None);
}

#[test]
fn a_closed_tab_hands_back_every_other_session() {
	let (tab, focused) = three();
	let mut ids: Vec<u64> = tab.into_others().into_iter().map(|shell| shell.0).collect();
	ids.push(focused.0);
	ids.sort_unstable();
	assert_eq!(ids, [1, 2, 3]);
}

/// A small deterministic generator, so the walk is the same every run.
struct Steps(u64);

impl Steps {
	fn next(&mut self, below: u64) -> u64 {
		self.0 = self
			.0
			.wrapping_mul(6364136223846793005)
			.wrapping_add(1442695040888963407);
		(self.0 >> 33) % below
	}
}

#[test]
fn a_long_walk_of_splits_focus_moves_and_closes_never_loses_a_session() {
	let names = ["a", "b", "c", "d"];
	let (area, cell, divider) = GEOMETRY;
	let mut steps = Steps(7);
	let (mut tab, mut focused) = (TabPanes::new(1), Shell(1, "a"));
	let (mut next, mut stopped) = (2u64, Vec::new());
	for step in 0..2000 {
		let name = names[(next % 4) as usize];
		match steps.next(6) {
			0 | 1 => {
				let split = if steps.next(2) == 0 {
					Split::Right
				} else {
					Split::Down
				};
				let expected = tab.split_place(split, next, GEOMETRY);
				match tab.split(&mut focused, split, Shell(next, name)) {
					Ok(()) => {
						let placed = tab.layout().places(area, cell, divider);
						let place = placed.iter().find(|place| place.id == next);
						assert_eq!(place, expected.as_ref(), "step {step}");
						assert_eq!(focused.0, next, "step {step}: the new pane is focused");
						next += 1;
					}
					Err(shell) => {
						assert_eq!(shell.0, next, "step {step}");
						assert_eq!(expected, None, "step {step}: placed but refused");
					}
				}
			}
			2 => {
				let ids = tab.layout().ids().to_vec();
				let id = ids[steps.next(ids.len() as u64) as usize];
				match tab.close(&mut focused, id) {
					Closed::Pane(shell) => stopped.push(shell.0),
					Closed::Last => assert_eq!(ids, [id], "step {step}"),
					Closed::Missing => panic!("step {step}: {id} is a pane"),
				}
			}
			3 => {
				let direction = [
					Direction::Left,
					Direction::Right,
					Direction::Up,
					Direction::Down,
				][steps.next(4) as usize];
				let before = focused.0;
				let moved = tab.focus_by(&mut focused, |panes| {
					panes.focus_toward(direction, area, cell, divider)
				});
				assert_eq!(moved, focused.0 != before, "step {step}");
			}
			4 => {
				let before = focused.0;
				let moved = tab.focus_by(&mut focused, |panes| panes.cycle(1));
				assert_eq!(moved, tab.layout().is_split(), "step {step}");
				assert_eq!(moved, focused.0 != before, "step {step}");
			}
			_ => {
				let ids = tab.layout().ids().to_vec();
				let id = ids[steps.next(ids.len() as u64) as usize];
				let moved = tab.focus(&mut focused, id);
				assert_eq!(focused.0, id, "step {step}");
				assert_eq!(moved, tab.layout().ids().len() > 1 && moved, "step {step}");
			}
		}
		assert_consistent(&tab, &focused);
	}
	// Every session started is shown, or was handed back to be stopped.
	let mut all: Vec<u64> = stopped;
	all.extend(tab.layout().ids());
	all.sort_unstable();
	assert_eq!(all, (1..next).collect::<Vec<_>>());
}

#[test]
fn a_split_tab_keeps_each_session_with_its_own_name() {
	let (mut tab, mut focused) = three();
	for id in [1, 2, 3, 2, 1] {
		tab.focus(&mut focused, id);
		let name = ["one", "two", "three"][id as usize - 1];
		assert_eq!(focused, Shell(id, name));
		for other in tab.others() {
			assert_eq!(tab.find(&focused, other.0), Some(other));
		}
	}
}
