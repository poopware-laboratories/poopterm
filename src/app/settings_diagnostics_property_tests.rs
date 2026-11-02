//! Properties of the Diagnostics page over random window sizes, scales,
//! sections and contents: everything stays inside the window, the log
//! stays in its well above the footer, and only the page's colors are used.
use super::{
	settings_diagnostics::{LOG_LINE, WELL_BOTTOM, WELL_TOP, log_capacity},
	settings_diagnostics_tests::{COLORS, FG, draw_in, sample},
	settings_view::MIN_SIZE,
};
use crate::{
	fonts::{BUNDLED, Font},
	render::scene::Command,
	ui::{
		diagnostics::{Diagnostics, Drawing, Reason},
		diagnostics_page::{Section, copied},
	},
	warnings::Warning,
};
use std::time::Duration;

/// A small deterministic generator, so a failure names its case.
fn next(state: &mut u64) -> usize {
	*state = state
		.wrapping_mul(6_364_136_223_846_793_005)
		.wrapping_add(1_442_695_040_888_963_407);
	(*state >> 33) as usize
}

/// Text up to `max` characters long, from a mix of narrow, wide and non-ASCII ones.
fn words(state: &mut u64, max: usize) -> String {
	let len = next(state) % max;
	let pieces = ["a", "W", " ", "é", "-", "/", "0"];
	(0..len)
		.map(|_| pieces[next(state) % pieces.len()])
		.collect()
}

/// One random case: a scale, a window in physical pixels, a section, what
/// the page shows and the footer's status.
fn case(state: &mut u64) -> (usize, (usize, usize), Section, Diagnostics, String) {
	let s = 1 + next(state) % 3;
	let size = (
		(MIN_SIZE.0 + next(state) % 900) * s,
		(MIN_SIZE.1 + next(state) % 700) * s,
	);
	let section = Section::ALL[next(state) % Section::ALL.len()];
	let drawing = match next(state) % 3 {
		0 => Drawing::Gpu,
		1 => Drawing::Software(Reason::Asked),
		_ => Drawing::Software(Reason::Abandoned(words(state, 300))),
	};
	let mut diagnostics = sample(drawing, 0);
	diagnostics.warnings = (0..next(state) % 60)
		.map(|i| Warning {
			at: Duration::from_secs(i as u64 * 61),
			text: words(state, 400),
		})
		.collect();
	diagnostics.dropped = next(state) % 3;
	diagnostics.facts[1].1 = words(state, 200);
	let status = match next(state) % 3 {
		0 => String::new(),
		1 => copied(Ok(())),
		_ => copied(Err(std::io::Error::other(words(state, 300)))),
	};
	(s, size, section, diagnostics, status)
}

/// How far drawing `text` at `px` reaches, each advance rounded up as
/// drawing moves; the recorder's fonts are the bundled font alone.
fn reach(font: &Font, text: &str, px: f32) -> usize {
	text
		.chars()
		.map(|ch| font.metrics(ch, px).advance_width.ceil() as usize)
		.sum()
}

#[test]
fn every_line_and_shape_stays_inside_the_window() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	let mut state = 0x5eed;
	for round in 0..150 {
		let (s, (width, height), section, diagnostics, status) = case(&mut state);
		let none = &|_, _, _, _| false;
		let commands = draw_in(s, (width, height), &diagnostics, section, &status, none);
		for command in &commands {
			match command {
				Command::Text { at, text, px, .. } => {
					let right = at.0 + reach(&font, text, *px);
					assert!(right <= width - 20 * s, "{round}: {text:?} reaches {right}");
					let bottom = at.1 + (*px * 1.3) as usize;
					assert!(
						bottom <= height,
						"{round}: {text:?} reaches down to {bottom}"
					);
				}
				Command::Rect((x, y, w, h), _) | Command::RoundRect((x, y, w, h), ..) => {
					assert!(x + w <= width && y + h <= height, "{round}: {command:?}");
				}
				_ => {}
			}
		}
	}
}

#[test]
fn the_log_stays_in_its_well_and_the_well_above_the_footer() {
	let mut state = 0xd1a6;
	for round in 0..150 {
		let (s, (width, height), _, diagnostics, status) = case(&mut state);
		let none = &|_, _, _, _| false;
		let commands = draw_in(
			s,
			(width, height),
			&diagnostics,
			Section::Log,
			&status,
			none,
		);
		let well_bottom = height - WELL_BOTTOM * s;
		assert!(
			well_bottom + 16 * s <= height - 52 * s,
			"{round}: room above the footer"
		);
		let mut tops: Vec<usize> = commands
			.iter()
			.filter_map(|command| match command {
				Command::Text { at, mono: true, .. } => Some(at.1),
				_ => None,
			})
			.collect();
		tops.dedup();
		assert!(!tops.is_empty(), "{round}: the log always says something");
		assert!(
			tops.len() <= log_capacity(height, s),
			"{round}: {} lines",
			tops.len()
		);
		for top in tops {
			assert!(top >= WELL_TOP * s, "{round}: {top} above the well");
			assert!(
				top + LOG_LINE * s <= well_bottom,
				"{round}: {top} below the well"
			);
		}
	}
}

#[test]
fn the_log_shows_every_warning_that_fits_and_counts_the_rest() {
	let mut state = 0x10c;
	for round in 0..150 {
		let (s, size, _, diagnostics, _) = case(&mut state);
		let none = &|_, _, _, _| false;
		let commands = draw_in(s, size, &diagnostics, Section::Log, "", none);
		let messages: Vec<&str> = commands
			.iter()
			.filter_map(|command| match command {
				Command::Text {
					text,
					mono: true,
					color,
					..
				} if *color == FG => Some(text.as_str()),
				_ => None,
			})
			.collect();
		let (warnings, lines) = (diagnostics.warnings.len(), log_capacity(size.1, s));
		let hidden = diagnostics.dropped > 0 || warnings > lines;
		if warnings == 0 && diagnostics.dropped == 0 {
			assert_eq!(messages, ["No warnings."], "{round}");
		} else {
			assert_eq!(
				messages.len(),
				warnings.min(lines - usize::from(hidden)) + usize::from(hidden),
				"{round}"
			);
			assert_eq!(
				messages[0].ends_with("not shown"),
				hidden,
				"{round}: {messages:?}"
			);
		}
	}
}

#[test]
fn only_the_pages_colors_are_drawn() {
	let page = [
		FG,
		COLORS.muted,
		COLORS.accelerated,
		COLORS.software,
		COLORS.rule,
		COLORS.well,
		COLORS.page,
		COLORS.border,
	];
	let mut state = 0xc010;
	for round in 0..60 {
		let (s, size, section, diagnostics, status) = case(&mut state);
		let none = &|_, _, _, _| false;
		for command in draw_in(s, size, &diagnostics, section, &status, none) {
			let color = match command {
				Command::Text { color, .. } | Command::Rect(_, color) | Command::RoundRect(_, _, color) => {
					color
				}
				_ => continue,
			};
			assert!(page.contains(&color), "{round}: {color:06x} in {command:?}");
		}
	}
}
