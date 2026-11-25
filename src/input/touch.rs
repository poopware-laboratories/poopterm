//! Touches as a phone uses them (iOS): a tap is a click where the finger
//! went down, a drag scrolls by the distance moved. Only the first finger
//! counts; others touching meanwhile are ignored, and a cancelled touch
//! (the system took it, for a gesture of its own) does nothing.

/// How far a finger may move, in pixels, and still tap.
pub(crate) const SLOP: f64 = 12.0;

/// Where a touch is in its life, as the window system reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
	Started,
	Moved,
	Ended,
	Cancelled,
}

/// winit's phase as ours.
pub(crate) fn phase(phase: winit::event::TouchPhase) -> Phase {
	use winit::event::TouchPhase;
	match phase {
		TouchPhase::Started => Phase::Started,
		TouchPhase::Moved => Phase::Moved,
		TouchPhase::Ended => Phase::Ended,
		TouchPhase::Cancelled => Phase::Cancelled,
	}
}

/// What a touch does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Action {
	/// Click the left button at this point.
	Click(f64, f64),
	/// Scroll by this many pixels, positive when the finger moved down
	/// (towards older lines), as a trackpad's pixel delta.
	Scroll(f64),
}

/// The finger being followed.
#[derive(Clone, Copy, Debug)]
struct Finger {
	id: u64,
	start: (f64, f64),
	last_y: f64,
	scrolling: bool,
}

/// The touch being followed, if any.
#[derive(Debug, Default)]
pub(crate) struct Touches {
	finger: Option<Finger>,
}

impl Touches {
	/// Takes touch `id` in `phase` at `position` (pixels); what it does.
	pub(crate) fn event(&mut self, phase: Phase, id: u64, position: (f64, f64)) -> Option<Action> {
		if phase == Phase::Started {
			if self.finger.is_none() {
				self.finger = Some(Finger {
					id,
					start: position,
					last_y: position.1,
					scrolling: false,
				});
			}
			return None;
		}
		let finger = self.finger.as_mut().filter(|finger| finger.id == id)?;
		match phase {
			Phase::Started => None,
			Phase::Moved => {
				let (dx, dy) = (position.0 - finger.start.0, position.1 - finger.start.1);
				if !finger.scrolling && dx.hypot(dy) <= SLOP {
					return None;
				}
				finger.scrolling = true;
				let moved = position.1 - finger.last_y;
				finger.last_y = position.1;
				(moved != 0.0).then_some(Action::Scroll(moved))
			}
			Phase::Ended => {
				let finger = self.finger.take()?;
				(!finger.scrolling).then_some(Action::Click(finger.start.0, finger.start.1))
			}
			Phase::Cancelled => {
				self.finger = None;
				None
			}
		}
	}
}

#[cfg(test)]
#[path = "touch_tests.rs"]
mod tests;
