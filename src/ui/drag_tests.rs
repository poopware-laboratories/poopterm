//! When a selection drag goes on, by what the system says is held.
use super::drag::{drag_goes_on, left_is_down};

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
