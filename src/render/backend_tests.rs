//! Exhaustive renderer-selection tests with a window-free fake backend.
use super::backend::{Backend, Selection, candidates, select};
use crate::config::{Preferences, RendererPreference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
	Metal(u32),
	Software(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
	NoDevice,
	NoSurface,
	Rejected(u32),
}

#[derive(Debug, PartialEq, Eq)]
struct Run {
	result: Result<Selection<Value, Failure>, Vec<(Backend, Failure)>>,
	attempts: Vec<Backend>,
}

fn run(
	preference: RendererPreference,
	gpu: Result<Value, Failure>,
	software: Result<Value, Failure>,
) -> Run {
	let mut attempts = Vec::new();
	let result = select(preference, |backend| {
		attempts.push(backend);
		match backend {
			Backend::Gpu => gpu,
			Backend::Software => software,
		}
	});
	Run { result, attempts }
}

fn selected(run: Run) -> Selection<Value, Failure> {
	run.result.expect("a backend should have been selected")
}

fn failed(run: Run) -> Vec<(Backend, Failure)> {
	run.result.expect_err("every backend should have failed")
}

#[test]
fn auto_with_a_device_returns_the_gpu_value_unchanged() {
	let run = run(
		RendererPreference::Auto,
		Ok(Value::Metal(41)),
		Ok(Value::Software(99)),
	);
	assert_eq!(run.attempts, [Backend::Gpu]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, Value::Metal(41));
	assert!(selection.failures.is_empty());
}

#[test]
fn auto_fallback_returns_the_software_value_unchanged() {
	let run = run(
		RendererPreference::Auto,
		Err(Failure::NoDevice),
		Ok(Value::Software(73)),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(73));
	assert_eq!(selection.failures, [(Backend::Gpu, Failure::NoDevice)]);
}

#[test]
fn auto_reports_both_failures_when_neither_backend_starts() {
	let run = run(
		RendererPreference::Auto,
		Err(Failure::NoDevice),
		Err(Failure::NoSurface),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	assert_eq!(
		failed(run),
		[
			(Backend::Gpu, Failure::NoDevice),
			(Backend::Software, Failure::NoSurface),
		]
	);
}

#[test]
fn software_mode_returns_the_software_value_unchanged() {
	let run = run(
		RendererPreference::Software,
		Ok(Value::Metal(10)),
		Ok(Value::Software(20)),
	);
	assert_eq!(run.attempts, [Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(20));
	assert!(selection.failures.is_empty());
}

#[test]
fn software_mode_returns_only_the_software_failure() {
	let run = run(
		RendererPreference::Software,
		Err(Failure::NoDevice),
		Err(Failure::NoSurface),
	);
	assert_eq!(run.attempts, [Backend::Software]);
	assert_eq!(failed(run), [(Backend::Software, Failure::NoSurface)]);
}

#[test]
fn gpu_mode_returns_the_gpu_value_without_fallback() {
	let run = run(
		RendererPreference::Gpu,
		Ok(Value::Metal(7)),
		Ok(Value::Software(8)),
	);
	assert_eq!(run.attempts, [Backend::Gpu]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, Value::Metal(7));
	assert!(selection.failures.is_empty());
}

#[test]
fn gpu_mode_falls_back_instead_of_turning_a_driver_error_into_startup_failure() {
	let run = run(
		RendererPreference::Gpu,
		Err(Failure::Rejected(19)),
		Ok(Value::Software(23)),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(23));
	assert_eq!(selection.failures, [(Backend::Gpu, Failure::Rejected(19))]);
}

#[test]
fn gpu_mode_reports_driver_and_surface_failures_together() {
	let run = run(
		RendererPreference::Gpu,
		Err(Failure::Rejected(29)),
		Err(Failure::Rejected(31)),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	assert_eq!(
		failed(run),
		[
			(Backend::Gpu, Failure::Rejected(29)),
			(Backend::Software, Failure::Rejected(31)),
		]
	);
}

#[test]
fn a_success_stops_before_a_backend_that_would_panic() {
	let mut attempts = Vec::new();
	let selection = select(RendererPreference::Auto, |backend| {
		attempts.push(backend);
		match backend {
			Backend::Gpu => Ok::<_, ()>(Value::Metal(1)),
			Backend::Software => panic!("software must not start after GPU success"),
		}
	})
	.unwrap();
	assert_eq!(attempts, [Backend::Gpu]);
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, Value::Metal(1));
}

#[test]
fn software_mode_never_calls_a_gpu_probe_that_would_panic() {
	let mut attempts = Vec::new();
	let selection = select(RendererPreference::Software, |backend| {
		attempts.push(backend);
		match backend {
			Backend::Gpu => panic!("software mode must not probe the GPU"),
			Backend::Software => Ok::<_, ()>(Value::Software(2)),
		}
	})
	.unwrap();
	assert_eq!(attempts, [Backend::Software]);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(2));
}

#[test]
fn selection_accepts_values_borrowed_from_the_caller() {
	let metal = String::from("Metal");
	let software = String::from("softbuffer");
	let selection = select(RendererPreference::Auto, |backend| match backend {
		Backend::Gpu => Ok::<_, &str>(metal.as_str()),
		Backend::Software => Ok(software.as_str()),
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, "Metal");
	assert!(selection.failures.is_empty());
	assert_eq!(metal, "Metal");
	assert_eq!(software, "softbuffer");
}

#[test]
fn fallback_accepts_errors_borrowed_from_the_caller() {
	let reason = String::from("device removed");
	let selection = select(RendererPreference::Auto, |backend| match backend {
		Backend::Gpu => Err(reason.as_str()),
		Backend::Software => Ok("ready"),
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, "ready");
	assert_eq!(selection.failures, [(Backend::Gpu, "device removed")]);
	assert_eq!(reason, "device removed");
}

#[test]
fn a_mutable_probe_can_consume_a_scripted_sequence() {
	let mut script = vec![
		(Backend::Software, Ok(Value::Software(37))),
		(Backend::Gpu, Err(Failure::NoDevice)),
	];
	let selection = select(RendererPreference::Auto, |backend| {
		let (expected, result) = script.pop().unwrap();
		assert_eq!(backend, expected);
		result
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(37));
	assert_eq!(selection.failures, [(Backend::Gpu, Failure::NoDevice)]);
	assert!(script.is_empty());
}

#[test]
fn every_call_owns_an_independent_failure_history() {
	let first = run(
		RendererPreference::Auto,
		Err(Failure::Rejected(1)),
		Ok(Value::Software(2)),
	);
	let second = run(
		RendererPreference::Auto,
		Err(Failure::Rejected(3)),
		Ok(Value::Software(4)),
	);
	assert_eq!(
		selected(first).failures,
		[(Backend::Gpu, Failure::Rejected(1))]
	);
	assert_eq!(
		selected(second).failures,
		[(Backend::Gpu, Failure::Rejected(3))]
	);
}

#[test]
fn zero_sized_values_and_errors_follow_the_same_fallback_contract() {
	let selection = select(RendererPreference::Gpu, |backend| match backend {
		Backend::Gpu => Err(()),
		Backend::Software => Ok(()),
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, ());
	assert_eq!(selection.failures, [(Backend::Gpu, ())]);
	assert_eq!(std::mem::size_of_val(&selection.value), 0);
}

#[test]
fn an_empty_error_string_is_preserved_for_the_diagnostic_layer() {
	let selection = select(RendererPreference::Auto, |backend| match backend {
		Backend::Gpu => Err(""),
		Backend::Software => Ok("software"),
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, "software");
	assert_eq!(selection.failures.len(), 1);
	assert_eq!(selection.failures[0].0, Backend::Gpu);
	assert_eq!(selection.failures[0].1, "");
}

#[test]
fn structured_errors_are_not_flattened_or_rewritten() {
	let selection = select(RendererPreference::Gpu, |backend| match backend {
		Backend::Gpu => Err((5, "MTLCreateSystemDefaultDevice")),
		Backend::Software => Ok((8, "softbuffer")),
	})
	.unwrap();
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, (8, "softbuffer"));
	assert_eq!(
		selection.failures,
		[(Backend::Gpu, (5, "MTLCreateSystemDefaultDevice"))]
	);
}

#[test]
fn default_preferences_feed_the_auto_candidate_order() {
	let preferences = Preferences::default();
	assert_eq!(preferences.renderer, RendererPreference::Auto);
	assert_eq!(
		candidates(preferences.renderer),
		[Backend::Gpu, Backend::Software]
	);
}

#[test]
fn parsed_auto_config_prefers_gpu_and_keeps_software_as_fallback() {
	let preferences = Preferences::parse("renderer = \"auto\"");
	let run = run(
		preferences.renderer,
		Err(Failure::NoDevice),
		Ok(Value::Software(43)),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(43));
}

#[test]
fn parsed_software_config_skips_an_available_gpu() {
	let preferences = Preferences::parse("renderer = \"software\"");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(47)),
		Ok(Value::Software(53)),
	);
	assert_eq!(run.attempts, [Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(53));
}

#[test]
fn parsed_gpu_config_retains_the_mandatory_software_fallback() {
	let preferences = Preferences::parse("renderer = \"gpu\"");
	let run = run(
		preferences.renderer,
		Err(Failure::Rejected(59)),
		Ok(Value::Software(61)),
	);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.failures, [(Backend::Gpu, Failure::Rejected(59))]);
}

#[test]
fn unknown_renderer_config_uses_the_safe_auto_policy() {
	let preferences = Preferences::parse("renderer = \"quantum\"");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(67)),
		Ok(Value::Software(71)),
	);
	assert_eq!(preferences.renderer, RendererPreference::Auto);
	assert_eq!(run.attempts, [Backend::Gpu]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, Value::Metal(67));
}

#[test]
fn last_duplicate_config_value_controls_the_probe_policy() {
	let preferences =
		Preferences::parse("renderer = \"gpu\"\nrenderer = \"auto\"\nrenderer = \"software\"");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(73)),
		Ok(Value::Software(79)),
	);
	assert_eq!(preferences.renderer, RendererPreference::Software);
	assert_eq!(run.attempts, [Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(79));
}

#[test]
fn inline_comments_do_not_change_the_selected_policy() {
	let preferences = Preferences::parse("renderer = gpu # prefer Metal when possible");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(83)),
		Ok(Value::Software(89)),
	);
	assert_eq!(preferences.renderer, RendererPreference::Gpu);
	assert_eq!(run.attempts, [Backend::Gpu]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Gpu);
	assert_eq!(selection.value, Value::Metal(83));
}

