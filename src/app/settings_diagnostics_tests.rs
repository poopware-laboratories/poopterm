//! Tests of the Settings window's Diagnostics page: what a click lands on,
//! and what each section draws, where and in which color, read from the
//! recorded scene and checked against the approved design (#205).
use super::{
	settings_diagnostics::{
		BODY, COLUMNS, CONTENT_TOP, COPY_WIDTH, DEVICE_TOP, FACT_STRIDE, FEATURES_TOP, LOG_LINE, ROW,
		SMALL, WELL_BOTTOM, WELL_INSET, WELL_TOP, compose_diagnostics, diagnostics_target,
		log_capacity,
	},
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record, texts},
	settings_view::MIN_SIZE,
};
use crate::{
	fonts::{BUNDLED, Font},
	render::scene::{Command, Rect},
	ui::{
		diagnostics::{Diagnostics, Drawing, Reason, report},
		diagnostics_colors::DiagnosticsColors,
		diagnostics_page::{Section, copied},
	},
	warnings::Warning,
};
use std::time::Duration;

/// The recorder's window, in logical pixels, and its text color.
const SIZE: (usize, usize) = (900, 600);
const FG: u32 = 0xeeeeee;
/// Colors told apart from each other and from `FG`.
const COLORS: DiagnosticsColors = DiagnosticsColors {
	muted: 0x888888,
	accelerated: 0x22aa22,
	limited: 0xaaaa22,
	software: 0x777777,
	rule: 0x333333,
	well: 0x222222,
	page: 0x111111,
	border: 0x444444,
};
const NAMES: [&str; 5] = ["Grid", "Window chrome", "Overlays", "Images", "Wallpaper"];

/// One line of recorded text.
#[derive(Debug, PartialEq)]
struct Line {
	text: String,
	px: f32,
	color: u32,
	mono: bool,
}

fn sample(drawing: Drawing, warnings: usize) -> Diagnostics {
	Diagnostics {
		version: "26.11.30 \"Test\"".into(),
		drawing,
		facts: vec![
			("Renderer setting", "auto".into()),
			("Device", "AMD Radeon (RADV NAVI32)".into()),
		],
		warnings: (0..warnings)
			.map(|i| Warning {
				at: Duration::from_secs(60 * i as u64),
				text: format!("warning {i}"),
			})
			.collect(),
		dropped: 0,
		home: None,
	}
}

/// The page in a window `size` physical pixels large, nothing hovered
/// unless `hovered` says so.
fn draw_in(
	scale: usize,
	size: (usize, usize),
	diagnostics: &Diagnostics,
	section: Section,
	status: &str,
	hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
) -> Vec<Command> {
	record(scale, |pen, p, style| {
		(p.width, p.height) = size;
		compose_diagnostics(
			diagnostics,
			section,
			status,
			&COLORS,
			style,
			p,
			pen,
			hovered,
		)
	})
}

fn draw(scale: usize, diagnostics: &Diagnostics, section: Section, status: &str) -> Vec<Command> {
	let size = (SIZE.0 * scale, SIZE.1 * scale);
	draw_in(scale, size, diagnostics, section, status, &|_, _, _, _| {
		false
	})
}

/// The text drawn with its top-left corner at logical `(x, y)`.
fn text_at(commands: &[Command], scale: usize, (x, y): (usize, usize)) -> Option<String> {
	line_at(commands, scale, (x, y)).map(|line| line.text)
}

/// The line drawn with its top-left corner at logical `(x, y)`.
fn line_at(commands: &[Command], scale: usize, (x, y): (usize, usize)) -> Option<Line> {
	let at = (x * scale, y * scale);
	commands.iter().find_map(|command| match command {
		Command::Text {
			at: place,
			text,
			px,
			color,
			mono,
			..
		} if *place == at => Some(Line {
			text: text.clone(),
			px: *px / scale as f32,
			color: *color,
			mono: *mono,
		}),
		_ => None,
	})
}

fn round_rects(commands: &[Command]) -> Vec<(Rect, usize, u32)> {
	let round = |command: &Command| match *command {
		Command::RoundRect(rect, r, color) => Some((rect, r, color)),
		_ => None,
	};
	commands.iter().filter_map(round).collect()
}

