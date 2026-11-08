//! Corner radii of the window chrome, one per kind of element, so every
//! surface rounds the same way and a redesign changes them in one place.

/// A kind of rounded element in the chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Corner {
	/// Keycaps, and the light under a tab's close button.
	Chip,
	/// Hover and pressed fills, fields, selected rows and tab fills.
	Control,
	/// The outline of a bordered Settings button.
	Button,
	/// The top corners of the active tab, which joins the terminal.
	Tab,
	/// A floating popover: the search box and the command palette.
	Popover,
}

impl Corner {
	/// The radius in device pixels at the display's integer `scale`.
	pub(crate) fn radius(self, scale: usize) -> usize {
		let logical = match self {
			Self::Chip => 5,
			Self::Control => 6,
			Self::Button => 7,
			Self::Tab => 8,
			Self::Popover => 10,
		};
		logical * scale
	}
}

/// The radius of a fill drawn `inset` pixels inside an outline rounded by
/// `outer`, so both curves share a center; square once the inset reaches it.
pub(crate) fn concentric(outer: usize, inset: usize) -> usize {
	outer.saturating_sub(inset)
}

/// The radius that makes a square of `side` a circle, or a bar of that
/// height a pill.
pub(crate) fn pill_radius(side: usize) -> usize {
	side / 2
}
