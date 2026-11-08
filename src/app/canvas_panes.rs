//! Planning a split tab's frames: each pane's grid is compared with what
//! its pixels show, so only the rows that changed in it are drawn again.
use super::{Canvas, DrawKey, Plan, damage};
use crate::{term::Frame, ui::panes::Place};

/// What the pixels show of a split tab: its panes' places, and each pane's
/// grid and drawing inputs, in slot order.
pub(in crate::app) struct ShownPanes {
	places: Vec<Place>,
	frames: Vec<Frame>,
	keys: Vec<DrawKey>,
}

/// The drawing inputs of each pane at `places`, the focused one's
/// `focused`: the others show no selection or search match, and a hollow
/// cursor.
pub(in crate::app) fn pane_keys(places: &[Place], focused: DrawKey) -> Vec<DrawKey> {
	(places.iter())
		.map(|place| match place.id == focused.session {
			true => focused,
			false => DrawKey {
				session: place.id,
				focused: false,
				selection: None,
				search_match: None,
				overlay: focused.overlay,
			},
		})
		.collect()
}

impl Canvas {
	/// Decides how to draw the panes at `places`, whose grids are `frames`
	/// drawn with `keys`, in a `size` window: in part only when the pixels
	/// show the same panes in the same places, and every pane allows it.
	/// Panes do not shift pixels to scroll.
	pub(in crate::app) fn plan_panes(
		&mut self,
		frames: &[Frame],
		keys: &[DrawKey],
		places: &[Place],
		size: (usize, usize),
	) -> Plan {
		self.fit(size);
		// The pixels no longer show a single grid.
		self.shown = None;
		let Some(shown) = self.panes.as_ref().filter(|shown| shown.places == places) else {
			return Plan::Full;
		};
		let old = shown.frames.iter().zip(shown.keys.iter().copied());
		let new = frames.iter().zip(keys.iter().copied());
		let rows: Option<Vec<Vec<bool>>> = (old.zip(new))
			.map(|(old, new)| damage(old, new, false).map(|damage| damage.rows))
			.collect();
		match rows {
			Some(rows) if rows.len() == places.len() => Plan::Panes(rows),
			_ => Plan::Full,
		}
	}

