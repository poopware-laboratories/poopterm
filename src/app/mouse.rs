//! Mouse events: selection, links, wheel scrolling and mouse reporting.
use super::draw_tabs::hit;
use super::*;
use crate::input::mouse::{MouseAction, MouseButtonCode, MouseReport, encode_mouse};
use crate::input::{paste::paste_bytes, scroll::alternate_scroll_bytes};
use crate::render::header::close_rect;
use crate::ui::{
	copy::selection_text,
	links::{LinkAction, link_action},
};
use winit::{dpi::PhysicalPosition, event::MouseScrollDelta};

impl App {
	/// Where the active terminal's grid sits in the window.
	pub(super) fn grid_geometry(&self) -> GridGeometry {
		let term = self.active_term();
		GridGeometry {
			left: self.grid_left(),
			top: self.top,
			bottom: (self.height as usize).saturating_sub(self.status_height + self.margin),
			cell: (self.cw, self.ch),
			cols: term.cols,
			rows: term.rows,
			first_row: term.scrollback.len().saturating_sub(term.scroll_offset),
		}
	}

	pub(super) fn point_at(&self, x: f64, y: f64) -> Option<CellPoint> {
		self.grid_geometry().point_at(x, y)
	}

	/// `point`, a row of the current scrollback and screen, as a line that
	/// keeps naming the same text when full history evicts lines.
	fn to_line(&self, point: CellPoint) -> CellPoint {
		CellPoint {
			row: point.row + self.active_term().history_base,
			col: point.col,
		}
	}

	pub(super) fn selected_text(&self) -> Option<String> {
		selection_text(&self.active_term(), self.selection?)
	}

	pub(super) fn copy_selection(&self) {
		if let Some(text) = self.selected_text()
			&& let Err(error) = clipboard_write(&text)
		{
			eprintln!("poopterm: clipboard copy failed: {error}");
		}
	}

