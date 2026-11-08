//! Split panes: how one tab's terminals share its grid area, which pane a
//! point falls in, and how focus moves and room is given back when a pane
//! closes. Panes are named by their session ids; the tab owns the sessions.
use crate::graphics::Rect;

/// How a tab's panes are arranged, and in which order its slots list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrangement {
	/// One pane fills the tab.
	Single,
	/// Two panes side by side: left, right.
	Columns,
	/// Two panes stacked: top, bottom.
	Rows,
	/// One big pane on a side and two small ones stacked on the other:
	/// big, top small, bottom small.
	Main(Side),
}

/// The side the big pane of [`Arrangement::Main`] takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
	Left,
	Right,
}

/// Where a new pane goes beside the focused one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Split {
	/// To its right, the two side by side.
	Right,
	/// Below it, the two stacked.
	Down,
}

/// A direction to move focus in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
	Left,
	Right,
	Up,
	Down,
}

/// One pane's place in the tab: its session, its pixels, and the grid of
/// cells that fits in them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
	pub id: u64,
	pub rect: Rect,
	pub grid: (usize, usize),
}

/// A tab's panes: their arrangement, their sessions in slot order, and the
/// focused slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Panes {
	arrangement: Arrangement,
	ids: Vec<u64>,
	focus: usize,
}

impl Panes {
	/// One pane, session `id`, filling the tab.
	pub(crate) fn new(id: u64) -> Self {
		Self {
			arrangement: Arrangement::Single,
			ids: vec![id],
			focus: 0,
		}
	}

	#[cfg(test)]
	pub(crate) fn arrangement(&self) -> Arrangement {
		self.arrangement
	}

	/// The panes' sessions in slot order.
	pub(crate) fn ids(&self) -> &[u64] {
		&self.ids
	}

	/// Whether the tab shows more than one pane.
	pub(crate) fn is_split(&self) -> bool {
		self.ids.len() > 1
	}

	/// The focused pane's session.
	pub(crate) fn focused(&self) -> u64 {
		self.ids[self.focus]
	}

	/// The arrangement and slots after splitting the focused pane, or
	/// `None` when no arrangement has that shape. The new pane is last.
	fn after_split(&self, split: Split, id: u64) -> Option<(Arrangement, Vec<u64>)> {
		let ids = &self.ids;
		Some(match (self.arrangement, split, self.focus) {
			(Arrangement::Single, Split::Right, _) => (Arrangement::Columns, vec![ids[0], id]),
			(Arrangement::Single, Split::Down, _) => (Arrangement::Rows, vec![ids[0], id]),
			// The pane split down shares its column; the other grows into
			// the big one on its side.
			(Arrangement::Columns, Split::Down, 1) => {
				(Arrangement::Main(Side::Left), vec![ids[0], ids[1], id])
			}
			(Arrangement::Columns, Split::Down, _) => {
				(Arrangement::Main(Side::Right), vec![ids[1], ids[0], id])
			}
			_ => return None,
		})
	}

	/// Whether the focused pane can be split that way.
	pub(crate) fn can_split(&self, split: Split) -> bool {
		self.after_split(split, 0).is_some()
	}

	/// Splits the focused pane, putting session `id` beside or below it
	/// and focusing it. Returns false, changing nothing, when no
	/// arrangement has that shape or `id` is already a pane.
	pub(crate) fn split(&mut self, split: Split, id: u64) -> bool {
		if self.ids.contains(&id) {
			return false;
		}
		let Some((arrangement, ids)) = self.after_split(split, id) else {
			return false;
		};
		self.arrangement = arrangement;
		self.focus = ids.len() - 1;
		self.ids = ids;
		true
	}

	/// Closes pane `id`, its room going to the panes beside it. Returns
	/// false, changing nothing, for the last pane, which only closing the
	/// tab removes, and for a session that is not a pane. A closed focused
	/// pane passes focus to the pane that takes its room.
	pub(crate) fn close(&mut self, id: u64) -> bool {
		let Some(slot) = self.ids.iter().position(|&pane| pane == id) else {
			return false;
		};
		let (arrangement, heir) = match (self.arrangement, slot) {
			(Arrangement::Single, _) => return false,
			(Arrangement::Columns | Arrangement::Rows, _) => (Arrangement::Single, 1 - slot),
			// The small panes grow sideways into the big one's room.
			(Arrangement::Main(_), 0) => (Arrangement::Rows, 1),
			// The other small pane grows into the closed one's room.
			(Arrangement::Main(_), _) => (Arrangement::Columns, 3 - slot),
		};
		let focused = if self.focus == slot {
			self.ids[heir]
		} else {
			self.focused()
		};
		self.ids.remove(slot);
		// The big pane stays on its side.
		if self.arrangement == Arrangement::Main(Side::Right) && slot != 0 {
			self.ids.swap(0, 1);
		}
		self.arrangement = arrangement;
		self.focus = self.slot(focused).unwrap_or(0);
		true
	}

