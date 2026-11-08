//! The Metal device the renderer draws on, the system's default, and its
//! command queue; and what can go wrong talking to Metal.
use crate::objc::{Missing, Object, Pool, Raw};
use std::fmt;

#[link(name = "Metal", kind = "framework")]
unsafe extern "C" {
	fn MTLCreateSystemDefaultDevice() -> Raw;
}

// Metal finds the default device only in processes linked to Core Graphics.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {}

/// The largest side of a 2D texture and the most layers of an array one,
/// on every Mac GPU family Metal supports (Apple7 and later, Mac2).
pub(crate) const TEXTURE_LIMITS: [u32; 2] = [16384, 2048];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
	/// The system has no Metal device.
	NoDevice,
	Missing(Missing),
	/// Metal returned nil where it makes an object of this kind.
	Refused(&'static str),
	/// The shaders did not compile, or the pipeline was not made, as said.
	Shader(String),
	/// The GPU failed a frame, as said.
	Gpu(String),
	/// A frame the GPU cannot draw: its glyphs or images exceed what it
	/// holds.
	Draw,
	/// Texels written to, or read from, outside a texture.
	Region,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::NoDevice => write!(f, "no Metal device"),
			Self::Missing(missing) => missing.fmt(f),
			Self::Refused(what) => write!(f, "Metal made no {what}"),
			Self::Shader(why) => write!(f, "Metal shaders failed: {why}"),
			Self::Gpu(why) => write!(f, "the GPU failed a frame: {why}"),
			Self::Draw => write!(f, "a frame the GPU cannot hold"),
			Self::Region => write!(f, "a region outside its texture"),
		}
	}
}

impl From<Missing> for Error {
	fn from(missing: Missing) -> Self {
		Self::Missing(missing)
	}
}

/// `made`, an object a method returned owned, or `Refused(what)` if nil.
///
/// # Safety
///
/// `made` is nil or a live object the caller owns and hands over.
pub(crate) unsafe fn owned(made: Raw, what: &'static str) -> Result<Object, Error> {
	// SAFETY: as the caller vouches.
	unsafe { Object::owned(made) }.ok_or(Error::Refused(what))
}

/// `made`, an object a method returned without handing it over, retained,
/// or `Refused(what)` if nil.
///
/// # Safety
///
/// `made` is nil or a live object.
pub(crate) unsafe fn retained(made: Raw, what: &'static str) -> Result<Object, Error> {
	// SAFETY: as the caller vouches.
	unsafe { Object::retained(made) }.ok_or(Error::Refused(what))
}

pub(crate) struct Device {
	pub(super) device: Object,
	pub(super) queue: Object,
	name: String,
}

impl Device {
	/// The system's default device and a command queue on it.
	pub(crate) fn new() -> Result<Self, Error> {
		// SAFETY: takes nothing, and returns a device the caller owns, or nil.
		let device = unsafe { Object::owned(MTLCreateSystemDefaultDevice()) };
		Self::with(device.ok_or(Error::NoDevice)?)
	}

	/// Draws on `device`, an `MTLDevice`, with a command queue of its own.
	pub(crate) fn with(device: Object) -> Result<Self, Error> {
		let _pool = Pool::new();
		// SAFETY: an `MTLDevice`'s `newCommandQueue` takes nothing and
		// returns a queue the caller owns, or nil; `name` an `NSString` the
		// caller does not own, which the pool holds until it is copied.
		let (queue, name) = unsafe {
			let queue = owned(device.send(c"newCommandQueue", ()), "command queue")?;
			let name = retained(device.send(c"name", ()), "device name")?;
			(queue, crate::objc::text(&name))
		};
		Ok(Self {
			device,
			queue,
			name,
		})
	}

	/// The name of the device, as the system gives it.
	pub(crate) fn name(&self) -> &str {
		&self.name
	}
}

#[cfg(test)]
#[path = "device_tests.rs"]
pub(crate) mod tests;
