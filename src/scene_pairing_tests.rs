//! Windows and the scene they show in, whichever arrives first.
use super::scene_pairing::Pairing;

#[test]
fn a_window_made_before_the_scene_waits_for_it() {
	let mut pairing = Pairing::default();
	assert_eq!(pairing.window("terminal"), []);
	assert_eq!(pairing.scene(1), [("terminal", 1)]);
	assert_eq!(pairing.scene(1), [], "nothing is put in twice");
}

#[test]
fn a_window_made_once_the_scene_is_there_goes_in_at_once() {
	let mut pairing = Pairing::default();
	assert_eq!(pairing.scene(7), []);
	assert_eq!(pairing.window("settings"), [("settings", 7)]);
	assert_eq!(pairing.window("another"), [("another", 7)]);
}

#[test]
fn every_waiting_window_goes_in_in_the_order_made() {
	let mut pairing = Pairing::default();
	pairing.window("terminal");
	pairing.window("settings");
	assert_eq!(pairing.scene(2), [("terminal", 2), ("settings", 2)]);
}

#[test]
fn after_the_scene_goes_away_new_windows_wait_for_the_next_one() {
	let mut pairing = Pairing::default();
	pairing.scene(1);
	pairing.disconnect();
	assert_eq!(pairing.window("settings"), []);
	assert_eq!(pairing.scene(3), [("settings", 3)]);
}

#[test]
fn a_window_already_shown_is_not_moved_when_another_scene_connects() {
	let mut pairing = Pairing::default();
	pairing.window("terminal");
	pairing.scene(1);
	pairing.disconnect();
	assert_eq!(pairing.scene(2), [], "it lives in the scene it was put in");
}
