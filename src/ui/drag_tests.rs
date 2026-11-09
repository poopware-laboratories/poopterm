//! When a selection drag goes on, by what the system says is held.
use super::drag::{drag_after_move, drag_goes_on, left_is_down};

#[test]
fn the_left_button_is_bit_zero_of_the_mask() {
	assert!(left_is_down(0b001));
	assert!(left_is_down(0b101));
	assert!(left_is_down(0b111));
	assert!(left_is_down(usize::MAX));
}

#[test]
fn right_and_middle_alone_are_not_the_left_button() {
	assert!(!left_is_down(0));
	assert!(!left_is_down(0b010));
	assert!(!left_is_down(0b100));
	assert!(!left_is_down(0b110));
	assert!(!left_is_down(usize::MAX << 1));
}

#[test]
fn a_drag_goes_on_while_the_left_button_is_held() {
	assert!(drag_goes_on(Some(0b001)));
	assert!(drag_goes_on(Some(0b011)));
}

#[test]
fn a_drag_ends_when_the_system_says_nothing_is_held() {
	// The release was lost (the window moved under the pointer, or another
	// program took it): the pointer moving on must not keep selecting.
	assert!(!drag_goes_on(Some(0)));
	assert!(!drag_goes_on(Some(0b110)));
}

#[test]
fn a_system_that_cannot_say_leaves_the_drag_alone() {
	// Where the release is always delivered, there is nothing to ask.
	assert!(drag_goes_on(None));
}

#[test]
fn every_combination_of_the_three_buttons_is_judged_by_the_left_one() {
	for mask in 0..8usize {
		let (left, right, middle) = (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
		assert_eq!(left_is_down(mask), left, "{left} {right} {middle}");
		assert_eq!(drag_goes_on(Some(mask)), left, "{left} {right} {middle}");
	}
}

#[test]
fn a_drag_that_goes_on_is_handed_back_unchanged() {
	assert_eq!(drag_after_move(Some((3, true)), Some(1)), Some((3, true)));
	assert_eq!(drag_after_move(Some("anchor"), None), Some("anchor"));
}

#[test]
fn a_drag_that_lost_its_release_is_dropped() {
	assert_eq!(drag_after_move(Some((3, true)), Some(0)), None);
	assert_eq!(
		drag_after_move(Some(String::from("anchor")), Some(0b110)),
		None
	);
}

#[test]
fn no_drag_stays_no_drag_whatever_is_held() {
	for held in [None, Some(0), Some(1), Some(7)] {
		assert_eq!(drag_after_move(None::<u8>, held), None, "{held:?}");
	}
}

#[test]
fn a_drag_is_never_started_by_the_buttons() {
	// Only a press starts one; a held button alone must not.
	assert!(drag_after_move(None::<()>, Some(1)).is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_can_say_which_buttons_are_held() {
	assert!(crate::platform::held_mouse_buttons().is_some());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn other_systems_always_deliver_the_release_so_they_are_not_asked() {
	assert_eq!(crate::platform::held_mouse_buttons(), None);
}