fn rects(commands: &[Command]) -> Vec<(Rect, u32)> {
	let rect = |command: &Command| match *command {
		Command::Rect(rect, color) => Some((rect, color)),
		_ => None,
	};
	commands.iter().filter_map(rect).collect()
}

/// The design's px for a line `size` times the window's 18.
fn px(size: f32) -> f32 {
	18.0 * size
}

/// How wide the bundled font draws `chars` characters at `size`.
fn bundled_width(chars: usize, size: f32, scale: usize) -> f32 {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	chars as f32 * font.metrics('M', px(size) * scale as f32).advance_width
}

#[test]
fn clicks_land_on_back_the_sections_and_copy_report() {
	for s in [1, 2] {
		let target = |x: usize, y: usize| diagnostics_target((x * s, y * s), (760 * s, 640 * s), s);
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
		assert_eq!(
			target(30, 80),
			Some(SettingsTarget::Section(Section::Graphics))
		);
		assert_eq!(
			target(127, 107),
			Some(SettingsTarget::Section(Section::Graphics))
		);
		assert_eq!(target(140, 90), Some(SettingsTarget::Section(Section::Log)));
		assert_eq!(target(239, 90), Some(SettingsTarget::Section(Section::Log)));
		// The gap between the buttons, beside them and below them.
		assert_eq!(target(130, 90), None);
		assert_eq!(target(250, 90), None);
		assert_eq!(target(30, 75), None);
		assert_eq!(target(30, 108), None);
		assert_eq!(target(30, 200), None);
		// Copy report: 140 wide, 20 from the right edge, 16 from the bottom.
		assert_eq!(target(600, 588), Some(SettingsTarget::CopyReport));
		assert_eq!(target(739, 623), Some(SettingsTarget::CopyReport));
		assert_eq!(target(599, 600), None);
		assert_eq!(target(740, 600), None);
		assert_eq!(target(700, 624), None);
	}
}

#[test]
fn the_window_routes_clicks_on_the_diagnostics_page_to_it() {
	let target = |x, y| settings_target((x, y), (760, 640), 1, SettingsPage::Diagnostics);
	assert_eq!(target(690, 30), Some(SettingsTarget::Back));
	assert_eq!(target(140, 90), Some(SettingsTarget::Section(Section::Log)));
	assert_eq!(target(700, 600), Some(SettingsTarget::CopyReport));
	// Nothing of the main page answers here.
	assert_eq!(target(30, 240), None);
	assert_eq!(target(530, 280), None);
}

#[test]
fn the_page_has_a_large_title_a_lit_back_button_and_lights_the_section_shown() {
	for s in [1, 2] {
		for (lit, section) in Section::ALL.into_iter().enumerate() {
			let commands = draw(s, &sample(Drawing::Gpu, 0), section, "");
			let title = line_at(&commands, s, (24, 24)).unwrap();
			assert_eq!((title.text.as_str(), title.px), ("Diagnostics", 22.0));
			assert_eq!(title.color, FG);
			let buttons = buttons(&commands);
			let back = ((900 - 164) * s, 20 * s, 140 * s, 36 * s);
			assert_eq!(buttons[0], (back, true), "the way back stays lit");
			let graphics = (24 * s, 76 * s, 104 * s, 32 * s);
			let log = (136 * s, 76 * s, 104 * s, 32 * s);
			assert_eq!(buttons[1..3], [(graphics, lit == 0), (log, lit == 1)]);
			let labels = texts(&commands);
			for (rect, label) in [(graphics, "Graphics"), (log, "Log")] {
				let (_, at) = labels.iter().find(|(text, _)| text == label).unwrap();
				assert!(inside(*at, rect), "{label} at {at:?} outside {rect:?}");
			}
		}
	}
}

#[test]
fn a_hovered_section_button_lights_too() {
	let over_log = |x: usize, _: usize, _: usize, _: usize| x == 136;
	let commands = draw_in(
		1,
		SIZE,
		&sample(Drawing::Gpu, 0),
		Section::Graphics,
		"",
		&over_log,
	);
	let lit: Vec<bool> = buttons(&commands)[1..3]
		.iter()
		.map(|(_, lit)| *lit)
		.collect();
	assert_eq!(lit, [true, true]);
}