#[test]
fn whitespace_around_config_tokens_does_not_change_the_policy() {
	let preferences = Preferences::parse("  renderer\t =\t \"software\"   ");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(97)),
		Ok(Value::Software(101)),
	);
	assert_eq!(preferences.renderer, RendererPreference::Software);
	assert_eq!(run.attempts, [Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(101));
}

#[test]
fn case_mismatched_config_does_not_accidentally_force_a_backend() {
	let preferences = Preferences::parse("renderer = \"GPU\"");
	let run = run(
		preferences.renderer,
		Err(Failure::NoDevice),
		Ok(Value::Software(103)),
	);
	assert_eq!(preferences.renderer, RendererPreference::Auto);
	assert_eq!(run.attempts, [Backend::Gpu, Backend::Software]);
	let selection = selected(run);
	assert_eq!(selection.backend, Backend::Software);
	assert_eq!(selection.value, Value::Software(103));
}

#[test]
fn renderer_policy_is_independent_of_unrelated_config_values() {
	let preferences =
		Preferences::parse("font_size = 26\nrenderer = \"gpu\"\nshow_tabs = false\npadding = 11");
	let run = run(
		preferences.renderer,
		Ok(Value::Metal(107)),
		Ok(Value::Software(109)),
	);
	assert_eq!(
		preferences.font_size,
		crate::ui::font_size::FontSize::points(26)
	);
	assert!(!preferences.show_tabs);
	assert_eq!(preferences.padding, 11);
	assert_eq!(run.attempts, [Backend::Gpu]);
	assert_eq!(selected(run).value, Value::Metal(107));
}

