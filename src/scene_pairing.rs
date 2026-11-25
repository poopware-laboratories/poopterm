//! Windows and the scene they show in, for iOS's scene lifecycle: the
//! terminal's window is made at launch, before UIKit connects the scene,
//! and Settings' later, so whichever arrives first waits for the other.
//! Once the scene is there, every window made goes straight into it.
#![cfg_attr(
	all(not(target_os = "ios"), not(test)),
	expect(dead_code, reason = "only iOS has scenes")
)]

/// The connected scene, if any, and the windows waiting for one.
#[derive(Debug)]
pub(crate) struct Pairing<W, S> {
	scene: Option<S>,
	waiting: Vec<W>,
}

impl<W, S> Pairing<W, S> {
	/// No scene yet and no window waiting.
	pub(crate) const fn new() -> Self {
		Self {
			scene: None,
			waiting: Vec::new(),
		}
	}
}

impl<W, S> Default for Pairing<W, S> {
	fn default() -> Self {
		Self::new()
	}
}

impl<W, S: Clone> Pairing<W, S> {
	/// A window was made: the window and scene to put it in, if there is
	/// a scene yet.
	pub(crate) fn window(&mut self, window: W) -> Vec<(W, S)> {
		match &self.scene {
			Some(scene) => vec![(window, scene.clone())],
			None => {
				self.waiting.push(window);
				Vec::new()
			}
		}
	}

	/// The scene connected: every waiting window, in the order made, with it.
	pub(crate) fn scene(&mut self, scene: S) -> Vec<(W, S)> {
		self.scene = Some(scene.clone());
		self
			.waiting
			.drain(..)
			.map(|window| (window, scene.clone()))
			.collect()
	}

	/// The scene went away; windows made from now on wait for the next.
	pub(crate) fn disconnect(&mut self) {
		self.scene = None;
	}
}