	fn slot(&self, id: u64) -> Option<usize> {
		self.ids.iter().position(|&pane| pane == id)
	}

	/// Focuses pane `id`. Returns whether focus moved: false for the
	/// focused pane and for a session that is not a pane.
	pub(crate) fn focus(&mut self, id: u64) -> bool {
		match self.slot(id) {
			Some(slot) if slot != self.focus => {
				self.focus = slot;
				true
			}
			_ => false,
		}
	}

	/// Focuses the pane `offset` slots on, wrapping around. Returns whether
	/// focus moved.
	pub(crate) fn cycle(&mut self, offset: isize) -> bool {
		let count = self.ids.len() as isize;
		let slot = (self.focus as isize + offset).rem_euclid(count) as usize;
		let moved = slot != self.focus;
		self.focus = slot;
		moved
	}

	/// Focuses the nearest pane in `direction` from the focused one, laid
	/// out in `area`: the closest of those beside it on that side, then the
	/// one sharing most of its edge, then the earliest slot. Returns whether
	/// focus moved.
	pub(crate) fn focus_toward(
		&mut self,
		direction: Direction,
		area: Rect,
		cell: (usize, usize),
		divider: usize,
	) -> bool {
		let places = self.places(area, cell, divider);
		let from = places[self.focus].rect;
		let best = (places.iter().enumerate())
			.filter_map(|(slot, place)| {
				let (gap, shared) = beside(from, place.rect, direction)?;
				Some((gap, std::cmp::Reverse(shared), slot))
			})
			.min();
		match best {
			Some((_, _, slot)) => {
				self.focus = slot;
				true
			}
			None => false,
		}
	}

	/// Each pane's place in `area`, in slot order, cut by `divider` pixel
	/// wide dividers that sit on whole cells from the top left, each grid
	/// at least 2 by 2 cells.
	pub(crate) fn places(&self, area: Rect, cell: (usize, usize), divider: usize) -> Vec<Place> {
		let rects = match self.arrangement {
			Arrangement::Single => vec![area],
			Arrangement::Columns => columns(area, cell.0, divider).to_vec(),
			Arrangement::Rows => rows(area, cell.1, divider).to_vec(),
			Arrangement::Main(side) => {
				let [left, right] = columns(area, cell.0, divider);
				let (big, small) = match side {
					Side::Left => (left, right),
					Side::Right => (right, left),
				};
				let [top, bottom] = rows(small, cell.1, divider);
				vec![big, top, bottom]
			}
		};
		(self.ids.iter().zip(rects))
			.map(|(&id, rect)| Place {
				id,
				rect,
				grid: grid(rect, cell),
			})
			.collect()
	}

	/// The dividers between the panes in `area`, as [`places`](Self::places)
	/// lays them out.
	pub(crate) fn dividers(&self, area: Rect, cell: (usize, usize), divider: usize) -> Vec<Rect> {
		let [left, right] = columns(area, cell.0, divider);
		let vertical = Rect {
			x: left.x + left.width,
			width: right.x - (left.x + left.width),
			..area
		};
		let [top, bottom] = rows(area, cell.1, divider);
		let horizontal = Rect {
			y: top.y + top.height,
			height: bottom.y - (top.y + top.height),
			..area
		};
		let small = |rect: Rect| {
			let [top, bottom] = rows(rect, cell.1, divider);
			Rect {
				y: top.y + top.height,
				height: bottom.y - (top.y + top.height),
				..rect
			}
		};
		match self.arrangement {
			Arrangement::Single => Vec::new(),
			Arrangement::Columns => vec![vertical],
			Arrangement::Rows => vec![horizontal],
			Arrangement::Main(Side::Left) => vec![vertical, small(right)],
			Arrangement::Main(Side::Right) => vec![vertical, small(left)],
		}
	}

