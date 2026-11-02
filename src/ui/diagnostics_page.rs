//! The Diagnostics page as plain data: its sections, how a window came to
//! draw as it does, the log rows that fit, and what Copy report says.
use super::diagnostics::{Diagnostics, Drawing, Reason, clock, private};
use crate::{config::RendererPreference, render::backend::Backend, warnings::Warning};
use std::{io, path::PathBuf};

/// The page's sections, which its switch shows one at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
	#[default]
	Graphics,
	Log,
}

impl Section {
	pub(crate) const ALL: [Self; 2] = [Self::Graphics, Self::Log];

	pub(crate) fn label(self) -> &'static str {
		match self {
			Self::Graphics => "Graphics",
			Self::Log => "Log",
		}
	}
}

/// How a window draws after starting on `chosen`, once the GPU, if tried,
/// failed with `gpu_failure`; `gpu_here` says whether this build has a GPU
/// renderer at all.
pub(crate) fn started(chosen: Backend, gpu_failure: Option<&str>, gpu_here: bool) -> Drawing {
	match (chosen, gpu_failure) {
		(Backend::Gpu, _) => Drawing::Gpu,
		(Backend::Software, None) => Drawing::Software(Reason::Asked),
		(Backend::Software, Some(_)) if !gpu_here => Drawing::Software(Reason::Unsupported),
		(Backend::Software, Some(error)) => Drawing::Software(Reason::Unavailable(error.into())),
	}
}

/// `text` in at most `columns` characters, ending in `…` when cut.
pub(crate) fn fit(text: &str, columns: usize) -> String {
	if text.chars().count() <= columns {
		return text.to_owned();
	}
	let kept = text.chars().take(columns.saturating_sub(1));
	kept.chain((columns > 0).then_some('…')).collect()
}

/// The log in at most `rows` rows of `columns` characters: the newest
/// warnings, oldest first, under a line counting those left out.
pub(crate) fn log_rows(diagnostics: &Diagnostics, rows: usize, columns: usize) -> Vec<String> {
	let warnings = &diagnostics.warnings;
	if rows == 0 {
		return Vec::new();
	}
	if warnings.is_empty() && diagnostics.dropped == 0 {
		return vec!["No warnings.".into()];
	}
	let noted = diagnostics.dropped > 0 || warnings.len() > rows;
	let shown = warnings.len().min(rows - usize::from(noted));
	let hidden = warnings.len() - shown + diagnostics.dropped;
	let note = match hidden {
		1 => "1 older warning not shown".to_owned(),
		hidden => format!("{hidden} older warnings not shown"),
	};
	let home = diagnostics.home.as_deref();
	let row = |warning: &Warning| {
		let text = private(&warning.text, home);
		format!("{} UTC  {text}", clock(warning.at))
	};
	let newest = warnings[warnings.len() - shown..].iter().map(row);
	noted
		.then_some(note)
		.into_iter()
		.chain(newest)
		.map(|line| fit(&line, columns))
		.collect()
}

/// The footer's status after Copy report wrote to the clipboard.
pub(crate) fn copied(written: io::Result<()>) -> String {
	match written {
		Ok(()) => "Copied the report. Paste it into a bug report.".into(),
		Err(error) => format!("Could not copy the report: {error}"),
	}
}

/// What the page shows of a window that draws as `drawing`, with the
/// renderer `preference` in effect, on the GPU `device` if any, after the
/// `recent` warnings, with paths under `home` shown as `~`.
pub(crate) fn gather(
	drawing: Drawing,
	preference: RendererPreference,
	device: Option<&str>,
	(warnings, dropped): (Vec<Warning>, usize),
	home: Option<PathBuf>,
) -> Diagnostics {
	let setting = ("Renderer setting", preference.name().to_owned());
	let device = device.map(|device| ("Device", device.to_owned()));
	Diagnostics {
		version: format!(
			"{} \"{}\"",
			env!("CARGO_PKG_VERSION"),
			env!("POOPTERM_CODENAME")
		),
		drawing,
		facts: [Some(setting), device].into_iter().flatten().collect(),
		warnings,
		dropped,
		home,
	}
}
