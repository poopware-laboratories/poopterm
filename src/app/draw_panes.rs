//! Drawing a split tab: each pane's grid in its place and the dividers
//! between them, whole or only the rows that changed in each pane.
use super::{canvas::merge_bands, *};
use crate::{
	render::{background::fill_inside, painter::Painter, panes, pen::Pen},
	ui::{
		chrome_colors::hairline,
		panes::{Place, divider_line, pane_padding},
	},
};

impl App {
	/// Copies the screens of the panes at `places` for drawing.
	pub(super) fn snapshot_panes(&mut self, places: &[Place]) {
		let mut frames = std::mem::take(&mut self.pane_frames);
		frames.resize_with(places.len(), Frame::default);
		let (tab, focused) = (&self.splits[self.active], &self.sessions[self.active]);
		// pr-check: window glue: copies the live panes' screens
		for (place, frame) in places.iter().zip(&mut frames) {
			if let Some(session) = tab.find(focused, place.id) {
				session.term().snapshot(frame);
			}
		}
		// pr-check: end window glue
		self.pane_frames = frames;
	}

	/// Draws the active tab's panes from their snapshots, and the dividers
	/// between them: all of each pane, or with `rows`, those rows of each.
	/// Returns the pixel rows repainted.
	pub(super) fn draw_panes(
		&mut self,
		p: &mut impl Pen,
		scale: usize,
		rows: Option<&[Vec<bool>]>,
	) -> Vec<(usize, usize)> {
		let ((area, cell, divider), size) = (self.pane_geometry(), p.size());
		let (padding, focused) = (
			self.padding_area(size.0, size.1),
			self.sessions[self.active].id,
		);
		let (frames, places) = (
			std::mem::take(&mut self.pane_frames),
			self.tab_places(self.active),
		);
		// pr-check: window glue: hands the live panes to the tested pane views and drawing
		let views: Vec<_> = (places.iter().zip(&frames))
			.map(|(place, frame)| {
				let pad = pane_padding(place.rect, area, padding);
				let view = self.grid_view(frame, scale, pad);
				view.in_pane(frame, place.rect, pad, place.id == focused)
			})
			.collect();
		let (border, bg, strength) = (self.border, self.theme.bg, self.wallpaper_strength);
		let paper =
			(self.wallpaper.as_mut()).map(|paper| paper.inside(size, border, bg, strength).clone());
		let restore = |p: &mut _, rect| fill_inside(p, rect, border, (bg, paper.as_ref()));
		let bands = panes::draw_panes(p, &mut self.fonts, &views, rows, restore);
		// pr-check: end window glue
		self.pane_frames = frames;
		let dividers = self.splits[self.active]
			.layout()
			.dividers(area, cell, divider);
		let line_color = hairline(&self.theme);
		for line in dividers.into_iter().map(|gap| divider_line(gap, scale)) {
			p.rect(line.x, line.y, line.width, line.height, line_color);
		}
		bands
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
		bands.extend(self.draw_panes(p, scale, Some(rows)));
		merge_bands(&mut bands);
		bands
	}
}
