//! Tests of what each GPU frame's outcome leads to, whichever backend
//! draws: which frames are drawn again, which the next redraws rows of,
//! and which renderer draws the next frame.
use super::{Drawn, Next, after_drawn, next, remembers};

#[test]
fn a_frame_shown_or_with_nothing_to_show_is_done() {
	assert_eq!(next(Ok(Drawn::Shown)), Next::Done);
	// A minimized window draws again once it has area, not before.
	assert_eq!(next(Ok(Drawn::Skipped)), Next::Done);
}

#[test]
fn a_frame_the_swapchain_could_not_take_is_drawn_again() {
	assert_eq!(next(Ok(Drawn::Again)), Next::Redraw);
}

#[test]
fn a_frame_whose_grid_rows_the_pen_keeps_is_remembered_however_it_was_shown() {
	for next in [Next::Done, Next::Redraw] {
		assert!(remembers(true, &next), "{next:?}");
		assert!(!remembers(false, &next), "{next:?}");
	}
}

#[test]
fn a_frame_after_the_gpu_failed_is_not_remembered() {
	// Software never painted the grid the GPU drew.
	assert!(!remembers(true, &Next::Abandon("lost".into())));
}

/// What `after_drawn` did with a GPU after a frame came to `next`, when
/// software would come to `software`: the renderer it hands back, and what
/// it asked for on the way, in order.
fn after(next: Next, software: Result<&str, &str>) -> (Option<String>, Vec<String>) {
	let calls = std::cell::RefCell::new(Vec::new());
	let renderer = after_drawn(
		"gpu",
		next,
		|| calls.borrow_mut().push("again".to_owned()),
		|gpu| format!("kept {gpu}"),
		|gpu, error| {
			calls.borrow_mut().push(format!("{gpu} failed: {error}"));
			software.map(str::to_owned).map_err(str::to_owned)
		},
	);
	(renderer, calls.into_inner())
}

#[test]
fn the_gpu_draws_on_after_a_frame_it_showed() {
	let (renderer, calls) = after(Next::Done, Ok("software"));
	assert_eq!(renderer.as_deref(), Some("kept gpu"));
	assert!(calls.is_empty(), "{calls:?}");
}

#[test]
fn the_gpu_draws_on_and_asks_for_another_frame_after_one_it_could_not_show() {
	let (renderer, calls) = after(Next::Redraw, Ok("software"));
	assert_eq!(renderer.as_deref(), Some("kept gpu"));
	assert_eq!(calls, ["again"]);
}

#[test]
fn a_gpu_that_failed_hands_its_window_to_software_which_draws_the_frame_again() {
	let (renderer, calls) = after(Next::Abandon("device lost".into()), Ok("software"));
	assert_eq!(renderer.as_deref(), Some("software"));
	assert_eq!(calls, ["again", "gpu failed: device lost"]);
}

#[test]
fn a_window_whose_gpu_and_software_both_failed_has_no_renderer() {
	let (renderer, calls) = after(Next::Abandon("device lost".into()), Err("no shm"));
	assert_eq!(renderer, None);
	assert_eq!(calls, ["again", "gpu failed: device lost"]);
}

fn logged(text: &str) -> bool {
	crate::warnings::recent()
		.0
		.iter()
		.any(|warning| warning.text == text)
}

#[test]
fn a_gpu_with_no_software_to_fall_back_on_says_why_in_the_diagnostics_log() {
	let (renderer, _) = after(
		Next::Abandon("lost".into()),
		Err("no surface (gpu_tests 7f30)"),
	);
	assert_eq!(renderer, None);
	assert!(logged(
		"software renderer unavailable: no surface (gpu_tests 7f30)"
	));
}

#[test]
fn falling_back_to_software_that_works_logs_nothing_of_its_own() {
	let (renderer, _) = after(
		Next::Abandon("lost (gpu_tests 0c94)".into()),
		Ok("software"),
	);
	assert_eq!(renderer.as_deref(), Some("software"));
	let (kept, _) = crate::warnings::recent();
	assert!(
		!kept
			.iter()
			.any(|warning| warning.text.contains("gpu_tests 0c94"))
	);
}
