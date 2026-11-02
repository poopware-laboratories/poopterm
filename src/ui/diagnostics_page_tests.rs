//! Tests of the Diagnostics page's plain data: its sections, how a window
//! came to draw as it does, the log rows that fit, and Copy report's status.
use super::diagnostics::{Diagnostics, Drawing, Reason, report};
use super::diagnostics_page::{Section, copied, fit, gather, log_rows, started};
use crate::{config::RendererPreference, render::backend::Backend, warnings::Warning};
use std::{io, path::PathBuf, time::Duration};

fn warning(seconds: u64, text: &str) -> Warning {
	Warning {
		at: Duration::from_secs(seconds),
		text: text.into(),
	}
}

/// Diagnostics with `count` warnings a minute apart from 08:00:00 UTC, and
/// `dropped` older ones gone.
fn logged(count: usize, dropped: usize) -> Diagnostics {
	Diagnostics {
		version: "26.11.30".into(),
		drawing: Drawing::Gpu,
		facts: Vec::new(),
		warnings: (0..count)
			.map(|i| warning(8 * 3600 + 60 * i as u64, &format!("warning {i}")))
			.collect(),
		dropped,
		home: Some(PathBuf::from("/home/ada")),
	}
}

#[test]
fn the_page_switches_between_graphics_and_the_log_starting_on_graphics() {
	assert_eq!(Section::ALL, [Section::Graphics, Section::Log]);
	assert_eq!(Section::default(), Section::Graphics);
	let labels: Vec<&str> = Section::ALL.iter().map(|section| section.label()).collect();
	assert_eq!(labels, ["Graphics", "Log"]);
}

#[test]
fn a_window_that_started_on_the_gpu_draws_on_it_whatever_failed_before() {
	assert_eq!(started(Backend::Gpu, None, true), Drawing::Gpu);
	assert_eq!(started(Backend::Gpu, Some("ignored"), true), Drawing::Gpu);
}

#[test]
fn software_without_a_gpu_attempt_was_asked_for() {
	// `renderer = "software"` never tries the GPU, so nothing failed.
	for gpu_here in [true, false] {
		assert_eq!(
			started(Backend::Software, None, gpu_here),
			Drawing::Software(Reason::Asked)
		);
	}
}

#[test]
fn software_after_a_failed_gpu_says_why_where_there_is_a_gpu_renderer() {
	let why = "no Vulkan device suits: llvmpipe draws on the CPU, slower than software";
	assert_eq!(
		started(Backend::Software, Some(why), true),
		Drawing::Software(Reason::Unavailable(why.into()))
	);
}

#[test]
fn software_where_no_gpu_renderer_exists_is_unsupported_not_a_failure() {
	assert_eq!(
		started(
			Backend::Software,
			Some("not available on this system yet"),
			false
		),
		Drawing::Software(Reason::Unsupported)
	);
}

#[test]
fn text_that_fits_is_kept_whole() {
	assert_eq!(fit("", 0), "");
	assert_eq!(fit("", 5), "");
	assert_eq!(fit("abcde", 5), "abcde");
	assert_eq!(fit("abcde", 6), "abcde");
}

#[test]
fn text_too_long_ends_in_an_ellipsis_within_the_columns() {
	assert_eq!(fit("abcdef", 5), "abcd…");
	assert_eq!(fit("abcdef", 1), "…");
	assert_eq!(fit("abcdef", 0), "");
}

#[test]
fn fitting_counts_characters_not_bytes() {
	// Five characters of two, three and four bytes fit five columns.
	assert_eq!(fit("éé€🦀a", 5), "éé€🦀a");
	assert_eq!(fit("éé€🦀ab", 5), "éé€🦀…");
	assert_eq!(fit("€€€", 2), "€…");
}

#[test]
fn an_empty_log_says_so() {
	assert_eq!(log_rows(&logged(0, 0), 10, 80), ["No warnings."]);
	// Fitted like any row, in a window too narrow for it.
	assert_eq!(log_rows(&logged(0, 0), 10, 4), ["No …"]);
}

#[test]
fn a_log_that_fits_lists_every_warning_oldest_first_in_utc() {
	assert_eq!(
		log_rows(&logged(3, 0), 3, 80),
		[
			"08:00:00 UTC  warning 0",
			"08:01:00 UTC  warning 1",
			"08:02:00 UTC  warning 2",
		]
	);
}

