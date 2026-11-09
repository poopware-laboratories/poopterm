//! Keyboard focus in the Settings window: which rectangles can be focused
//! and in what order, how Tab steps through them, and the ring around the
//! focused one.
use crate::render::scene::{Rect, Scene};

/// The middle of `rect`: where Enter clicks for the control it holds.
pub(super) fn center((x, y, w, h): Rect) -> (usize, usize) {
	(x + w / 2, y + h / 2)
}

/// The rectangles of `scene` the keyboard can focus, in Tab order: those
/// inside the sidebar, which is `nav` pixels wide, in the order drawn, then
/// the page's top to bottom and left to right. A rectangle whose center
/// `answers` says no click lands on is left out, and one drawn twice is
/// listed once.
pub(super) fn focus_order(
	scene: &Scene,
	nav: usize,
	mut answers: impl FnMut((usize, usize)) -> bool,
) -> Vec<Rect> {
	let mut seen: Vec<Rect> = Vec::new();
	for rect in scene.focus_rects() {
		if !seen.contains(&rect) && answers(center(rect)) {
			seen.push(rect);
		}
	}
	let (mut sidebar, mut page): (Vec<Rect>, Vec<Rect>) =
		seen.into_iter().partition(|rect| rect.0 < nav);
	page.sort_by_key(|&(x, y, ..)| (y, x));
	sidebar.append(&mut page);
	sidebar
}

/// The control focused after pressing Tab (`delta` 1) or Shift+Tab (-1)
/// among `count`, wrapping at both ends. With none focused, Tab goes to the
/// first and Shift+Tab to the last; a focus past the controls counts as
/// none.
pub(super) fn step_focus(current: Option<usize>, count: usize, delta: isize) -> Option<usize> {
	if count == 0 {
		return None;
	}
	match current.filter(|&at| at < count) {
		Some(at) => Some((at as isize + delta).rem_euclid(count as isize) as usize),
		None if delta >= 0 => Some(0),
		None => Some(count - 1),
	}
}

/// The ring around `rect`: top, bottom, left and right strokes, two pixels
/// thick at the base scale, just outside it.
pub(super) fn ring((x, y, w, h): Rect, scale: usize) -> [Rect; 4] {
	let t = 2 * scale;
	let (left, top) = (x.saturating_sub(t), y.saturating_sub(t));
	let (width, height) = (w + 2 * t, h + 2 * t);
	[
		(left, top, width, t),
		(left, y + h, width, t),
		(left, top, t, height),
		(x + w, top, t, height),
	]
}

/// Draws the ring around `rect` in `color`.
pub(super) fn draw_ring(p: &mut Scene, rect: Rect, scale: usize, color: u32) {
	for (x, y, w, h) in ring(rect, scale) {
		p.rect(x, y, w, h, color);
	}
}