#[test]
fn graphics_on_the_gpu_marks_each_feature_with_a_full_ring_in_green() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Graphics, "");
		let (x, stroke) = (COLUMNS[1] * s, (3 * s).div_ceil(2));
		let marks: Vec<_> = (0..5)
			.flat_map(|i| {
				let y = (FEATURES_TOP + i * ROW + 11) * s;
				let inner = 12 * s - 2 * stroke;
				[
					((x, y, 12 * s, 12 * s), 6 * s, COLORS.accelerated),
					(
						(x + stroke, y + stroke, inner, inner),
						inner / 2,
						COLORS.page,
					),
					(
						(x + 3 * s, y + 3 * s, 6 * s, 6 * s),
						3 * s,
						COLORS.accelerated,
					),
				]
			})
			.collect();
		assert_eq!(round_rects(&commands), marks);
		for (i, name) in NAMES.into_iter().enumerate() {
			let top = FEATURES_TOP + i * ROW;
			let name_line = line_at(&commands, s, (24, top + 8)).unwrap();
			assert_eq!((name_line.text.as_str(), name_line.color), (name, FG));
			let how = line_at(&commands, s, (COLUMNS[1] + 20, top + 8)).unwrap();
			assert_eq!(how.text, "Hardware accelerated");
			assert_eq!((how.color, how.px), (COLORS.accelerated, px(BODY)));
		}
		let detail = line_at(&commands, s, (COLUMNS[2], FEATURES_TOP + 9)).unwrap();
		assert_eq!(detail.text, "Vulkan; glyph outlines drawn in shaders");
		assert_eq!((detail.color, detail.px), (COLORS.muted, px(SMALL)));
	}
}

#[test]
fn graphics_in_software_draws_empty_rings_and_the_reason() {
	let s = 1;
	let commands = draw(
		s,
		&sample(Drawing::Software(Reason::Asked), 0),
		Section::Graphics,
		"",
	);
	let marks = round_rects(&commands);
	assert_eq!(marks.len(), 10, "a ring and its inside for each feature");
	let top = FEATURES_TOP + 11;
	assert_eq!(
		marks[..2],
		[
			((156, top, 12, 12), 6, COLORS.software),
			((158, top + 2, 8, 8), 4, COLORS.page)
		]
	);
	let how = line_at(&commands, s, (176, FEATURES_TOP + 8)).unwrap();
	assert_eq!(
		(how.text.as_str(), how.color),
		("Software only", COLORS.software)
	);
	assert_eq!(
		text_at(&commands, s, (COLUMNS[2], FEATURES_TOP + 9)).as_deref(),
		Some("renderer = \"software\" in config.toml or POOPTERM_RENDERER")
	);
}

#[test]
fn a_rule_runs_under_every_feature_from_margin_to_margin() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Graphics, "");
		let rules: Vec<_> = (0..5)
			.map(|i| {
				let y = (FEATURES_TOP + (i + 1) * ROW - 1) * s;
				((24 * s, y, (900 - 48) * s, s), COLORS.rule)
			})
			.collect();
		assert_eq!(rects(&commands), rules);
	}
}

#[test]
fn a_long_reason_is_cut_to_the_window() {
	let error = "VkResult -4 ".repeat(40);
	let drawing = Drawing::Software(Reason::Abandoned(error));
	for width in [680, 900] {
		let commands = draw_in(
			1,
			(width, 600),
			&sample(drawing.clone(), 0),
			Section::Graphics,
			"",
			&|_, _, _, _| false,
		);
		let detail = text_at(&commands, 1, (COLUMNS[2], FEATURES_TOP + 9)).unwrap();
		assert!(
			detail.starts_with("The GPU failed while drawing"),
			"{detail}"
		);
		assert!(detail.ends_with('…'), "{detail}");
		let drawn = bundled_width(detail.chars().count(), SMALL, 1);
		assert!(384.0 + drawn <= (width - 24) as f32, "{width}: {drawn}");
	}
}

