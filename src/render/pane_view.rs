//! A split tab's pane as a grid view: the window's view of its grid, moved
//! into the pane and showing that pane's screen.
use super::grid::GridView;
use crate::{graphics::Rect, term::Frame};

impl<'a> GridView<'a> {
	/// This view showing `frame` in a pane at `rect`, its edge backgrounds
	/// reaching into `padding`. Only the `focused` pane keeps the selection,
	/// the search match and a solid cursor.
	pub(crate) fn in_pane(self, frame: &'a Frame, rect: Rect, padding: Rect, focused: bool) -> Self {
		Self {
			frame,
			origin: (rect.x, rect.y),
			selection: self.selection.filter(|_| focused),
			search_match: self.search_match.filter(|_| focused),
			focused: self.focused && focused,
			padding: Some(padding),
			..self
		}
	}
}
