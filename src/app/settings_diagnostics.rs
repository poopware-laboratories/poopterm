//! The Settings window's Diagnostics page: how the window is drawn and why,
//! recent warnings, and Copy report. Positions are logical pixels; the
//! page's text and layout come from `ui::diagnostics_page`.
use super::{settings_hit::SettingsTarget, settings_pen::Pen};
use crate::{
	render::{
		painter::ButtonStyle,
		scene::{Rect, Scene},
	},
	ui::{
		diagnostics::{Diagnostics, statuses},
		diagnostics_page::{Section, fit, log_rows},
	},
};

/// Top of the section switch, and the size of its buttons.
const SECTION_ROW: usize = 76;
const SECTION_BUTTON: (usize, usize) = (104, 32);
/// Top of a section's content, and the distance between its rows.
pub(super) const CONTENT_TOP: usize = 128;
pub(super) const STATUS_STRIDE: usize = 52;
pub(super) const FACT_STRIDE: usize = 28;
pub(super) const LOG_STRIDE: usize = 24;
/// Left edge of the Graphics section's second column.
const VALUE_X: usize = 220;
/// The footer's button: its label and width.
pub(super) const COPY_BUTTON: (&str, usize) = ("Copy report", 140);
/// The footer's status until Copy report says how it went.
pub(super) const NOTE: &str = "Copy report puts this page on the clipboard, with your home as ~.";

/// Section `index`'s switch button, in physical pixels.
fn section_button(index: usize, s: usize) -> Rect {
	let (w, h) = SECTION_BUTTON;
	((24 + index * (w + 8)) * s, SECTION_ROW * s, w * s, h * s)
}

/// How many log rows fit between the switch and the footer of a window
/// `height` physical pixels tall.
pub(super) fn log_capacity(height: usize, scale: usize) -> usize {
	(height / scale.max(1)).saturating_sub(64 + CONTENT_TOP) / LOG_STRIDE
}

/// The control under physical `(x, y)` on the Diagnostics page.
pub(super) fn diagnostics_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	s: usize,
) -> Option<SettingsTarget> {
	let within = |(rx, ry, rw, rh): Rect| (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y);
	let back = (width.saturating_sub(164 * s), 20 * s, 140 * s, 36 * s);
	let copy_w = COPY_BUTTON.1 * s;
	let copy = (
		width.saturating_sub(copy_w + 20 * s),
		height.saturating_sub(52 * s),
		copy_w,
		36 * s,
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

/// Draws the page showing `section` of `diagnostics`, with `status` in the
/// footer, or the note while it is empty.
pub(super) fn compose_diagnostics(
	diagnostics: &Diagnostics,
	section: Section,
	status: &str,
	style: ButtonStyle,
	p: &mut Scene,
	pen: &mut Pen,
	hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
) {
	let s = pen.scale;
	pen.text(p, 24 * s, 28, "Diagnostics", 1.0);
	pen.back_button(p, style, hovered);
	for (index, shown) in Section::ALL.into_iter().enumerate() {
		let rect = section_button(index, s);
		p.button(
			rect,
			style,
			shown == section || hovered(rect.0, rect.1, rect.2, rect.3),
		);
		pen.centered(p, rect, shown.label(), 0.76);
	}
	match section {
		Section::Graphics => graphics(diagnostics, p, pen),
		Section::Log => {
			let columns = pen.columns(p.width.saturating_sub(48 * s), 0.72);
			let rows = log_rows(diagnostics, log_capacity(p.height, s), columns);
			for (i, row) in rows.iter().enumerate() {
				pen.text(p, 24 * s, CONTENT_TOP + i * LOG_STRIDE, row, 0.72);
			}
		}
	}
	let status = if status.is_empty() { NOTE } else { status };
	pen.footer(p, style, COPY_BUTTON, status, hovered);
}

/// Each feature with a filled mark when the GPU draws it and a hollow one
/// when software does, then the facts.
fn graphics(diagnostics: &Diagnostics, p: &mut Scene, pen: &mut Pen) {
	let s = pen.scale;
	let (value_x, color) = (VALUE_X * s, pen.color);
	let width = p.width.saturating_sub((VALUE_X + 24) * s);
	for (i, status) in statuses(&diagnostics.drawing).iter().enumerate() {
		let top = CONTENT_TOP + i * STATUS_STRIDE;
		let (x, y, side, line) = (24 * s, (top + 5) * s, 12 * s, 2 * s);
		if status.accelerated {
			p.rect(x, y, side, side, color);
		} else {
			p.rect(x, y, side, line, color);
			p.rect(x, y + side - line, side, line, color);
			p.rect(x, y, line, side, color);
			p.rect(x + side - line, y, line, side, color);
		}
		pen.text(p, 48 * s, top, status.feature.name(), 0.82);
		let how = if status.accelerated {
			"Hardware accelerated"
		} else {
			"Software only"
		};
		pen.text(p, value_x, top, how, 0.82);
		let detail = fit(&status.detail, pen.columns(width, 0.68));
		pen.text(p, value_x, top + 24, &detail, 0.68);
	}
	let (first, columns) = (
		CONTENT_TOP + 5 * STATUS_STRIDE + 8,
		pen.columns(width, 0.76),
	);
	for (i, (label, value)) in diagnostics.facts.iter().enumerate() {
		let top = first + i * FACT_STRIDE;
		pen.text(p, 48 * s, top, label, 0.76);
		pen.text(p, value_x, top, &fit(value, columns), 0.76);
	}
}