	pub(super) fn token_at(&self, point: CellPoint) -> Option<String> {
		let term = self.active_term();
		let cells = term.row_cells(point.row)?;
		if point.col >= cells.len() || cells[point.col].ch.is_whitespace() {
			return None;
		}
		let is_token_char = |ch: char| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '<' | '>');
		let mut start = point.col;
		let mut end = point.col + 1;
		while start > 0 && is_token_char(cells[start - 1].ch) {
			start -= 1;
		}
		while end < cells.len() && is_token_char(cells[end].ch) {
			end += 1;
		}
		Some(
			cells[start..end]
				.iter()
				.filter(|cell| cell.width != 0)
				.flat_map(|cell| std::iter::once(cell.ch).chain(term.marks(cell).iter().copied()))
				.collect::<String>()
				.trim_matches(|ch: char| matches!(ch, ')' | ']' | '}' | ',' | '.' | ';' | ':'))
				.to_owned(),
		)
	}

	pub(super) fn open_or_copy_token(&self, x: f64, y: f64) -> bool {
		let Some(point) = self.point_at(x, y) else {
			return false;
		};
		let Some(token) = self.token_at(point) else {
			return false;
		};
		let term = self.active_term();
		let explicit = term
			.row_cells(point.row)
			.and_then(|cells| cells.get(point.col))
			.and_then(|cell| term.hyperlink_uri(cell))
			.filter(|uri| !uri.is_empty());
		let target = explicit.unwrap_or(&token);
		let result = match link_action(target) {
			Some(LinkAction::OpenUrl(url)) => open_external(&url),
			Some(LinkAction::OpenPath(path)) => open_external(&path.to_string_lossy()),
			Some(LinkAction::CopyPath(path)) => clipboard_write(&path),
			None => return false,
		};
		if let Err(error) = result {
			eprintln!("poopterm: could not follow link {target}: {error}");
		}
		true
	}

	/// The OSC 8 target under the pointer, shown in the status bar so a link's
	/// real destination is visible before it is clicked.
	pub(super) fn update_hovered_link(&mut self) {
		let (x, y) = self.cursor_pos;
		let link = self.point_at(x, y).and_then(|point| {
			let term = self.active_term();
			let cell = term.row_cells(point.row)?.get(point.col)?;
			term.hyperlink_uri(cell).map(str::to_owned)
		});
		// Links show a hand; otherwise the program's OSC 22 shape, if any.
		let shape = if link.is_some() {
			Some("pointer")
		} else {
			self.active_term().pointer_shape()
		};
		let icon = shape
			.and_then(|name| name.parse().ok())
			.unwrap_or(winit::window::CursorIcon::Default);
		if icon != self.pointer_icon
			&& let Some(window) = &self.window
		{
			window.set_cursor(icon);
			self.pointer_icon = icon;
		}
		if link != self.hovered_link {
			self.hovered_link = link;
			self.request_redraw();
		}
	}

	pub(super) fn paste_clipboard(&mut self) {
		match clipboard_read() {
			Ok(text) => {
				let bytes = paste_bytes(&text, self.active_term().bracketed_paste);
				self.active_session().write(&bytes);
			}
			Err(error) => eprintln!("poopterm: clipboard paste failed: {error}"),
		}
	}

	/// Reports a mouse event to the application when it enabled mouse tracking.
	/// Returns true when the event belongs to the application (even if it could
	/// not be encoded), so the caller skips local selection and scrolling.
	pub(super) fn send_mouse_report(
		&mut self,
		button: MouseButtonCode,
		kind: MouseAction,
		(x, y): (f64, f64),
	) -> bool {
		let Some(point) = self.point_at(x, y) else {
			return false;
		};
		let term = self.active_term();
		let wants = match kind {
			MouseAction::Motion if button == MouseButtonCode::None => term.mouse_tracking == 1003,
			MouseAction::Motion => matches!(term.mouse_tracking, 1002 | 1003),
			_ => term.mouse_tracking != 0,
		};
		if !wants {
			return false;
		}
		let visible_start = term.scrollback.len().saturating_sub(term.scroll_offset);
		let report = MouseReport {
			button,
			kind,
			col: point.col + 1,
			row: point.row.saturating_sub(visible_start) + 1,
			mods: self.mods,
		};
		if let Some(bytes) = encode_mouse(report, term.mouse_sgr) {
			self.active_session().write(&bytes);
		}
		true
	}

	/// A press clears the old selection; a new one appears only once the
	/// pointer is dragged to another cell, so clicks leave no mark.
	pub(super) fn begin_selection(&mut self, x: f64, y: f64) {
		if let Some(point) = self.point_at(x, y) {
			// Alt drags a rectangle, as in xterm and Alacritty.
			self.drag_anchor = Some((self.to_line(point), self.mods.alt));
			if self.selection.take().is_some() {
				self.request_redraw();
			}
		}
	}

	/// A plain click in the line being edited moves the shell's cursor there
	/// when its prompt asked for that (OSC 133 `cl=`), or reaches the shell
	/// as a mouse report (`click_events`).
	fn move_cursor_to_click(&mut self, x: f64, y: f64) {
		if self.mods.ctrl || self.mods.shift || self.mods.alt || self.mods.super_key {
			return;
		}
		let Some(point) = self.point_at(x, y) else {
			return;
		};
		let moves = {
			let term = self.active_term();
			point
				.row
				.checked_sub(term.scrollback.len())
				.and_then(|row| term.prompt_click(row, point.col))
		};
		if let Some(moves) = moves {
			self.selection = None;
			self.active_session().write(&moves);
		}
	}

	/// Drags past the grid's edges keep selecting up to the nearest cell.
	pub(super) fn extend_selection(&mut self, x: f64, y: f64) {
		let Some((anchor, block)) = self.drag_anchor else {
			return;
		};
		let point = self.to_line(self.grid_geometry().nearest(x, y));
		let selection = Selection::dragged(anchor, point, self.selection, block);
		if selection.map(Selection::ordered) != self.selection.map(Selection::ordered) {
			self.selection = selection;
			self.request_redraw();
		}
	}
}