	/// Records that the pixels now show `frames` drawn with `keys` at
	/// `places`; hands back the previous frames' buffers for reuse.
	pub(in crate::app) fn remember_panes(
		&mut self,
		frames: &mut Vec<Frame>,
		keys: Vec<DrawKey>,
		places: Vec<Place>,
	) {
		self.shown = None;
		match &mut self.panes {
			Some(shown) => {
				std::mem::swap(&mut shown.frames, frames);
				(shown.keys, shown.places) = (keys, places);
			}
			None => {
				let frames = std::mem::take(frames);
				self.panes = Some(ShownPanes {
					places,
					frames,
					keys,
				});
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		graphics::Rect,
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
		ui::selection::{CellPoint, Selection},
	};

	const SIZE: (usize, usize) = (100, 60);

	fn frame(input: &str) -> Frame {
		frame_of(4, input)
	}

	fn frame_of(cols: usize, input: &str) -> Frame {
		let mut term = Term::new(cols, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		frame
	}

	fn place(id: u64, x: usize) -> Place {
		Place {
			id,
			rect: Rect {
				x,
				y: 0,
				width: 40,
				height: 60,
			},
			grid: (4, 3),
		}
	}

	fn places() -> Vec<Place> {
		vec![place(1, 0), place(2, 50)]
	}

	const FOCUSED: DrawKey = DrawKey {
		session: 2,
		focused: true,
		selection: None,
		search_match: None,
		overlay: false,
	};

	fn selected() -> DrawKey {
		DrawKey {
			selection: Some(Selection {
				start: CellPoint { row: 1, col: 0 },
				end: CellPoint { row: 1, col: 2 },
				block: false,
			}),
			..FOCUSED
		}
	}

	/// A canvas showing `frames` in [`places`], focused on pane 2.
	fn showing(frames: &[Frame]) -> Canvas {
		let mut canvas = Canvas::new();
		let keys = pane_keys(&places(), FOCUSED);
		assert_eq!(
			canvas.plan_panes(frames, &keys, &places(), SIZE),
			Plan::Full
		);
		canvas.remember_panes(&mut frames.to_vec(), keys, places());
		canvas
	}

	#[test]
	fn only_the_focused_pane_keeps_its_highlights_and_focus() {
		let keys = pane_keys(&places(), selected());
		assert_eq!(keys[1], selected());
		assert_eq!(
			keys[0],
			DrawKey {
				session: 1,
				focused: false,
				selection: None,
				search_match: None,
				overlay: false,
			}
		);
		let overlaid = pane_keys(
			&places(),
			DrawKey {
				overlay: true,
				..FOCUSED
			},
		);
		assert!(overlaid.iter().all(|key| key.overlay));
	}

	#[test]
	fn unchanged_panes_need_nothing_drawn_and_changed_ones_their_rows() {
		let mut canvas = showing(&[frame("a"), frame("b")]);
		let keys = pane_keys(&places(), FOCUSED);
		let plan = canvas.plan_panes(&[frame("a"), frame("b")], &keys, &places(), SIZE);
		let none = vec![false; 3];
		assert_eq!(plan, Plan::Panes(vec![none.clone(), none.clone()]));
		// Text on the second row of the first pane: that row, and the
		// cursor's old and new rows.
		let plan = canvas.plan_panes(&[frame("a\r\nx"), frame("b")], &keys, &places(), SIZE);
		assert_eq!(
			plan,
			Plan::Panes(vec![vec![true, true, false], none.clone()])
		);
	}

	#[test]
	fn panes_scrolling_draw_their_rows_again_instead_of_shifting_pixels() {
		let mut canvas = showing(&[frame("1\r\n2\r\n3"), frame("b")]);
		let keys = pane_keys(&places(), FOCUSED);
		let scrolled = frame("1\r\n2\r\n3\r\n4");
		let plan = canvas.plan_panes(&[scrolled, frame("b")], &keys, &places(), SIZE);
		assert_eq!(
			plan,
			Plan::Panes(vec![vec![true, true, true], vec![false; 3]])
		);
	}

	#[test]
	fn a_selection_in_the_focused_pane_damages_its_rows_only() {
		let frames = [frame("a"), frame("b")];
		let mut canvas = showing(&frames);
		let keys = pane_keys(&places(), selected());
		let plan = canvas.plan_panes(&frames, &keys, &places(), SIZE);
		assert_eq!(
			plan,
			Plan::Panes(vec![vec![false; 3], vec![false, true, false]])
		);
	}

	#[test]
	fn moved_panes_other_focus_overlays_and_sizes_draw_everything() {
		let frames = [frame("a"), frame("b")];
		let keys = pane_keys(&places(), FOCUSED);
		let moved = vec![place(1, 0), place(2, 52)];
		assert_eq!(
			showing(&frames).plan_panes(&frames, &keys, &moved, SIZE),
			Plan::Full
		);
		let other = vec![place(1, 0), place(3, 50)];
		assert_eq!(
			showing(&frames).plan_panes(&frames, &keys, &other, SIZE),
			Plan::Full
		);
		let refocused = pane_keys(
			&places(),
			DrawKey {
				session: 1,
				..FOCUSED
			},
		);
		assert_eq!(
			showing(&frames).plan_panes(&frames, &refocused, &places(), SIZE),
			Plan::Full
		);
		let overlay = pane_keys(
			&places(),
			DrawKey {
				overlay: true,
				..FOCUSED
			},
		);
		assert_eq!(
			showing(&frames).plan_panes(&frames, &overlay, &places(), SIZE),
			Plan::Full
		);
		assert_eq!(
			showing(&frames).plan_panes(&frames, &keys, &places(), (101, 60)),
			Plan::Full
		);
		// A pane's terminal resized.
		let wider = [frame("a"), frame_of(5, "b")];
		assert_eq!(
			showing(&frames).plan_panes(&wider, &keys, &places(), SIZE),
			Plan::Full
		);
	}

	#[test]
	fn a_resized_window_forgets_the_panes_and_its_pixels() {
		let frames = [frame("a"), frame("b")];
		let keys = pane_keys(&places(), FOCUSED);
		let mut canvas = showing(&frames);
		canvas.pixels.fill(7);
		assert_eq!(
			canvas.plan_panes(&frames, &keys, &places(), (120, 60)),
			Plan::Full
		);
		assert_eq!(canvas.pixels.len(), 120 * 60);
		assert!(
			canvas.pixels.iter().all(|&pixel| pixel == 0),
			"a fresh buffer"
		);
		canvas.remember_panes(&mut frames.to_vec(), keys.clone(), places());
		let plan = canvas.plan_panes(&frames, &keys, &places(), (120, 60));
		assert_eq!(plan, Plan::Panes(vec![vec![false; 3]; 2]));
	}

	#[test]
	fn an_overlay_closing_draws_a_whole_frame() {
		let frames = [frame("a"), frame("b")];
		let mut canvas = showing(&frames);
		let overlay = pane_keys(
			&places(),
			DrawKey {
				overlay: true,
				..FOCUSED
			},
		);
		assert_eq!(
			canvas.plan_panes(&frames, &overlay, &places(), SIZE),
			Plan::Full
		);
		canvas.remember_panes(&mut frames.to_vec(), overlay, places());
		let keys = pane_keys(&places(), FOCUSED);
		assert_eq!(
			canvas.plan_panes(&frames, &keys, &places(), SIZE),
			Plan::Full,
			"what the overlay covered must be drawn again"
		);
	}

	#[test]
	fn a_single_grid_between_split_frames_forgets_the_panes() {
		let frames = [frame("a"), frame("b")];
		let keys = pane_keys(&places(), FOCUSED);
		let mut canvas = showing(&frames);
		let mut single = frame("a");
		assert_eq!(canvas.plan(&single, FOCUSED, SIZE, true), Plan::Full);
		canvas.remember(&mut single, FOCUSED);
		assert_eq!(
			canvas.plan_panes(&frames, &keys, &places(), SIZE),
			Plan::Full
		);
		// And the panes forget the single grid.
		canvas.remember_panes(&mut frames.to_vec(), keys.clone(), places());
		assert_eq!(canvas.plan(&frame("a"), FOCUSED, SIZE, true), Plan::Full);
		let mut canvas = showing(&frames);
		canvas.invalidate();
		assert_eq!(
			canvas.plan_panes(&frames, &keys, &places(), SIZE),
			Plan::Full
		);
	}

	#[test]
	fn remembering_hands_back_the_previous_frames() {
		let mut canvas = showing(&[frame("a"), frame("b")]);
		let keys = pane_keys(&places(), FOCUSED);
		let mut next = vec![frame("c"), frame("d")];
		canvas.remember_panes(&mut next, keys.clone(), places());
		assert_eq!(next.len(), 2, "the old frames, to snapshot into");
		let plan = canvas.plan_panes(&[frame("c"), frame("d")], &keys, &places(), SIZE);
		assert_eq!(plan, Plan::Panes(vec![vec![false; 3]; 2]));
	}
}
