//! Window renderer boundary and the software backend.
use super::{Arc, Window, repaint};
use crate::{
	config::RendererPreference,
	render::{
		backend::{self, Backend, select},
		damage::GridDamage,
	},
	term::Frame,
};
use std::num::NonZeroU32;

pub(super) struct Renderer {
	_context: softbuffer::Context<Arc<Window>>,
	surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

impl Renderer {
	pub(super) fn new(window: Arc<Window>, preference: RendererPreference) -> Result<Self, String> {
		let variable = std::env::var(backend::VARIABLE).ok();
		let preference = backend::preference(preference, variable.as_deref());
		let selected = select(preference, |backend| match backend {
			Backend::Gpu => Err("not available in this build".to_owned()),
			Backend::Software => {
				let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
				let surface =
					softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
				Ok(Self {
					_context: context,
					surface,
				})
			}
		})
		.map_err(|failures| format_failures(&failures))?;
		if !selected.failures.is_empty() {
			eprintln!(
				"poopterm: {}; using {} renderer",
				format_failures(&selected.failures),
				selected.backend.name()
			);
		}
		Ok(selected.value)
	}

	pub(super) fn resize(&mut self, width: u32, height: u32) {
		if let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
			let _ = self.surface.resize(width, height);
		}
	}

	pub(super) fn present(
		&mut self,
		_frame: &Frame,
		_damage: Option<&GridDamage>,
		pixels: &[u32],
		width: usize,
		bands: Option<&[(usize, usize)]>,
		damage_only: bool,
	) {
		if let Ok(buffer) = self.surface.buffer_mut() {
			repaint::present(buffer, pixels, width, bands, damage_only);
		}
	}
}

fn format_failures(failures: &[(Backend, String)]) -> String {
	failures
		.iter()
		.map(|(backend, error)| format!("{} renderer unavailable: {error}", backend.name()))
		.collect::<Vec<_>>()
		.join("; ")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fallback_diagnostic_names_the_backend_and_reason() {
		let failures = [(Backend::Gpu, "Metal device creation failed".to_owned())];
		assert_eq!(
			format_failures(&failures),
			"gpu renderer unavailable: Metal device creation failed"
		);
	}

	#[test]
	fn startup_diagnostic_keeps_multiple_failures_in_attempt_order() {
		let failures = [
			(Backend::Gpu, "no device".to_owned()),
			(Backend::Software, "no window surface".to_owned()),
		];
		assert_eq!(
			format_failures(&failures),
			"gpu renderer unavailable: no device; software renderer unavailable: no window surface"
		);
	}

	#[test]
	fn no_failures_produce_no_diagnostic_text() {
		assert_eq!(format_failures(&[]), "");
	}
}
