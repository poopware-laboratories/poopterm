//! Font sizes in points, kept exact to a hundredth of a point so fractional
//! sizes (13.5) survive parsing, stepping and saving without float drift.

/// The sizes Settings offers; zoom goes beyond them (see `ui::zoom`).
pub(crate) const SETTINGS_MIN: FontSize = FontSize::points(10);
pub(crate) const SETTINGS_MAX: FontSize = FontSize::points(32);
/// One click on Settings' font size stepper.
pub(crate) const SETTINGS_STEP: FontSize = FontSize::hundredths(50);

/// A font size in hundredths of a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FontSize(u32);

impl FontSize {
	pub(crate) const fn points(points: u32) -> Self {
		Self(points * 100)
	}

	pub(crate) const fn hundredths(hundredths: u32) -> Self {
		Self(hundredths)
	}

	/// Reads a size such as `18` or `13.5`, rounded to a hundredth of a
	/// point; `None` for text that is not a finite, non-negative number.
	pub(crate) fn parse(text: &str) -> Option<Self> {
		let points: f64 = text.trim().parse().ok()?;
		// `as` saturates, so huge sizes become u32::MAX for the caller to clamp.
		(points.is_finite() && points >= 0.0).then(|| Self((points * 100.0).round() as u32))
	}

	pub(crate) fn as_f64(self) -> f64 {
		f64::from(self.0) / 100.0
	}

	/// The size one `step` up or down, snapped onto multiples of `step`
	/// (13.3 steps by 0.5 to 13.5 or 13) and kept within `min..=max`.
	pub(crate) fn stepped(self, step: Self, up: bool, min: Self, max: Self) -> Self {
		let step = step.0.max(1);
		let below = self.0 / step * step;
		let next = if up {
			below.saturating_add(step)
		} else if below == self.0 {
			below.saturating_sub(step)
		} else {
			below
		};
		Self(next).clamp(min, max)
	}
}

impl std::fmt::Display for FontSize {
	/// Prints `18`, `13.5` or `13.25`: the shortest exact decimal.
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let (whole, fraction) = (self.0 / 100, self.0 % 100);
		match fraction {
			0 => write!(f, "{whole}"),
			_ if fraction % 10 == 0 => write!(f, "{whole}.{}", fraction / 10),
			_ => write!(f, "{whole}.{fraction:02}"),
		}
	}
}
