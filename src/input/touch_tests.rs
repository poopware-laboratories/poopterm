//! Taps and drags: where a tap clicks, when a touch becomes a scroll, and
//! the fingers and cancellations that do nothing.
use super::*;

#[test]
fn a_tap_clicks_where_the_finger_went_down() {
	let mut touches = Touches::default();
	assert_eq!(touches.event(Phase::Started, 1, (100.0, 200.0)), None);
	assert_eq!(
		touches.event(Phase::Ended, 1, (103.0, 204.0)),
		Some(Action::Click(100.0, 200.0))
	);
}

#[test]
fn a_wobble_within_the_slop_is_still_a_tap() {
	let mut touches = Touches::default();
	touches.event(Phase::Started, 1, (10.0, 10.0));
	assert_eq!(touches.event(Phase::Moved, 1, (10.0 + SLOP, 10.0)), None);
	assert_eq!(
		touches.event(Phase::Ended, 1, (10.0, 10.0)),
		Some(Action::Click(10.0, 10.0))
	);
}

#[test]
fn a_drag_scrolls_by_each_move_and_never_clicks() {
	let mut touches = Touches::default();
	touches.event(Phase::Started, 7, (50.0, 100.0));
	assert_eq!(
		touches.event(Phase::Moved, 7, (50.0, 130.0)),
		Some(Action::Scroll(30.0))
	);
	assert_eq!(
		touches.event(Phase::Moved, 7, (50.0, 120.0)),
		Some(Action::Scroll(-10.0))
	);
	assert_eq!(
		touches.event(Phase::Moved, 7, (80.0, 120.0)),
		None,
		"sideways scrolls nothing"
	);
	assert_eq!(touches.event(Phase::Ended, 7, (80.0, 120.0)), None);
}

#[test]
fn once_scrolling_small_moves_scroll_too() {
	let mut touches = Touches::default();
	touches.event(Phase::Started, 1, (0.0, 0.0));
	touches.event(Phase::Moved, 1, (0.0, 40.0));
	assert_eq!(
		touches.event(Phase::Moved, 1, (0.0, 41.0)),
		Some(Action::Scroll(1.0))
	);
}

#[test]
fn other_fingers_are_ignored_while_one_is_down() {
	let mut touches = Touches::default();
	touches.event(Phase::Started, 1, (0.0, 0.0));
	assert_eq!(touches.event(Phase::Started, 2, (90.0, 90.0)), None);
	assert_eq!(touches.event(Phase::Moved, 2, (90.0, 300.0)), None);
	assert_eq!(touches.event(Phase::Ended, 2, (90.0, 300.0)), None);
	assert_eq!(
		touches.event(Phase::Ended, 1, (0.0, 0.0)),
		Some(Action::Click(0.0, 0.0))
	);
}

#[test]
fn a_cancelled_touch_does_nothing_and_frees_the_next() {
	let mut touches = Touches::default();
	touches.event(Phase::Started, 1, (0.0, 0.0));
	assert_eq!(touches.event(Phase::Cancelled, 1, (0.0, 0.0)), None);
	assert_eq!(touches.event(Phase::Ended, 1, (0.0, 0.0)), None);
	touches.event(Phase::Started, 2, (5.0, 6.0));
	assert_eq!(
		touches.event(Phase::Ended, 2, (5.0, 6.0)),
		Some(Action::Click(5.0, 6.0))
	);
}

#[test]
fn an_end_without_a_start_does_nothing() {
	let mut touches = Touches::default();
	assert_eq!(touches.event(Phase::Ended, 3, (1.0, 1.0)), None);
	assert_eq!(touches.event(Phase::Moved, 3, (1.0, 90.0)), None);
}

#[test]
fn every_winit_phase_maps_to_ours() {
	use winit::event::TouchPhase;
	assert_eq!(phase(TouchPhase::Started), Phase::Started);
	assert_eq!(phase(TouchPhase::Moved), Phase::Moved);
	assert_eq!(phase(TouchPhase::Ended), Phase::Ended);
	assert_eq!(phase(TouchPhase::Cancelled), Phase::Cancelled);
}
