//! Tests of the Diagnostics model: feature statuses and their reasons, the
//! clock, what is kept private, and the plain-text report.
use super::diagnostics::{Diagnostics, Drawing, Feature, Reason, clock, private, report, statuses};
use crate::warnings::{LONGEST, Warning};
use std::{
	path::{Path, PathBuf},
	time::Duration,
};

/// Every reason, and what each feature says for it.
fn reasons() -> [(Reason, &'static str); 4] {
	[
		(
			Reason::Asked,
			"renderer = \"software\" in config.toml or POOPTERM_RENDERER",
		),
		(
			Reason::Unsupported,
			"No GPU renderer for this platform or window system yet",
		),
		(
			Reason::Unavailable("no Vulkan loader".into()),
			"Vulkan unavailable: no Vulkan loader",
		),
		(
			Reason::Abandoned("device lost".into()),
			"The GPU failed while drawing: device lost",
		),
	]
}

#[test]
fn every_feature_is_listed_once_in_page_order() {
	let names: Vec<&str> = Feature::ALL.iter().map(|feature| feature.name()).collect();
	assert_eq!(
		names,
		["Grid", "Window chrome", "Overlays", "Images", "Wallpaper"]
	);
	let features: Vec<Feature> = statuses(&Drawing::Gpu).iter().map(|s| s.feature).collect();
	assert_eq!(features, Feature::ALL);
}

#[test]
fn on_the_gpu_every_feature_is_accelerated_and_says_how() {
	let details: Vec<(bool, String)> = statuses(&Drawing::Gpu)
		.into_iter()
		.map(|s| (s.accelerated, s.detail))
		.collect();
	let want = [
		"Vulkan; glyph outlines drawn in shaders",
		"Vulkan quads",
		"Selection, search and cursor as quads",
		"Sixel and kitty images as textures",
		"A texture",
	];
	assert_eq!(details, want.map(|detail| (true, detail.to_owned())));
}

#[test]
fn in_software_every_feature_gives_the_reason() {
	for (reason, said) in reasons() {
		let statuses = statuses(&Drawing::Software(reason));
		let features: Vec<Feature> = statuses.iter().map(|status| status.feature).collect();
		assert_eq!(features, Feature::ALL, "{said}");
		for status in statuses {
			assert!(!status.accelerated, "{said}");
			assert_eq!(status.detail, said);
		}
	}
}

#[test]
fn a_reason_from_an_error_stays_on_one_bounded_line() {
	let error = format!("bad\ncall {}", "x".repeat(2000));
	let [status, ..] = statuses(&Drawing::Software(Reason::Abandoned(error)));
	assert!(
		status
			.detail
			.starts_with("The GPU failed while drawing: bad�call x")
	);
	assert_eq!(status.detail.len(), LONGEST);
	let [status, ..] = statuses(&Drawing::Software(Reason::Unavailable(
		"a\u{1b}[2Jb".into(),
	)));
	assert_eq!(status.detail, "Vulkan unavailable: a�[2Jb");
}

#[test]
fn the_clock_is_utc_time_of_day() {
	assert_eq!(clock(Duration::ZERO), "00:00:00");
	assert_eq!(
		clock(Duration::from_secs(9 * 3600 + 41 * 60 + 2)),
		"09:41:02"
	);
	assert_eq!(clock(Duration::from_millis(86_399_999)), "23:59:59");
	assert_eq!(clock(Duration::from_secs(86_400)), "00:00:00");
	// 2026-11-02 11:30:18 UTC.
	assert_eq!(clock(Duration::from_secs(1_793_619_018)), "11:30:18");
}

#[test]
fn the_clock_never_goes_past_a_day() {
	for seconds in [59, 60, 3599, 3600, 86_399, 86_401, u64::MAX] {
		let shown = clock(Duration::from_secs(seconds));
		let parts: Vec<u64> = shown.split(':').map(|part| part.parse().unwrap()).collect();
		assert_eq!(shown.len(), 8, "{seconds}");
		assert!(
			parts[0] < 24 && parts[1] < 60 && parts[2] < 60,
			"{seconds}: {shown}"
		);
		assert_eq!(parts[0] * 3600 + parts[1] * 60 + parts[2], seconds % 86_400);
	}
}

#[test]
fn paths_in_the_home_directory_start_with_a_tilde() {
	let home = Some(Path::new("/home/vx"));
	assert_eq!(
		private(
			"wallpaper ignored: /home/vx/Pictures/x.png: not found",
			home
		),
		"wallpaper ignored: ~/Pictures/x.png: not found"
	);
	assert_eq!(
		private("from /home/vx to /home/vx/b", home),
		"from ~ to ~/b"
	);
}

#[test]
fn text_without_the_home_directory_is_left_alone() {
	let home = Some(Path::new("/home/vx"));
	let text = "font: no glyph for U+1FAE8 in /usr/share/fonts";
	assert_eq!(private(text, home), text);
	assert_eq!(private("/home/vx/a", None), "/home/vx/a");
	assert_eq!(private("", home), "");
}

#[test]
fn a_root_or_empty_home_is_not_abbreviated() {
	// Replacing "/" would turn every slash into a tilde.
	let text = "wallpaper ignored: /home/vx/x.png";
	assert_eq!(private(text, Some(Path::new("/"))), text);
	assert_eq!(private(text, Some(Path::new(""))), text);
}

fn sample() -> Diagnostics {
	Diagnostics {
		version: "26.11.28 \"Lunar Moth\"".into(),
		drawing: Drawing::Gpu,
		facts: vec![
			("Device", "AMD Radeon RX 7800 XT (RADV NAVI32)".into()),
			("Presenter", "Vulkan on Wayland".into()),
		],
		warnings: vec![
			Warning {
				at: Duration::from_secs(9 * 3600 + 41 * 60 + 2),
				text: "wallpaper ignored: /home/vx/Pictures/x.png: not found".into(),
			},
			Warning {
				at: Duration::from_secs(10 * 3600),
				text: "clipboard copy failed".into(),
			},
		],
		dropped: 0,
		home: Some(PathBuf::from("/home/vx")),
	}
}

#[test]
fn the_report_reads_as_plain_text_section_by_section() {
	let expected = "\
Poopterm 26.11.28 \"Lunar Moth\" diagnostics

Graphics
  Grid             Hardware accelerated: Vulkan; glyph outlines drawn in shaders
  Window chrome    Hardware accelerated: Vulkan quads
  Overlays         Hardware accelerated: Selection, search and cursor as quads
  Images           Hardware accelerated: Sixel and kitty images as textures
  Wallpaper        Hardware accelerated: A texture
  Device           AMD Radeon RX 7800 XT (RADV NAVI32)
  Presenter        Vulkan on Wayland

Log
  09:41:02 UTC     wallpaper ignored: ~/Pictures/x.png: not found
  10:00:00 UTC     clipboard copy failed
";
	assert_eq!(report(&sample()), expected);
}

#[test]
fn the_report_says_why_it_draws_in_software() {
	for (reason, said) in reasons() {
		let diagnostics = Diagnostics {
			drawing: Drawing::Software(reason),
			..sample()
		};
		let text = report(&diagnostics);
		for feature in Feature::ALL {
			let line = format!("  {:<17}Software only: {said}\n", feature.name());
			assert!(text.contains(&line), "{line}");
		}
		assert!(!text.contains("Hardware accelerated"));
	}
}

#[test]
fn a_label_as_long_as_the_column_still_leaves_no_gap_unfilled() {
	// Labels pad to 17 columns; a longer one pushes its value along.
	let diagnostics = Diagnostics {
		facts: vec![
			("Seventeen columns", "a".into()),
			("Eighteen  columns!", "b".into()),
		],
		..sample()
	};
	let text = report(&diagnostics);
	assert!(text.contains("\n  Seventeen columnsa\n"));
	assert!(text.contains("\n  Eighteen  columns!b\n"));
}

#[test]
fn without_facts_the_features_lead_straight_to_the_log() {
	let diagnostics = Diagnostics {
		facts: vec![],
		..sample()
	};
	assert!(report(&diagnostics).contains("Hardware accelerated: A texture\n\nLog\n"));
}

#[test]
fn an_empty_log_says_so() {
	let diagnostics = Diagnostics {
		warnings: vec![],
		..sample()
	};
	assert!(report(&diagnostics).ends_with("\nLog\n  No warnings.\n"));
}

#[test]
fn dropped_warnings_are_counted_before_the_kept_ones() {
	let diagnostics = Diagnostics {
		dropped: 3,
		..sample()
	};
	let text = report(&diagnostics);
	assert!(text.contains("\nLog\n  3 older warnings dropped\n  09:41:02 UTC"));
	assert!(!text.contains("No warnings"));
}

#[test]
fn one_dropped_warning_is_singular_and_not_no_warnings() {
	let one = Diagnostics {
		dropped: 1,
		warnings: vec![],
		..sample()
	};
	assert!(report(&one).ends_with("\nLog\n  1 older warning dropped\n"));
	let many = Diagnostics {
		dropped: usize::MAX,
		warnings: vec![],
		..sample()
	};
	assert!(report(&many).ends_with(&format!("\n  {} older warnings dropped\n", usize::MAX)));
}

#[test]
fn the_report_keeps_the_home_directory_out_only_when_it_knows_it() {
	assert!(!report(&sample()).contains("/home/vx"));
	let unknown = Diagnostics {
		home: None,
		..sample()
	};
	assert!(report(&unknown).contains("wallpaper ignored: /home/vx/Pictures/x.png"));
	let root = Diagnostics {
		home: Some(PathBuf::from("/")),
		..sample()
	};
	assert!(!report(&root).contains('~'));
}

#[test]
fn every_report_line_ends_in_a_newline_and_none_is_blank_but_the_separators() {
	let text = report(&sample());
	assert!(text.ends_with('\n'));
	let blank: Vec<usize> = text
		.lines()
		.enumerate()
		.filter(|(_, line)| line.is_empty())
		.map(|(i, _)| i)
		.collect();
	// After the title, and before the log.
	assert_eq!(blank, [1, 10]);
}

#[test]
fn only_whole_path_components_count_as_the_home_directory() {
	let home = Some(Path::new("/home/vx"));
	// Siblings that only start like the home directory stay as they are.
	for sibling in [
		"/home/vxx/a",
		"/home/vx.bak/a",
		"/home/vx-old",
		"/home/vx_2",
		"/home/vxé",
	] {
		assert_eq!(private(sibling, home), sibling);
	}
	// The home directory itself, at the end or before punctuation.
	assert_eq!(private("cannot read /home/vx", home), "cannot read ~");
	assert_eq!(
		private("cannot read /home/vx: denied", home),
		"cannot read ~: denied"
	);
	assert_eq!(private("\"/home/vx\"", home), "\"~\"");
	assert_eq!(
		private("/home/vxx and /home/vx/a", home),
		"/home/vxx and ~/a"
	);
}