#[test]
fn graphics_heads_its_features_and_lists_the_device_below() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Graphics, "");
		for (top, heading) in [(CONTENT_TOP, "Features"), (DEVICE_TOP, "Device")] {
			let line = line_at(&commands, s, (24, top)).unwrap();
			assert_eq!(line.text, heading);
			assert_eq!((line.color, line.px), (COLORS.muted, px(SMALL)));
		}
		for (i, (label, value)) in [
			("Renderer setting", "auto"),
			("Device", "AMD Radeon (RADV NAVI32)"),
		]
		.into_iter()
		.enumerate()
		{
			let top = DEVICE_TOP + 24 + i * FACT_STRIDE;
			let label_line = line_at(&commands, s, (24, top)).unwrap();
			assert_eq!(
				(label_line.text.as_str(), label_line.color),
				(label, COLORS.muted)
			);
			let value_line = line_at(&commands, s, (COLUMNS[1], top)).unwrap();
			assert_eq!((value_line.text.as_str(), value_line.color), (value, FG));
		}
	}
}

#[test]
fn the_designs_five_device_facts_fit_the_smallest_window() {
	let bottom = DEVICE_TOP + 24 + 5 * FACT_STRIDE;
	assert!(bottom <= MIN_SIZE.1 - 64, "{bottom}");
	let gap = DEVICE_TOP - (FEATURES_TOP + 5 * ROW);
	assert!(gap >= 26, "room for the line under the features: {gap}");
}

#[test]
fn graphics_draws_no_log_and_the_log_draws_no_graphics() {
	let diagnostics = sample(Drawing::Gpu, 2);
	let graphics = draw(1, &diagnostics, Section::Graphics, "");
	assert!(
		!texts(&graphics)
			.iter()
			.any(|(text, _)| text.contains("warning 1"))
	);
	assert!(
		!graphics
			.iter()
			.any(|c| matches!(c, Command::Text { mono: true, .. }))
	);
	let log = draw(1, &diagnostics, Section::Log, "");
	assert!(rects(&log).is_empty(), "no rules");
	assert!(!texts(&log).iter().any(|(text, _)| text == "Grid"));
}

#[test]
fn the_log_opens_with_its_intro_above_a_bordered_well() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 2), Section::Log, "");
		let intro = line_at(&commands, s, (24, CONTENT_TOP)).unwrap();
		assert!(
			intro.text.starts_with("Warnings Poopterm printed"),
			"{intro:?}"
		);
		assert_eq!((intro.color, intro.px), (COLORS.muted, px(SMALL)));
		let (w, h) = (900 - 48, 600 - WELL_TOP - WELL_BOTTOM);
		assert_eq!(
			round_rects(&commands),
			[
				((24 * s, WELL_TOP * s, w * s, h * s), 6 * s, COLORS.border),
				(
					(25 * s, (WELL_TOP + 1) * s, (w - 2) * s, (h - 2) * s),
					5 * s,
					COLORS.well
				),
			]
		);
	}
}

#[test]
fn the_log_lists_each_warning_in_mono_with_its_time_muted() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 2), Section::Log, "");
		let (x, top) = (24 + WELL_INSET.0, WELL_TOP + WELL_INSET.1);
		for (i, time) in ["00:00:00 UTC", "00:01:00 UTC"].into_iter().enumerate() {
			let y = top + i * LOG_LINE;
			let time_line = line_at(&commands, s, (x, y)).unwrap();
			let expected = Line {
				text: time.into(),
				px: px(SMALL) * 12.0 / 13.0,
				color: COLORS.muted,
				mono: true,
			};
			assert_eq!(time_line, expected);
			// The message starts 12 past the time.
			let font = Font::from_bytes(BUNDLED, 0).unwrap();
			let advance = font.metrics('M', 12.0 * s as f32).advance_width;
			let time_w = 12 * advance.ceil() as usize;
			let message = commands.iter().find_map(|command| match command {
				Command::Text {
					at,
					text,
					color,
					mono: true,
					..
				} if *at == (x * s + time_w + 12 * s, y * s) => Some((text.clone(), *color)),
				_ => None,
			});
			assert_eq!(message, Some((format!("warning {i}"), FG)), "x{s} line {i}");
		}
	}
}

#[test]
fn a_long_log_fills_the_well_and_stops_inside_it() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 40), Section::Log, "");
		let lines = log_capacity(SIZE.1 * s, s);
		assert_eq!(lines, 15);
		let (x, top) = (24 + WELL_INSET.0, WELL_TOP + WELL_INSET.1);
		assert_eq!(
			text_at(&commands, s, (x, top)).as_deref(),
			Some("26 older warnings not shown")
		);
		let last = top + (lines - 1) * LOG_LINE;
		assert_eq!(
			text_at(&commands, s, (x, last)).as_deref(),
			Some("00:39:00 UTC")
		);
		assert_eq!(text_at(&commands, s, (x, last + LOG_LINE)), None);
		assert!(last + LOG_LINE + WELL_INSET.1 <= SIZE.1 - WELL_BOTTOM);
	}
}

