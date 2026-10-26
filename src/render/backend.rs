//! Renderer backend selection, independent of windows and graphics APIs.

use crate::config::RendererPreference;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Backend {
	Gpu,
	Software,
}

impl Backend {
	pub(crate) const fn name(self) -> &'static str {
		match self {
			Self::Gpu => "gpu",
			Self::Software => "software",
		}
	}
}

pub(crate) const fn candidates(preference: RendererPreference) -> &'static [Backend] {
	match preference {
		RendererPreference::Software => &[Backend::Software],
		RendererPreference::Auto | RendererPreference::Gpu => &[Backend::Gpu, Backend::Software],
	}
}

/// The variable that sets the renderer for one run, over `config.toml`.
pub(crate) const VARIABLE: &str = "POOPTERM_RENDERER";

/// The renderer to use: the one `value`, that of [`VARIABLE`], names, else
/// the `configured` one.
pub(crate) fn preference(
	configured: RendererPreference,
	value: Option<&str>,
) -> RendererPreference {
	value
		.and_then(RendererPreference::named)
		.unwrap_or(configured)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Selection<T, E> {
	pub(crate) backend: Backend,
	pub(crate) value: T,
	pub(crate) failures: Vec<(Backend, E)>,
}

pub(crate) fn select<T, E>(
	preference: RendererPreference,
	mut attempt: impl FnMut(Backend) -> Result<T, E>,
) -> Result<Selection<T, E>, Vec<(Backend, E)>> {
	let mut failures = Vec::new();
	for &backend in candidates(preference) {
		match attempt(backend) {
			Ok(value) => {
				return Ok(Selection {
					backend,
					value,
					failures,
				});
			}
			Err(error) => failures.push((backend, error)),
		}
	}
	Err(failures)
}

#[cfg(test)]
mod tests {
	use super::{Backend, candidates, select};
	use crate::config::RendererPreference;

	#[test]
	fn auto_tries_gpu_before_software() {
		assert_eq!(
			candidates(RendererPreference::Auto),
			&[Backend::Gpu, Backend::Software]
		);
	}

	#[test]
	fn software_never_probes_the_gpu() {
		assert_eq!(
			candidates(RendererPreference::Software),
			&[Backend::Software]
		);
	}

	#[test]
	fn an_explicit_gpu_request_still_has_a_safe_fallback() {
		assert_eq!(
			candidates(RendererPreference::Gpu),
			&[Backend::Gpu, Backend::Software]
		);
	}

	#[test]
	fn backend_names_are_stable_for_diagnostics() {
		assert_eq!(Backend::Gpu.name(), "gpu");
		assert_eq!(Backend::Software.name(), "software");
	}

	#[test]
	fn auto_uses_a_working_gpu_without_touching_software() {
		let mut attempted = Vec::new();
		let selected = select(RendererPreference::Auto, |backend| {
			attempted.push(backend);
			Ok::<_, &'static str>(backend.name())
		})
		.unwrap();
		assert_eq!(selected.backend, Backend::Gpu);
		assert_eq!(selected.value, "gpu");
		assert!(selected.failures.is_empty());
		assert_eq!(attempted, [Backend::Gpu]);
	}

	#[test]
	fn auto_falls_back_once_and_keeps_the_gpu_reason() {
		let mut attempted = Vec::new();
		let selected = select(RendererPreference::Auto, |backend| {
			attempted.push(backend);
			match backend {
				Backend::Gpu => Err("no device"),
				Backend::Software => Ok("cpu surface"),
			}
		})
		.unwrap();
		assert_eq!(selected.backend, Backend::Software);
		assert_eq!(selected.value, "cpu surface");
		assert_eq!(selected.failures, [(Backend::Gpu, "no device")]);
		assert_eq!(attempted, [Backend::Gpu, Backend::Software]);
	}

	#[test]
	fn software_request_does_not_call_a_failing_gpu_probe() {
		let mut attempted = Vec::new();
		let selected = select(RendererPreference::Software, |backend| {
			attempted.push(backend);
			match backend {
				Backend::Software => Ok::<_, &'static str>(17),
				Backend::Gpu => panic!("software selection must not probe the GPU"),
			}
		})
		.unwrap();
		assert_eq!(selected.backend, Backend::Software);
		assert_eq!(selected.value, 17);
		assert!(selected.failures.is_empty());
		assert_eq!(attempted, [Backend::Software]);
	}

	#[test]
	fn explicit_gpu_request_falls_back_instead_of_aborting_startup() {
		let selected = select(RendererPreference::Gpu, |backend| match backend {
			Backend::Gpu => Err("metal unavailable"),
			Backend::Software => Ok("softbuffer"),
		})
		.unwrap();
		assert_eq!(selected.backend, Backend::Software);
		assert_eq!(selected.value, "softbuffer");
		assert_eq!(selected.failures, [(Backend::Gpu, "metal unavailable")]);
	}

	#[test]
	fn every_failure_is_returned_in_attempt_order() {
		let mut sequence = 0;
		let failures = select::<(), _>(RendererPreference::Gpu, |backend| {
			sequence += 1;
			Err((sequence, backend))
		})
		.unwrap_err();
		assert_eq!(sequence, 2);
		assert_eq!(
			failures,
			[
				(Backend::Gpu, (1, Backend::Gpu)),
				(Backend::Software, (2, Backend::Software)),
			]
		);
	}
}
