//! Tests of the Settings window's Diagnostics page: what a click lands on,
//! and what each section draws, read from the recorded scene.
use super::{
	settings_diagnostics::{
		CONTENT_TOP, COPY_BUTTON, FACT_STRIDE, FACTS_TOP, LOG_STRIDE, NOTE, STATUS_STRIDE,
		compose_diagnostics, diagnostics_target, log_capacity,
	},
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record, texts},
	settings_view::MIN_SIZE,
};
use crate::{
	fonts::{BUNDLED, Font},
	render::scene::Command,
	ui::{
		diagnostics::{Diagnostics, Drawing, Reason, report},
		diagnostics_page::Section,
	},
	warnings::Warning,
};
use std::time::Duration;

/// The recorder's window, in logical pixels.
const SIZE: (usize, usize) = (900, 600);
const FG: u32 = 0xeeeeee;

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

fn draw(scale: usize, diagnostics: &Diagnostics, section: Section, status: &str) -> Vec<Command> {
	record(scale, |pen, p, style| {
		compose_diagnostics(
			diagnostics,
			section,
			status,
			style,
			p,
			pen,
			&|_, _, _, _| false,
		)
	})
}

/// The text drawn with its top-left corner at logical `(x, y)`.
fn text_at(commands: &[Command], scale: usize, (x, y): (usize, usize)) -> Option<String> {
	let at = (x * scale, y * scale);
	texts(commands)
		.into_iter()
		.find(|(_, place)| *place == at)
		.map(|(text, _)| text)
}

fn rects(commands: &[Command]) -> Vec<(usize, usize, usize, usize)> {
	let rect = |command: &Command| match *command {
		Command::Rect(rect, color) if color == FG => Some(rect),
		_ => None,
	};
	commands.iter().filter_map(rect).collect()
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
		// Copy report replaces Save: 140 wide, 20 from the right edge.
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
fn the_page_has_a_title_a_back_button_and_lights_the_section_shown() {
	for s in [1, 2] {
		for (lit, section) in Section::ALL.into_iter().enumerate() {
			let commands = draw(s, &sample(Drawing::Gpu, 0), section, "");
			assert_eq!(
				text_at(&commands, s, (24, 28)).as_deref(),
				Some("Diagnostics")
			);
			let buttons = buttons(&commands);
			let back = ((900 - 164) * s, 20 * s, 140 * s, 36 * s);
			assert_eq!(buttons[0], (back, false));
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
	let diagnostics = sample(Drawing::Gpu, 0);
	let commands = record(1, |pen, p, style| {
		let over_log = |x: usize, _: usize, _: usize, _: usize| x == 136;
		compose_diagnostics(
			&diagnostics,
			Section::Graphics,
			"",
			style,
			p,
			pen,
			&over_log,
		)
	});
	let lit: Vec<bool> = buttons(&commands)[1..3]
		.iter()
		.map(|(_, lit)| *lit)
		.collect();
	assert_eq!(lit, [true, true]);
}

#[test]
fn graphics_on_the_gpu_fills_every_mark_and_says_how() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Graphics, "");
		let marks: Vec<_> = (0..5)
			.map(|i| {
				(
					24 * s,
					(CONTENT_TOP + i * STATUS_STRIDE + 5) * s,
					12 * s,
					12 * s,
				)
			})
			.collect();
		assert_eq!(rects(&commands), marks);
		for (i, name) in ["Grid", "Window chrome", "Overlays", "Images", "Wallpaper"]
			.into_iter()
			.enumerate()
		{
			let top = CONTENT_TOP + i * STATUS_STRIDE;
			assert_eq!(text_at(&commands, s, (48, top)).as_deref(), Some(name));
			assert_eq!(
				text_at(&commands, s, (220, top)).as_deref(),
				Some("Hardware accelerated")
			);
		}
		let grid_detail = text_at(&commands, s, (220, CONTENT_TOP + 24));
		assert_eq!(
			grid_detail.as_deref(),
			Some("Vulkan; glyph outlines drawn in shaders")
		);
	}
}

#[test]
fn graphics_in_software_draws_hollow_marks_and_the_reason() {
	let s = 1;
	let drawing = Drawing::Software(Reason::Asked);
	let commands = draw(s, &sample(drawing, 0), Section::Graphics, "");
	let rects = rects(&commands);
	// Four sides each, none of them a filled square.
	assert_eq!(rects.len(), 20);
	let top = CONTENT_TOP + 5;
	assert_eq!(
		rects[..4],
		[
			(24, top, 12, 2),
			(24, top + 10, 12, 2),
			(24, top, 2, 12),
			(34, top, 2, 12)
		]
	);
	assert!(rects.iter().all(|&(_, _, w, h)| w == 2 || h == 2));
	assert_eq!(
		text_at(&commands, s, (220, CONTENT_TOP)).as_deref(),
		Some("Software only")
	);
	assert_eq!(
		text_at(&commands, s, (220, CONTENT_TOP + 24)).as_deref(),
		Some("renderer = \"software\" in config.toml or POOPTERM_RENDERER")
	);
}