impl App {
	pub(super) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
		let (x, y) = self.cursor_pos;
		let scroll_up = match &delta {
			MouseScrollDelta::LineDelta(_, n) => *n > 0.0,
			MouseScrollDelta::PixelDelta(p) => p.y > 0.0,
		};
		if self.handle_vertical_tab_wheel(x, y, scroll_up) {
			return;
		}
		if self.preferences.show_tabs
			&& !self.preferences.vertical_tabs
			&& y >= self.tab_y() as f64
			&& y < self.top as f64
		{
			let layout = self.tab_layout();
			if layout.first_visible > 0 && scroll_up {
				self.tab_scroll -= 1;
			} else if layout.first_visible + layout.visible_count < self.sessions.len() && !scroll_up {
				self.tab_scroll += 1;
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		let lines = match delta {
			MouseScrollDelta::LineDelta(_, lines) => self.scroll.lines(lines.into()),
			MouseScrollDelta::PixelDelta(p) => self.scroll.pixels(p.y, self.ch as f64),
		};
		if lines == 0 {
			return;
		}
		let button = if lines > 0 {
			MouseButtonCode::WheelUp
		} else {
			MouseButtonCode::WheelDown
		};
		for _ in 0..lines.unsigned_abs() {
			if !self.send_mouse_report(button, MouseAction::Press, (x, y)) {
				break;
			}
		}
		let over_grid = self.point_at(x, y).is_some();
		let mut term = self.active_term();
		if term.mouse_tracking != 0 && over_grid {
			return;
		}
		if term.primary_screen.is_some() {
			let bytes = alternate_scroll_bytes(lines, term.application_cursor);
			drop(term);
			self.active_session().write(&bytes);
			return;
		}
		term.scroll_view(lines);
		drop(term);
		self.request_redraw();
	}

	pub(super) fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
		let previous = self.cursor_pos;
		self.cursor_pos = (position.x, position.y);
		self.update_hovered_link();
		if self.drag_anchor.is_some() {
			self.extend_selection(position.x, position.y);
		} else {
			let button = self.held_button.unwrap_or(MouseButtonCode::None);
			self.send_mouse_report(button, MouseAction::Motion, self.cursor_pos);
		}
		let over_sidebar =
			self.is_over_vertical_sidebar(previous.0) || self.is_over_vertical_sidebar(position.x);
		if ((previous.1 < self.top as f64 || position.y < self.top as f64) || over_sidebar)
			&& let Some(window) = &self.window
		{
			window.request_redraw();
		}
	}

	pub(super) fn handle_mouse_input(
		&mut self,
		event_loop: &ActiveEventLoop,
		state: ElementState,
		button: MouseButton,
	) {
		let (x, y) = self.cursor_pos;
		let code = match button {
			MouseButton::Left => MouseButtonCode::Left,
			MouseButton::Middle => MouseButtonCode::Middle,
			MouseButton::Right => MouseButtonCode::Right,
			_ => return,
		};
		self.held_button = (state == ElementState::Pressed).then_some(code);
		if self.handle_vertical_tab_mouse(event_loop, state, button, (x, y)) {
			return;
		}
		let tab_y = self.tab_y() as f64;
		if self.preferences.show_tabs
			&& state == ElementState::Pressed
			&& button == MouseButton::Left
			&& y < self.top as f64
		{
			let layout = self.tab_layout();
			let in_tabbar = y >= tab_y && y < self.top as f64;
			if in_tabbar
				&& x >= layout.settings_x as f64
				&& x < (layout.settings_x + layout.settings_w) as f64
			{
				self.open_settings_window(event_loop);
			} else if in_tabbar && x >= layout.plus_x as f64 && x < (layout.plus_x + layout.plus_w) as f64
			{
				self.add_tab();
			} else if in_tabbar {
				if x >= layout.left_arrow_x as f64
					&& x < (layout.left_arrow_x + layout.arrow_w) as f64
					&& layout.first_visible > 0
				{
					self.tab_scroll = self.tab_scroll.saturating_sub(1);
				} else if x >= layout.right_arrow_x as f64
					&& x < (layout.right_arrow_x + layout.arrow_w) as f64
					&& layout.first_visible + layout.visible_count < self.sessions.len()
				{
					self.tab_scroll += 1;
				}
				for local_index in 0..layout.visible_count {
					let index = layout.first_visible + local_index;
					if index >= self.sessions.len() {
						break;
					}
					let left = layout.tab_x(index);
					let width = layout.tab_width(index);
					if x >= left as f64 && x < (left + width) as f64 {
						let close = close_rect(left, width, (self.tab_y(), self.tab_height), layout.scale);
						if self.sessions.len() > 1 && hit((x, y), close) {
							self.close_tab(index, event_loop);
						} else {
							self.activate_tab(index);
						}
						break;
					}
				}
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		} else if state == ElementState::Pressed
			&& button == MouseButton::Left
			&& y < self.titlebar_height as f64
		{
			if let Some(window) = &self.window {
				let _ = window.drag_window();
			}
		} else {
			let kind = if state == ElementState::Pressed {
				MouseAction::Press
			} else {
				MouseAction::Release
			};
			let handled = self.send_mouse_report(code, kind, (x, y));
			if state == ElementState::Pressed && button == MouseButton::Left && !handled {
				let tracking = self.active_term().mouse_tracking != 0;
				let link_click =
					(self.mods.ctrl || self.mods.super_key) && !tracking && self.open_or_copy_token(x, y);
				if !link_click {
					self.begin_selection(x, y);
				}
			}
			if state == ElementState::Released {
				// The press cleared the selection; none means no drag.
				let clicked = self.drag_anchor.take().is_some() && self.selection.is_none();
				if button == MouseButton::Left && !handled && clicked {
					self.move_cursor_to_click(x, y);
				}
			}
		}
	}
}