#[test]
fn log_capacity_counts_the_lines_the_well_holds() {
	assert_eq!(log_capacity(640, 1), 17, "the design's well holds 17");
	assert_eq!(log_capacity(MIN_SIZE.1, 1), 15);
	assert_eq!(log_capacity(1280, 2), 17);
	let empty = WELL_TOP + WELL_BOTTOM + 2 * WELL_INSET.1;
	assert_eq!(log_capacity(empty + LOG_LINE - 1, 1), 0);
	assert_eq!(log_capacity(empty + LOG_LINE, 1), 1);
	assert_eq!(log_capacity(10, 1), 0);
	assert_eq!(log_capacity(640, 0), 17, "a zero scale counts as 1");
}

#[test]
fn an_empty_log_says_so_on_its_first_line() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Log, "");
		let (x, top) = (24 + WELL_INSET.0, WELL_TOP + WELL_INSET.1);
		let line = line_at(&commands, s, (x, top)).unwrap();
		assert_eq!(
			(line.text.as_str(), line.color, line.mono),
			("No warnings.", FG, true)
		);
		assert_eq!(text_at(&commands, s, (x, top + LOG_LINE)), None);
	}
}

#[test]
fn long_log_messages_are_cut_inside_the_well_at_every_scale() {
	let mut diagnostics = sample(Drawing::Gpu, 0);
	diagnostics.warnings.push(Warning {
		at: Duration::ZERO,
		text: "gpu renderer failed: ".repeat(20),
	});
	for s in [1, 2] {
		let size = (MIN_SIZE.0 * s, MIN_SIZE.1 * s);
		let commands = draw_in(s, size, &diagnostics, Section::Log, "", &|_, _, _, _| false);
		let message = texts(&commands)
			.into_iter()
			.find(|(text, _)| text.starts_with("gpu renderer failed"))
			.unwrap();
		assert!(message.0.ends_with('…'), "{message:?}");
		let drawn = bundled_width(message.0.chars().count(), SMALL * 12.0 / 13.0, s);
		let well_right = ((MIN_SIZE.0 - 24 - WELL_INSET.0 + 1) * s) as f32;
		assert!(message.1.0 as f32 + drawn <= well_right, "x{s}: {drawn}");
	}
}

#[test]
fn the_footer_shows_the_sections_footnote_until_a_copy() {
	for s in [1, 2] {
		for (section, footnote) in [
			(Section::Graphics, "Updated each time the page opens."),
			(Section::Log, "Environment variables appear by name only."),
		] {
			let commands = draw(s, &sample(Drawing::Gpu, 0), section, "");
			let copy = ((900 - 160) * s, (600 - 52) * s, COPY_WIDTH * s, 36 * s);
			assert_eq!(buttons(&commands).last(), Some(&(copy, false)));
			let labels = texts(&commands);
			let (_, at) = labels
				.iter()
				.find(|(text, _)| text == "Copy report")
				.unwrap();
			assert!(inside(*at, copy), "x{s}: {labels:?}");
			let line = line_at(&commands, s, (24, 600 - 44)).unwrap();
			assert_eq!((line.text.as_str(), line.color), (footnote, COLORS.muted));
			assert_eq!(line.px, 12.5);
		}
	}
}

#[test]
fn after_a_copy_the_button_says_copied_and_the_footer_says_what() {
	let status = copied(Ok(()));
	let commands = draw(1, &sample(Drawing::Gpu, 0), Section::Graphics, &status);
	assert!(texts(&commands).iter().any(|(text, _)| text == "Copied"));
	assert!(
		!texts(&commands)
			.iter()
			.any(|(text, _)| text == "Copy report")
	);
	let line = line_at(&commands, 1, (24, 600 - 44)).unwrap();
	assert_eq!(
		(line.text.as_str(), line.color),
		(
			"Copied: every section as plain text, ready to paste into an issue.",
			COLORS.muted
		)
	);
}

