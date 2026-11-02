//! The Settings window's Diagnostics page: how the window is drawn and why,
//! recent warnings, and Copy report, laid out as the approved design
//! (#205) draws it. Positions are logical pixels; the page's text comes
//! from `ui::diagnostics_page` and its colors from `ui::diagnostics_colors`.
use super::{
	settings_hit::SettingsTarget,
	settings_pen::{Pen, back_rect, footer_rect},
};
use crate::{
	render::{
		painter::ButtonStyle,
		scene::{Rect, Scene},
	},
	ui::{
		diagnostics::{Diagnostics, statuses},
		diagnostics_colors::DiagnosticsColors,
		diagnostics_page::{Section, copy_label, log_intro, log_rows, split_time},
	},
};

/// Top of the section switch, and the size of its buttons.
const SECTION_ROW: usize = 76;
const SECTION_BUTTON: (usize, usize) = (104, 32);
/// Top of a section's content.
pub(super) const CONTENT_TOP: usize = 128;
/// The design's text sizes, 22, 15, 14, 13 and 12.5 px, over the window's 18.
const TITLE: f32 = 22.0 / 18.0;
const BUTTON: f32 = 15.0 / 18.0;
pub(super) const BODY: f32 = 14.0 / 18.0;
pub(super) const SMALL: f32 = 13.0 / 18.0;
pub(super) const FOOTNOTE: f32 = 12.5 / 18.0;
/// The log's monospace text, 12 px.
const MONO: f32 = 12.0 / 18.0;
/// From a heading's top to the first row under it.
const HEADING_GAP: usize = 24;
/// The features' rows: their height, top and columns (name, status, detail).
pub(super) const ROW: usize = 34;
pub(super) const FEATURES_TOP: usize = CONTENT_TOP + HEADING_GAP;
pub(super) const COLUMNS: [usize; 3] = [24, 156, 384];
/// The "Device" heading, below the features and the line under them, and
/// the distance between its rows.
pub(super) const DEVICE_TOP: usize = FEATURES_TOP + 5 * ROW + 48;
pub(super) const FACT_STRIDE: usize = 25;
/// The log's well: its top, how far above the window's bottom it ends,
/// the inset of its text, and the distance between its lines.
pub(super) const WELL_TOP: usize = CONTENT_TOP + 26;
pub(super) const WELL_BOTTOM: usize = 114;
pub(super) const WELL_INSET: (usize, usize) = (13, 11);
pub(super) const LOG_LINE: usize = 20;
/// Copy report's width.
pub(super) const COPY_WIDTH: usize = 140;

/// Section `index`'s switch button, in physical pixels.
fn section_button(index: usize, s: usize) -> Rect {
	let (w, h) = SECTION_BUTTON;
	((24 + index * (w + 8)) * s, SECTION_ROW * s, w * s, h * s)
}

/// How many log lines fit in the well of a window `height` physical pixels
/// tall.
pub(super) fn log_capacity(height: usize, scale: usize) -> usize {
	let height = height / scale.max(1);
	height.saturating_sub(WELL_TOP + WELL_BOTTOM + 2 * WELL_INSET.1) / LOG_LINE
}

/// The control under physical `(x, y)` on the Diagnostics page.
pub(super) fn diagnostics_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	s: usize,
) -> Option<SettingsTarget> {
	let within = |(rx, ry, rw, rh): Rect| (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y);
	let (back, copy) = (
		back_rect(width, s),
		footer_rect((width, height), COPY_WIDTH, s),
	);
	let section = (0..Section::ALL.len()).find(|&index| within(section_button(index, s)));
	if within(back) {
		Some(SettingsTarget::Back)
	} else if within(copy) {
		Some(SettingsTarget::CopyReport)
	} else {
		section.map(|index| SettingsTarget::Section(Section::ALL[index]))
	}
}

/// Draws the page showing `section` of `diagnostics` in `colors`, with
/// `status`, how Copy report went, in the footer, or the section's
/// footnote while it is empty.
#[allow(clippy::too_many_arguments)]
pub(super) fn compose_diagnostics(
	diagnostics: &Diagnostics,
	section: Section,
	status: &str,
	colors: &DiagnosticsColors,
	style: ButtonStyle,
	p: &mut Scene,
	pen: &mut Pen,
	hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
) {
	let s = pen.scale;
	pen.text(p, 24 * s, 24, "Diagnostics", TITLE);
	// The design lights the way back: this page is one level down.
	pen.back_button(p, style, &|_, _, _, _| true);
	for (index, shown) in Section::ALL.into_iter().enumerate() {
		let rect = section_button(index, s);
		let lit = shown == section || hovered(rect.0, rect.1, rect.2, rect.3);
		p.button(rect, style, lit);
		pen.centered(p, rect, shown.label(), BODY);
	}
	match section {
		Section::Graphics => graphics(diagnostics, colors, p, pen),
		Section::Log => log(diagnostics, colors, p, pen),
	}
	footer(section, status, colors, style, p, pen, hovered);
}

