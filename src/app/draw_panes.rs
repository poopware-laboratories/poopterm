//! Drawing a split tab: each pane's grid in its place and the dividers
//! between them, whole or only the rows that changed in each pane.
use super::{canvas::merge_bands, *};
use crate::{
	graphics::Rect,
	render::{
		background::wallpaper_rect,
		grid::{GridView, draw_grid},
		padding::{extend_above, extend_below},
		painter::Painter,
		pen::{Pen, Untracked},
	},
	ui::panes::{Place, divider_line, pane_padding},
};

impl App {
	/// Copies the screens of the panes at `places` for drawing.
	pub(super) fn snapshot_panes(&mut self, places: &[Place]) {
		let mut frames = std::mem::take(&mut self.pane_frames);
		frames.resize_with(places.len(), Frame::default);
		let tab = self.active;
		let sessions = std::iter::once(&self.sessions[tab]).chain(&self.splits[tab].others);
		let sessions: Vec<&Session> = sessions.collect();
		for (place, frame) in places.iter().zip(&mut frames) {
			if let Some(session) = sessions.iter().find(|session| session.id == place.id) {
				session.term().snapshot(frame);
			}
		}
		self.pane_frames = frames;
	}

	/// Draws the active tab's panes from their snapshots: all of each, or
	/// with `rows`, those rows of each pane in slot order.
	pub(super) fn draw_panes(&mut self, p: &mut impl Pen, scale: usize, rows: Option<&[Vec<bool>]>) {
		let (area, size) = (self.grid_area(), p.size());
		let padding = self.padding_area(size.0, size.1);
		let focused = self.splits[self.active].layout.focused();
		let (selection, search_match) = (
			self.selection_rows(),
			self.search_matches.get(self.search_match_index).copied(),
		);
		let frames = std::mem::take(&mut self.pane_frames);
		for (slot, (place, frame)) in self.pane_places().iter().zip(&frames).enumerate() {
			let is_focused = place.id == focused;
			let view = GridView {
				frame,
				origin: (place.rect.x, place.rect.y),
				cell: (self.cw, self.ch),
				px: self.px,
				ascent: self.ascent,
				scale,
				default_bg: self.theme.bg,
				link_color: self.theme.palette[4],
				selection_colors: selection_colors(self.theme.bg),
				match_colors: (self.theme.bg, self.theme.palette[3]),
				selection: selection.filter(|_| is_focused),
				search_match: search_match.filter(|_| is_focused),
				focused: self.focused && is_focused,
				padding: Some(pane_padding(place.rect, area, padding)),
			};
			match rows.and_then(|rows| rows.get(slot)) {
				// Kept rows would belong to several grids: the pen keeps none.
				None => draw_grid(&mut Untracked(&mut *p), &mut self.fonts, &view, None),
				Some(rows) => {
					draw_grid(p, &mut self.fonts, &view, Some(rows));
					extend_above(p, &view);
					extend_below(p, &view);
				}
			}
		}
		self.pane_frames = frames;
		if rows.is_none() {
			let divider = self.divider();
			let layout = &self.splits[self.active].layout;
			for gap in layout.dividers(area, (self.cw, self.ch), divider) {
				let line = divider_line(gap, scale);
				p.rect(line.x, line.y, line.width, line.height, self.theme.border);
			}
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
		let (area, ch) = (self.grid_area(), self.ch);
		let padding = self.padding_area(p.width, p.height);
		let places = self.pane_places();
		for ((place, damaged), frame_rows) in places.iter().zip(rows).zip(self.grid_rows()) {
			let pad = pane_padding(place.rect, area, padding);
			let grid_bottom = (place.rect.y + frame_rows * ch).min(pad.y + pad.height);
			let band = |y: usize, height: usize| Rect { y, height, ..pad };
			let mut strips = vec![
				band(pad.y, place.rect.y.saturating_sub(pad.y)),
				band(
					grid_bottom,
					(pad.y + pad.height).saturating_sub(grid_bottom),
				),
			];
			let changed = damaged.iter().enumerate().filter(|(_, damaged)| **damaged);
			strips.extend(changed.map(|(row, _)| band(place.rect.y + row * ch, ch)));
			for strip in strips {
				self.restore_rect(p, strip);
				bands.push((strip.y, (strip.y + strip.height).min(p.height)));
			}
		}
		self.draw_panes(p, scale, Some(rows));
		merge_bands(&mut bands);
		bands
	}

	/// How many rows each pane's snapshot holds, in slot order.
	fn grid_rows(&self) -> Vec<usize> {
		self.pane_frames.iter().map(|frame| frame.rows).collect()
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
