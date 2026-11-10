//! The Diagnostics page's model: how the window is drawn and why, and
//! recent warnings, built from plain data, and the plain-text report that
//! Copy report puts on the clipboard. Paths in it lose the home directory.
use super::recent_frames::FrameCounts;
use crate::warnings::{Warning, clean};
use std::{
	fmt::Write,
	path::{Path, PathBuf},
	time::Duration,
};

/// What the main window draws, one status each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
	Grid,
	Chrome,
	Overlays,
	Images,
	Wallpaper,
}

impl Feature {
	pub(crate) const ALL: [Self; 5] = [
		Self::Grid,
		Self::Chrome,
		Self::Overlays,
		Self::Images,
		Self::Wallpaper,
	];

	/// Each feature's name, and how the GPU draws it.
	const TEXT: [(&str, &str); 5] = [
		("Grid", "Vulkan; glyph outlines drawn in shaders"),
		("Window chrome", "Vulkan quads"),
		("Overlays", "Selection, search and cursor as quads"),
		("Images", "Sixel and kitty images as textures"),
		("Wallpaper", "A texture"),
	];

	pub(crate) fn name(self) -> &'static str {
		Self::TEXT[self as usize].0
	}

	fn on_gpu(self) -> &'static str {
		Self::TEXT[self as usize].1
	}
}

/// How the main window is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Drawing {
	Gpu,
	Software(Reason),
}

/// The GPU API this platform draws with.
pub(crate) const GPU_API: &str = if cfg!(target_os = "macos") {
	"Metal"
} else {
	"Vulkan"
};

/// Why the main window is drawn in software.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
	/// `renderer = "software"`, or `POOPTERM_RENDERER`.
	Asked,
	/// No GPU renderer for this platform or kind of window yet.
	Unsupported,
	/// The GPU renderer (Vulkan, or Metal on macOS) could not start, as
	/// said.
	Unavailable(String),
	/// The GPU failed while drawing, as said.
	Abandoned(String),
}

impl Reason {
	fn describe(&self) -> String {
		clean(&match self {
			Self::Asked => "renderer = \"software\" in config.toml or POOPTERM_RENDERER".into(),
			Self::Unsupported => "No GPU renderer for this platform or window system yet".into(),
			Self::Unavailable(error) => format!("{GPU_API} unavailable: {error}"),
			Self::Abandoned(error) => format!("The GPU failed while drawing: {error}"),
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Status {
	pub(crate) feature: Feature,
	pub(crate) accelerated: bool,
	pub(crate) detail: String,
}

impl Status {
	/// How the feature is drawn, in two words or three.
	pub(crate) fn how(&self) -> &'static str {
		if self.accelerated {
			"Hardware accelerated"
		} else {
			"Software only"
		}
	}
}

/// Each feature's status, in [`Feature::ALL`] order.
pub(crate) fn statuses(drawing: &Drawing) -> [Status; 5] {
	Feature::ALL.map(|feature| match drawing {
		Drawing::Gpu => Status {
			feature,
			accelerated: true,
			detail: feature.on_gpu().into(),
		},
		Drawing::Software(reason) => Status {
			feature,
			accelerated: false,
			detail: reason.describe(),
		},
	})
}

/// Everything the page shows and the report says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostics {
	pub(crate) version: String,
	pub(crate) drawing: Drawing,
	/// How the last frames were drawn.
	pub(crate) frames: FrameCounts,
	/// Graphics details after the features: device, presenter, frames.
	pub(crate) facts: Vec<(&'static str, String)>,
	pub(crate) warnings: Vec<Warning>,
	pub(crate) dropped: usize,
	pub(crate) home: Option<PathBuf>,
}

/// The UTC time of day of `at`, time since the Unix epoch.
pub(crate) fn clock(at: Duration) -> String {
	let seconds = at.as_secs() % 86_400;
	format!(
		"{:02}:{:02}:{:02}",
		seconds / 3600,
		seconds / 60 % 60,
		seconds % 60
	)
}

/// A warning's text with the `home` directory written `~` where it is a
/// whole path component: `/home/vxx` is not in `/home/vx`. A root home,
/// which every path starts with, is not one.
pub(crate) fn private(text: &str, home: Option<&Path>) -> String {
	let Some(home) = home.filter(|home| home.parent().is_some()) else {
		return text.to_owned();
	};
	let home = home.display().to_string();
	let (mut out, mut rest) = (String::with_capacity(text.len()), text);
	while let Some(at) = rest.find(&home) {
		let after = &rest[at + home.len()..];
		let longer = after.starts_with(|c: char| c.is_alphanumeric() || "-_.".contains(c));
		out.push_str(&rest[..at]);
		out.push_str(if longer { &home } else { "~" });
		rest = after;
	}
	out.push_str(rest);
	out
}

fn row(out: &mut String, label: &str, value: &str) {
	let _ = writeln!(out, "  {label:<17}{value}");
}

/// The plain-text report: every section, for pasting into an issue.
pub(crate) fn report(diagnostics: &Diagnostics) -> String {
	let mut out = format!("Poopterm {} diagnostics\n\nGraphics\n", diagnostics.version);
	for status in statuses(&diagnostics.drawing) {
		let how = status.how();
		row(
			&mut out,
			status.feature.name(),
			&format!("{how}: {}", status.detail),
		);
	}
	if let Some(line) = diagnostics.frames.line() {
		let _ = writeln!(out, "  {line}");
	}
	for (label, value) in &diagnostics.facts {
		row(&mut out, label, value);
	}
	out.push_str("\nLog\n");
	match diagnostics.dropped {
		0 if diagnostics.warnings.is_empty() => out.push_str("  No warnings.\n"),
		0 => {}
		1 => out.push_str("  1 older warning dropped\n"),
		dropped => {
			let _ = writeln!(out, "  {dropped} older warnings dropped");
		}
	}
	for warning in &diagnostics.warnings {
		let text = private(&warning.text, diagnostics.home.as_deref());
		row(&mut out, &format!("{} UTC", clock(warning.at)), &text);
	}
	out
}
