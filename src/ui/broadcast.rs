//! Broadcasting input: what the user types goes to every pane of the tab,
//! not only the focused one.

/// The sessions input goes to: every pane of the tab (`panes`, in order)
/// while broadcasting, else only the `focused` one.
pub(crate) fn input_targets(focused: u64, panes: &[u64], broadcast: bool) -> Vec<u64> {
	match broadcast && panes.contains(&focused) {
		true => panes.to_vec(),
		false => vec![focused],
	}
}