#[test]
fn a_long_reason_is_cut_to_the_window() {
	let error = "VkResult -4 ".repeat(40);
	let drawing = Drawing::Software(Reason::Abandoned(error));
	for width in [680, 900] {
		let commands = record(1, |pen, p, style| {
			p.width = width;
			compose_diagnostics(
				&sample(drawing.clone(), 0),
				Section::Graphics,
				"",
				style,
				p,
				pen,
				&|_, _, _, _| false,
			)
		});
		let detail = text_at(&commands, 1, (220, CONTENT_TOP + 24)).unwrap();
		assert!(detail.starts_with("The GPU failed while drawing: VkResult -4"));
		assert!(detail.ends_with('…'), "{detail}");
		// The bundled font is monospaced: every character is as wide as 'M'.
		let font = Font::from_bytes(BUNDLED, 0).unwrap();
		let advance = font.metrics('M', 18.0 * 0.68).advance_width;
		let drawn = detail.chars().count() as f32 * advance;
		assert!(220.0 + drawn <= (width - 24) as f32, "{width}: {drawn}");
	}
}

#[test]
fn graphics_lists_its_facts_below_the_features() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Graphics, "");
		let first = CONTENT_TOP + 5 * STATUS_STRIDE + 8;
		for (i, (label, value)) in [
			("Renderer setting", "auto"),
			("Device", "AMD Radeon (RADV NAVI32)"),
		]
		.into_iter()
		.enumerate()
		{
			let top = first + i * FACT_STRIDE;
			assert_eq!(text_at(&commands, s, (48, top)).as_deref(), Some(label));
			assert_eq!(text_at(&commands, s, (220, top)).as_deref(), Some(value));
		}
	}
}

#[test]
fn graphics_with_two_facts_fits_the_smallest_window() {
	let bottom = CONTENT_TOP + 5 * STATUS_STRIDE + 8 + 2 * FACT_STRIDE;
	assert!(bottom <= MIN_SIZE.1 - 64, "{bottom}");
}

#[test]
fn graphics_draws_no_log_and_the_log_draws_no_graphics() {
	let diagnostics = sample(Drawing::Gpu, 2);
	let graphics = texts(&draw(1, &diagnostics, Section::Graphics, ""));
	assert!(!graphics.iter().any(|(text, _)| text.contains("warning 1")));
	let log = draw(1, &diagnostics, Section::Log, "");
	assert!(rects(&log).is_empty());
	assert!(!texts(&log).iter().any(|(text, _)| text == "Grid"));
}

#[test]
fn the_log_lists_warnings_a_row_apart() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 2), Section::Log, "");
		assert_eq!(
			text_at(&commands, s, (24, CONTENT_TOP)).as_deref(),
			Some("00:00:00 UTC  warning 0")
		);
		assert_eq!(
			text_at(&commands, s, (24, CONTENT_TOP + LOG_STRIDE)).as_deref(),
			Some("00:01:00 UTC  warning 1")
		);
	}
}

#[test]
fn a_long_log_fills_the_page_down_to_the_footer() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 40), Section::Log, "");
		let rows = log_capacity(SIZE.1 * s, s);
		assert_eq!(rows, 17);
		let last = CONTENT_TOP + (rows - 1) * LOG_STRIDE;
		assert_eq!(
			text_at(&commands, s, (24, CONTENT_TOP)).as_deref(),
			Some("24 older warnings not shown")
		);
		assert_eq!(
			text_at(&commands, s, (24, last)).as_deref(),
			Some("00:39:00 UTC  warning 39")
		);
		assert_eq!(text_at(&commands, s, (24, last + LOG_STRIDE)), None);
		assert!((last + LOG_STRIDE) <= SIZE.1 - 64);
	}
}

#[test]
fn log_capacity_counts_rows_between_the_switch_and_the_footer() {
	assert_eq!(log_capacity(640, 1), 18);
	assert_eq!(log_capacity(MIN_SIZE.1, 1), 17);
	assert_eq!(log_capacity(1280, 2), 18);
	assert_eq!(log_capacity(CONTENT_TOP + 64 + LOG_STRIDE - 1, 1), 0);
	assert_eq!(log_capacity(CONTENT_TOP + 64 + LOG_STRIDE, 1), 1);
	assert_eq!(log_capacity(10, 1), 0);
	assert_eq!(log_capacity(640, 0), 18, "a zero scale counts as 1");
}