#[test]
fn a_log_too_long_shows_the_newest_after_counting_the_rest() {
	assert_eq!(
		log_rows(&logged(5, 0), 3, 80),
		[
			"3 older warnings not shown",
			"08:03:00 UTC  warning 3",
			"08:04:00 UTC  warning 4",
		]
	);
	// One row short: the note takes one row, so two warnings are left out.
	assert_eq!(
		log_rows(&logged(4, 0), 3, 80)[0],
		"2 older warnings not shown"
	);
	assert_eq!(
		log_rows(&logged(2, 0), 1, 80),
		["2 older warnings not shown"]
	);
}

#[test]
fn dropped_warnings_are_counted_with_those_left_out() {
	assert_eq!(
		log_rows(&logged(2, 1), 3, 80),
		[
			"1 older warning not shown",
			"08:00:00 UTC  warning 0",
			"08:01:00 UTC  warning 1",
		]
	);
	assert_eq!(
		log_rows(&logged(3, 7), 3, 80),
		[
			"8 older warnings not shown",
			"08:01:00 UTC  warning 1",
			"08:02:00 UTC  warning 2",
		]
	);
	// Every warning dropped and none kept still says what happened.
	assert_eq!(
		log_rows(&logged(0, 4), 3, 80),
		["4 older warnings not shown"]
	);
}

#[test]
fn no_rows_shows_nothing() {
	assert!(log_rows(&logged(0, 0), 0, 80).is_empty());
	assert!(log_rows(&logged(3, 2), 0, 80).is_empty());
}

#[test]
fn log_rows_fit_the_columns_and_hide_the_home_directory() {
	let mut diagnostics = logged(0, 0);
	diagnostics
		.warnings
		.push(warning(0, "wallpaper ignored: /home/ada/Pictures/a.png"));
	assert_eq!(
		log_rows(&diagnostics, 5, 80),
		["00:00:00 UTC  wallpaper ignored: ~/Pictures/a.png"]
	);
	assert_eq!(log_rows(&diagnostics, 5, 24), ["00:00:00 UTC  wallpaper…"]);
	let rows = log_rows(&logged(9, 3), 4, 10);
	assert!(rows.iter().all(|row| row.chars().count() <= 10), "{rows:?}");
	// Three of nine kept warnings fit under the note, which counts the
	// other six and the three dropped.
	assert_eq!(rows[0], "9 older w…");
	assert_eq!(rows[3], "08:08:00 …");
}

#[test]
fn copy_report_says_whether_the_clipboard_took_it() {
	assert_eq!(
		copied(Ok(())),
		"Copied the report. Paste it into a bug report."
	);
	let failed = io::Error::new(io::ErrorKind::NotFound, "wl-copy not found");
	assert_eq!(
		copied(Err(failed)),
		"Could not copy the report: wl-copy not found"
	);
}

#[test]
fn renderer_preferences_name_themselves_as_config_toml_writes_them() {
	for preference in [
		RendererPreference::Auto,
		RendererPreference::Software,
		RendererPreference::Gpu,
	] {
		assert_eq!(
			RendererPreference::named(preference.name()),
			Some(preference)
		);
	}
	assert_eq!(RendererPreference::Gpu.name(), "gpu");
}

#[test]
fn gathered_diagnostics_name_the_release_setting_and_device() {
	let recent = (vec![warning(5, "drawing on RADV")], 2);
	let home = Some(PathBuf::from("/home/ada"));
	let diagnostics = gather(
		Drawing::Gpu,
		RendererPreference::Auto,
		Some("AMD Radeon (RADV NAVI32)"),
		recent.clone(),
		home.clone(),
	);
	let release = format!(
		"{} \"{}\"",
		env!("CARGO_PKG_VERSION"),
		env!("POOPTERM_CODENAME")
	);
	assert_eq!(
		diagnostics,
		Diagnostics {
			version: release,
			drawing: Drawing::Gpu,
			facts: vec![
				("Renderer setting", "auto".into()),
				("Device", "AMD Radeon (RADV NAVI32)".into()),
			],
			warnings: recent.0,
			dropped: 2,
			home,
		}
	);
}

#[test]
fn software_has_no_device_to_name() {
	let drawing = Drawing::Software(Reason::Asked);
	let diagnostics = gather(
		drawing.clone(),
		RendererPreference::Software,
		None,
		(Vec::new(), 0),
		None,
	);
	assert_eq!(diagnostics.drawing, drawing);
	assert_eq!(
		diagnostics.facts,
		[("Renderer setting", "software".to_owned())]
	);
	assert!(report(&diagnostics).contains("  Renderer setting software\n"));
}