	/// The pane under `point` in `area`, as [`places`](Self::places) lays
	/// them out; `None` on a divider or outside the area.
	pub(crate) fn hit(
		&self,
		area: Rect,
		cell: (usize, usize),
		divider: usize,
		(x, y): (usize, usize),
	) -> Option<u64> {
		let inside = |r: Rect| (r.x..r.x + r.width).contains(&x) && (r.y..r.y + r.height).contains(&y);
		(self.places(area, cell, divider).into_iter())
			.find(|place| inside(place.rect))
			.map(|place| place.id)
	}
}

/// How wide the gap between panes is at a whole-number chrome `scale`:
/// odd, so the line drawn in it sits in its middle.
pub(crate) fn divider_width(scale: usize) -> usize {
	9 * scale.max(1)
}

/// The line drawn in the middle of a `divider` gap, `scale` pixels thick
/// across it.
pub(crate) fn divider_line(divider: Rect, scale: usize) -> Rect {
	let thickness = scale.max(1);
	if divider.width < divider.height {
		let width = thickness.min(divider.width);
		Rect {
			x: divider.x + (divider.width - width) / 2,
			width,
			..divider
		}
	} else {
		let height = thickness.min(divider.height);
		Rect {
			y: divider.y + (divider.height - height) / 2,
			height,
			..divider
		}
	}
}

/// The grid of `cell`-sized cells that fits in `rect`, at least 2 by 2.
pub(crate) fn grid(rect: Rect, cell: (usize, usize)) -> (usize, usize) {
	(
		(rect.width / cell.0.max(1)).max(2),
		(rect.height / cell.1.max(1)).max(2),
	)
}

/// The length before a divider cutting `length` in two halves: half of
/// what the divider leaves, down to whole cells of `cell`.
fn first_half(length: usize, cell: usize, divider: usize) -> usize {
	let cell = cell.max(1);
	length.saturating_sub(divider) / 2 / cell * cell
}

/// `area` cut into a left and a right half by a `divider` wide gap.
fn columns(area: Rect, cell: usize, divider: usize) -> [Rect; 2] {
	let width = first_half(area.width, cell, divider);
	let gap = divider.min(area.width - width);
	[
		Rect { width, ..area },
		Rect {
			x: area.x + width + gap,
			width: area.width - width - gap,
			..area
		},
	]
}

/// `area` cut into a top and a bottom half by a `divider` high gap.
fn rows(area: Rect, cell: usize, divider: usize) -> [Rect; 2] {
	let height = first_half(area.height, cell, divider);
	let gap = divider.min(area.height - height);
	[
		Rect { height, ..area },
		Rect {
			y: area.y + height + gap,
			height: area.height - height - gap,
			..area
		},
	]
}

/// How far `to` lies from `from` in `direction`, and how much of `from`'s
/// edge facing it they share; `None` unless it lies wholly on that side and
/// shares some of that edge.
fn beside(from: Rect, to: Rect, direction: Direction) -> Option<(usize, usize)> {
	let span = |start: usize, length: usize| (start, start + length);
	let (horizontal, vertical) = (
		(span(from.x, from.width), span(to.x, to.width)),
		(span(from.y, from.height), span(to.y, to.height)),
	);
	// Along the direction, then across it.
	let ((from_along, to_along), (from_across, to_across)) = match direction {
		Direction::Left | Direction::Right => (horizontal, vertical),
		Direction::Up | Direction::Down => (vertical, horizontal),
	};
	let gap = match direction {
		Direction::Left | Direction::Up => from_along.0.checked_sub(to_along.1)?,
		Direction::Right | Direction::Down => to_along.0.checked_sub(from_along.1)?,
	};
	let shared = from_across
		.1
		.min(to_across.1)
		.saturating_sub(from_across.0.max(to_across.0));
	(shared > 0).then_some((gap, shared))
}

/// The padding a pane at `rect` draws its edge cells' backgrounds into:
/// its own rect, reaching out to `padding`'s edges on the sides where it
/// touches `area`'s, so the window's padding takes the edge panes' colors.
pub(crate) fn pane_padding(rect: Rect, area: Rect, padding: Rect) -> Rect {
	let (left, top) = (
		if rect.x == area.x { padding.x } else { rect.x },
		if rect.y == area.y { padding.y } else { rect.y },
	);
	let right = if rect.x + rect.width == area.x + area.width {
		padding.x + padding.width
	} else {
		rect.x + rect.width
	};
	let bottom = if rect.y + rect.height == area.y + area.height {
		padding.y + padding.height
	} else {
		rect.y + rect.height
	};
	Rect {
		x: left,
		y: top,
		width: right.saturating_sub(left),
		height: bottom.saturating_sub(top),
	}
}