#[test]
fn the_footer_offers_copy_report_and_the_note_until_copied() {
	for s in [1, 2] {
		for (status, shown) in [("", NOTE), ("Copied the report.", "Copied the report.")] {
			let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Log, status);
			let copy = ((900 - 160) * s, (600 - 52) * s, COPY_BUTTON.1 * s, 36 * s);
			assert_eq!(buttons(&commands).last(), Some(&(copy, false)));
			let labels = texts(&commands);
			let (_, at) = labels
				.iter()
				.find(|(text, _)| text == "Copy report")
				.unwrap();
			assert!(inside(*at, copy), "x{s}: {labels:?}");
			assert_eq!(
				text_at(&commands, s, (24, 600 - 40)).as_deref(),
				Some(shown)
			);
		}
	}
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
			let commands = record(s, |pen, p, style| {
				(p.width, p.height) = size;
				let none = &|_, _, _, _| false;
				compose_diagnostics(
					&sample(Drawing::Gpu, 3),
					Section::Log,
					"",
					style,
					p,
					pen,
					none,
				)
			});
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
		let commands = record(1, |pen, p, style| {
			p.width = 1300;
			let none = &|_, _, _, _| false;
			compose_diagnostics(&diagnostics, Section::Graphics, "", style, p, pen, none)
		});
		let report = report(&diagnostics);
		for i in 0..5 {
			let top = CONTENT_TOP + i * STATUS_STRIDE;
			let [name, how, detail] =
				[(48, top), (220, top), (220, top + 24)].map(|at| text_at(&commands, 1, at).unwrap());
			let line = format!("\n  {name:<17}{how}: {detail}\n");
			assert!(report.contains(&line), "{line:?} not in\n{report}");
		}
	}
}

#[test]
fn a_long_fact_is_cut_to_the_smallest_window() {
	let mut diagnostics = sample(Drawing::Gpu, 0);
	diagnostics.facts[1].1 = "Radeon ".repeat(30);
	let commands = record(1, |pen, p, style| {
		(p.width, p.height) = MIN_SIZE;
		let none = &|_, _, _, _| false;
		compose_diagnostics(&diagnostics, Section::Graphics, "", style, p, pen, none)
	});
	let value = text_at(&commands, 1, (220, FACTS_TOP + FACT_STRIDE)).unwrap();
	assert!(
		value.starts_with("Radeon Radeon") && value.ends_with('…'),
		"{value}"
	);
	let advance = Font::from_bytes(BUNDLED, 0)
		.unwrap()
		.metrics('M', 18.0 * 0.76)
		.advance_width;
	let drawn = value.chars().count() as f32 * advance;
	assert!(220.0 + drawn <= (MIN_SIZE.0 - 24) as f32, "{drawn}");
	// The short fact above it is drawn whole.
	assert_eq!(
		text_at(&commands, 1, (220, FACTS_TOP)).as_deref(),
		Some("auto")
	);
}

#[test]
fn hovering_back_or_copy_report_lights_only_that_button() {
	let diagnostics = sample(Drawing::Gpu, 0);
	for (hover_x, lit) in [(900 - 164, 0), (900 - 160, 3)] {
		let commands = record(1, |pen, p, style| {
			let over = |x: usize, _: usize, _: usize, _: usize| x == hover_x;
			compose_diagnostics(&diagnostics, Section::Log, "", style, p, pen, &over)
		});
		let lights: Vec<bool> = buttons(&commands).iter().map(|(_, on)| *on).collect();
		let mut expected = [false, false, true, false];
		expected[lit] = true;
		assert_eq!(lights, expected, "hovering x {hover_x}");
	}
}

#[test]
fn an_empty_log_says_so_where_the_rows_start() {
	for s in [1, 2] {
		let commands = draw(s, &sample(Drawing::Gpu, 0), Section::Log, "");
		assert_eq!(
			text_at(&commands, s, (24, CONTENT_TOP)).as_deref(),
			Some("No warnings.")
		);
		assert_eq!(text_at(&commands, s, (24, CONTENT_TOP + LOG_STRIDE)), None);
	}
}

#[test]
fn long_log_rows_are_cut_to_the_window_at_every_scale() {
	let mut diagnostics = sample(Drawing::Gpu, 0);
	diagnostics.warnings.push(Warning {
		at: Duration::ZERO,
		text: "gpu renderer failed: ".repeat(20),
	});
	for s in [1, 2] {
		let commands = record(s, |pen, p, style| {
			(p.width, p.height) = (MIN_SIZE.0 * s, MIN_SIZE.1 * s);
			let none = &|_, _, _, _| false;
			compose_diagnostics(&diagnostics, Section::Log, "", style, p, pen, none)
		});
		let row = text_at(&commands, s, (24, CONTENT_TOP)).unwrap();
		assert!(row.starts_with("00:00:00 UTC  gpu renderer failed") && row.ends_with('…'));
		let font = Font::from_bytes(BUNDLED, 0).unwrap();
		let advance = font.metrics('M', 18.0 * s as f32 * 0.72).advance_width;
		let drawn = row.chars().count() as f32 * advance;
		assert!(
			24.0 * s as f32 + drawn <= ((MIN_SIZE.0 - 24) * s) as f32,
			"x{s}: {drawn}"
		);
	}
}
