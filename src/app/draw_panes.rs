//! Drawing a split tab: each pane's grid in its place and the dividers
//! between them, whole or only the rows that changed in each pane.
use super::{canvas::merge_bands, *};
use crate::{
	graphics::Rect,
	render::{
		background::wallpaper_rect,
		grid::draw_grid,
		padding::{extend_above, extend_below},
		painter::Painter,
		pen::{Pen, Untracked},
	},
	ui::panes::{Place, damaged_strips, divider_line, pane_padding},
};

impl App {
	/// Copies the screens of the panes at `places` for drawing.
	pub(super) fn snapshot_panes(&mut self, places: &[Place]) {
		let mut frames = std::mem::take(&mut self.pane_frames);
		frames.resize_with(places.len(), Frame::default);
		let (tab, focused) = (&self.splits[self.active], &self.sessions[self.active]);
		for (place, frame) in places.iter().zip(&mut frames) {
			if let Some(session) = tab.find(focused, place.id) {
				session.term().snapshot(frame);
			}
		}
		self.pane_frames = frames;
	}

	/// Draws the active tab's panes from their snapshots: all of each, or
	/// with `rows`, those rows of each pane in slot order.
	pub(super) fn draw_panes(&mut self, p: &mut impl Pen, scale: usize, rows: Option<&[Vec<bool>]>) {
		let ((area, _, _), (width, height)) = (self.pane_geometry(), p.size());
		let padding = self.padding_area(width, height);
		let focused = self.sessions[self.active].id;
		let frames = std::mem::take(&mut self.pane_frames);
		let places = self.tab_places(self.active);
		for (slot, (place, frame)) in places.iter().zip(&frames).enumerate() {
			let pad = pane_padding(place.rect, area, padding);
			let view =
				(self.grid_view(frame, scale, pad)).in_pane(frame, place.rect, pad, place.id == focused);
			// pr-check: window glue: draws the real window's panes as the tested views say
			match rows.and_then(|rows| rows.get(slot)) {
				// Kept rows would belong to several grids: the pen keeps none.
				None => draw_grid(&mut Untracked(&mut *p), &mut self.fonts, &view, None),
				Some(rows) => {
					draw_grid(p, &mut self.fonts, &view, Some(rows));
					extend_above(p, &view);
					extend_below(p, &view);
				}
			}
			// pr-check: end window glue
		}
		self.pane_frames = frames;
		let (area, cell, divider) = self.pane_geometry();
		let dividers = self.splits[self.active]
			.layout()
			.dividers(area, cell, divider);
		for line in dividers.into_iter().map(|gap| divider_line(gap, scale)) {
			p.rect(line.x, line.y, line.width, line.height, self.theme.border);
		}
	}

	/// Updates the previous frame's pixels of a split tab: the bars, and in
	/// each pane its changed `rows` and the padding around its grid.
	/// Returns the changed pixel rows as `(start, end)` bands.
	pub(super) fn compose_panes_damaged(
		&mut self,
		p: &mut Painter,
		rows: &[Vec<bool>],
	) -> Vec<(usize, usize)> {
		let scale = self.scale();
		let mut bands = self.redraw_bars(p, scale);
		let ((area, _, _), padding) = (self.pane_geometry(), self.padding_area(p.width, p.height));
		for (place, damaged) in self.tab_places(self.active).iter().zip(rows) {
			let pad = pane_padding(place.rect, area, padding);
			for strip in damaged_strips(place.rect, pad, self.ch, damaged) {
				self.restore_rect(p, strip);
				bands.push((strip.y, (strip.y + strip.height).min(p.height)));
			}
		}
		self.draw_panes(p, scale, Some(rows));
		merge_bands(&mut bands);
		bands
	}

	/// Repaints `rect`, inside the border, with the window's background:
	/// the wallpaper or the theme's background.
	pub(super) fn restore_rect(&mut self, p: &mut impl Pen, rect: Rect) {
		let (border, bg) = (self.border, self.theme.bg);
		let Some(paper) = self.wallpaper.as_mut() else {
			return p.rect(rect.x, rect.y, rect.width, rect.height, bg);
		};
		let image = paper.inside(p.size(), border, bg, self.wallpaper_strength);
		wallpaper_rect(p, rect, border, image);
	}
}