/// A table of the features, a mark and a status in its color for each,
/// then the device's facts.
fn graphics(diagnostics: &Diagnostics, colors: &DiagnosticsColors, p: &mut Scene, pen: &mut Pen) {
	let s = pen.scale;
	let [name_x, status_x, detail_x] = COLUMNS.map(|x| x * s);
	let right = p.width.saturating_sub(24 * s);
	pen.tinted(p, name_x, CONTENT_TOP, "Features", SMALL, colors.muted);
	for (i, status) in statuses(&diagnostics.drawing).iter().enumerate() {
		let top = FEATURES_TOP + i * ROW;
		pen.text(p, name_x, top + 8, status.feature.name(), BODY);
		let color = if status.accelerated {
			colors.accelerated
		} else {
			colors.software
		};
		mark(
			p,
			(status_x, (top + 11) * s),
			s,
			status.accelerated,
			color,
			colors.page,
		);
		pen.tinted(p, status_x + 20 * s, top + 8, status.how(), BODY, color);
		let detail = pen.fit(&status.detail, right.saturating_sub(detail_x), SMALL);
		pen.tinted(p, detail_x, top + 9, &detail, SMALL, colors.muted);
		p.rect(
			name_x,
			(top + ROW - 1) * s,
			right.saturating_sub(name_x),
			s,
			colors.rule,
		);
	}
	pen.tinted(p, name_x, DEVICE_TOP, "Device", SMALL, colors.muted);
	for (i, (label, value)) in diagnostics.facts.iter().enumerate() {
		let top = DEVICE_TOP + HEADING_GAP + i * FACT_STRIDE;
		pen.tinted(p, name_x, top, label, BODY, colors.muted);
		let value = pen.fit(value, right.saturating_sub(status_x), BODY);
		pen.text(p, status_x, top, &value, BODY);
	}
}

/// A 12 px ring in `color` around a dot when `full`, its inside `page`.
fn mark(p: &mut Scene, (x, y): (usize, usize), s: usize, full: bool, color: u32, page: u32) {
	let (side, stroke) = (12 * s, (3 * s).div_ceil(2));
	let inner = side - 2 * stroke;
	p.round_rect(x, y, side, side, side / 2, color);
	p.round_rect(x + stroke, y + stroke, inner, inner, inner / 2, page);
	if full {
		p.round_rect(x + 3 * s, y + 3 * s, 6 * s, 6 * s, 3 * s, color);
	}
}

/// The log's intro, then its rows in a well down to the footer: each
/// warning's time, muted, and its message.
fn log(diagnostics: &Diagnostics, colors: &DiagnosticsColors, p: &mut Scene, pen: &mut Pen) {
	let s = pen.scale;
	let right = p.width.saturating_sub(24 * s);
	pen.tinted(p, 24 * s, CONTENT_TOP, &log_intro(), SMALL, colors.muted);
	let (x, y) = (24 * s, WELL_TOP * s);
	let (w, h) = (
		right.saturating_sub(x),
		(p.height / s).saturating_sub(WELL_TOP + WELL_BOTTOM) * s,
	);
	p.round_rect(x, y, w, h, 6 * s, colors.border);
	p.round_rect(
		x + s,
		y + s,
		w.saturating_sub(2 * s),
		h.saturating_sub(2 * s),
		5 * s,
		colors.well,
	);
	let (inset_x, inset_y) = WELL_INSET;
	let text_x = x + inset_x * s;
	let width = right.saturating_sub(text_x + (inset_x - 1) * s);
	for (i, row) in log_rows(diagnostics, log_capacity(p.height, s))
		.iter()
		.enumerate()
	{
		let top = WELL_TOP + inset_y + i * LOG_LINE;
		let (time, message) = split_time(row);
		let mut message_x = text_x;
		if !time.is_empty() {
			pen.mono(p, text_x, top, time, MONO, colors.muted);
			message_x += pen.mono_width(time, MONO) + 12 * s;
		}
		let room = (text_x + width).saturating_sub(message_x);
		let message = pen.mono_fit(message, room, MONO);
		pen.mono(p, message_x, top, &message, MONO, pen.color);
	}
}

/// Copy report at the right, labelled "Copied" after a copy that worked,
/// and at the left the section's footnote or how Copy report went.
fn footer(
	section: Section,
	status: &str,
	colors: &DiagnosticsColors,
	style: ButtonStyle,
	p: &mut Scene,
	pen: &mut Pen,
	hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
) {
	let s = pen.scale;
	let label = copy_label(status);
	let rect = footer_rect((p.width, p.height), COPY_WIDTH, s);
	p.button(rect, style, hovered(rect.0, rect.1, rect.2, rect.3));
	pen.centered(p, rect, label, BUTTON);
	let line = if status.is_empty() {
		section.footnote()
	} else {
		status
	};
	// A failed copy is news: it keeps the text color.
	let color = if status.is_empty() || label == "Copied" {
		colors.muted
	} else {
		pen.color
	};
	let line = pen.fit(line, rect.0.saturating_sub(48 * s), FOOTNOTE);
	pen.tinted(p, 24 * s, rect.1 / s + 8, &line, FOOTNOTE, color);
}
