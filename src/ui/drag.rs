//! Telling a selection drag that lost its release from one still going on.

/// Whether the left button is in `mask`, the system's set of held buttons
/// (bit 0 is the left button, 1 the right, 2 the middle).
pub(crate) fn left_is_down(mask: usize) -> bool {
	mask & 1 != 0
}

/// Whether a drag goes on, given the held buttons the system reports, or
/// `None` where it cannot say. A release can be lost when the window moves
/// under the pointer or another program takes it; without this the pointer
/// moving on would keep selecting.
pub(crate) fn drag_goes_on(held: Option<usize>) -> bool {
	held.is_none_or(left_is_down)
}

/// The drag after the pointer moved: kept while it goes on, `None` once the
/// system says its release was lost.
pub(crate) fn drag_after_move<T>(drag: Option<T>, held: Option<usize>) -> Option<T> {
	drag.filter(|_| drag_goes_on(held))
}