#[test]
fn saving_other_settings_preserves_a_hand_written_renderer_key() {
	let original = "# mine\nrenderer = \"software\"\nfont_size = 18\n";
	let updated = crate::config::updated(original, &[("font_size", "24".to_owned())]);
	assert!(updated.contains("# mine\n"));
	assert!(updated.contains("renderer = \"software\"\n"));
	assert!(updated.contains("font_size = 24\n"));
	assert_eq!(updated.matches("renderer").count(), 1);
	let preferences = Preferences::parse(&updated);
	assert_eq!(preferences.renderer, RendererPreference::Software);
}

#[test]
fn backend_names_match_the_values_users_put_in_config() {
	for (text, expected) in [
		("renderer = \"gpu\"", Backend::Gpu),
		("renderer = \"software\"", Backend::Software),
	] {
		let preference = Preferences::parse(text).renderer;
		assert!(candidates(preference).contains(&expected));
		assert!(text.contains(expected.name()));
		assert_eq!(expected.name(), expected.name().to_ascii_lowercase());
	}
}

#[test]
fn candidate_slices_are_static_and_reused_between_calls() {
	let first = candidates(RendererPreference::Auto);
	let second = candidates(RendererPreference::Auto);
	let software = candidates(RendererPreference::Software);
	assert!(std::ptr::eq(first, second));
	assert_eq!(first.len(), 2);
	assert_eq!(software.len(), 1);
	assert_eq!(first[0], Backend::Gpu);
	assert_eq!(first[1], Backend::Software);
	assert_eq!(software[0], Backend::Software);
}