#[test]
fn a_failed_copy_offers_to_retry_and_says_why_in_the_text_color() {
	let status = copied(Err(std::io::Error::other("no clipboard")));
	let commands = draw(1, &sample(Drawing::Gpu, 0), Section::Log, &status);
	assert!(
		texts(&commands)
			.iter()
			.any(|(text, _)| text == "Copy report")
	);
	let line = line_at(&commands, 1, (24, 600 - 44)).unwrap();
	assert_eq!(
		(line.text.as_str(), line.color),
		("Could not copy the report: no clipboard", FG)
	);
}

#[test]
fn every_button_drawn_is_the_one_a_click_there_lands_on() {
	let expected = [
		SettingsTarget::Back,
		SettingsTarget::Section(Section::Graphics),
		SettingsTarget::Section(Section::Log),
		SettingsTarget::CopyReport,
	];
	for s in [1, 2, 3] {
		for (width, height) in [MIN_SIZE, (760, 640), (1300, 900)] {
			let size = (width * s, height * s);
			let none = &|_, _, _, _| false;
			let commands = draw_in(s, size, &sample(Drawing::Gpu, 3), Section::Log, "", none);
			let drawn: Vec<_> = buttons(&commands)
				.into_iter()
				.map(|(rect, _)| rect)
				.collect();
			assert_eq!(drawn.len(), expected.len(), "x{s} {size:?}");
			// Every few pixels of the window, at most one button and the target
			// it stands for.
			for y in (0..size.1).step_by(3) {
				for x in (0..size.0).step_by(3) {
					let under = drawn.iter().position(|&rect| inside((x, y), rect));
					assert_eq!(
						diagnostics_target((x, y), size, s),
						under.map(|index| expected[index]),
						"x{s} {size:?} at ({x}, {y})"
					);
				}
			}
		}
	}
}

#[test]
fn graphics_says_of_each_feature_what_the_copied_report_says() {
	for drawing in [
		Drawing::Gpu,
		Drawing::Software(Reason::Asked),
		Drawing::Software(Reason::Unsupported),
		Drawing::Software(Reason::Unavailable("no Vulkan loader".into())),
		Drawing::Software(Reason::Abandoned("device lost".into())),
	] {
		let diagnostics = sample(drawing, 0);
		let none = &|_, _, _, _| false;
		let commands = draw_in(1, (1300, 600), &diagnostics, Section::Graphics, "", none);
		let report = report(&diagnostics);
		for i in 0..5 {
			let top = FEATURES_TOP + i * ROW;
			let [name, how, detail] = [(24, top + 8), (176, top + 8), (384, top + 9)]
				.map(|at| text_at(&commands, 1, at).unwrap());
			let line = format!("\n  {name:<17}{how}: {detail}\n");
			assert!(report.contains(&line), "{line:?} not in\n{report}");
		}
	}
}

#[test]
fn a_long_fact_is_cut_to_the_smallest_window() {
	let mut diagnostics = sample(Drawing::Gpu, 0);
	diagnostics.facts[1].1 = "Radeon ".repeat(30);
	let none = &|_, _, _, _| false;
	let commands = draw_in(1, MIN_SIZE, &diagnostics, Section::Graphics, "", none);
	let first = DEVICE_TOP + 24;
	let value = text_at(&commands, 1, (COLUMNS[1], first + FACT_STRIDE)).unwrap();
	assert!(
		value.starts_with("Radeon Radeon") && value.ends_with('…'),
		"{value}"
	);
	let drawn = bundled_width(value.chars().count(), BODY, 1);
	assert!(156.0 + drawn <= (MIN_SIZE.0 - 24) as f32, "{drawn}");
	// The short fact above it is drawn whole.
	assert_eq!(
		text_at(&commands, 1, (COLUMNS[1], first)).as_deref(),
		Some("auto")
	);
}

#[test]
fn hovering_copy_report_lights_it() {
	let over_copy = |x: usize, _: usize, _: usize, _: usize| x == 900 - 160;
	let commands = draw_in(
		1,
		SIZE,
		&sample(Drawing::Gpu, 0),
		Section::Log,
		"",
		&over_copy,
	);
	let lights: Vec<bool> = buttons(&commands).iter().map(|(_, on)| *on).collect();
	assert_eq!(lights, [true, false, true, true]);
}
