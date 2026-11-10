//! A window drawn by Metal: a `CAMetalLayer` hosted by the window's view,
//! its drawables made at the window's size in pixels and shown at its
//! scale, at most three in flight; each frame drawn into the next drawable
//! and shown with its commands. A frame lacking room in the atlas grows it
//! and is drawn again.
use super::{
	device::{Device, Error, retained},
	renderer::Renderer,
	texture::{BGRA, Texture, Usage},
};
use crate::{
	gpu::{facts::GpuFacts, quads::QuadPen},
	objc::{Bool, Object, Pool, class},
};
use std::{ffi::c_void, ptr::NonNull, rc::Rc};

/// Drawables in flight: one shown, one waiting to be, one drawn into.
pub(crate) const DRAWABLES: usize = 3;

/// What drawing a frame came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
	Shown,
	/// Nothing shown, as the window has no area: draw once it has some.
	Skipped,
	/// Nothing shown, as no drawable came free in time or the atlas grew:
	/// draw again now.
	Again,
}

/// `CGSize`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct Size {
	pub width: f64,
	pub height: f64,
}

/// What to tell the layer before a frame `size` pixels large at `scale`,
/// when it was last told `told` (size and scale): its drawables' size and
/// its contents' scale, or `None` when neither changed.
pub(crate) fn resized(
	told: Option<([u32; 2], f64)>,
	size: [u32; 2],
	scale: f64,
) -> Option<(Size, f64)> {
	let scale = if scale.is_finite() && scale > 0.0 {
		scale
	} else {
		1.0
	};
	if told == Some((size, scale)) {
		return None;
	}
	let drawables = Size {
		width: f64::from(size[0]),
		height: f64::from(size[1]),
	};
	Some((drawables, scale))
}

pub(crate) struct MetalWindow {
	/// Dropped first: it waits for the frames it drew.
	pub(super) renderer: Renderer,
	layer: Object,
	/// The size and scale the layer was last told.
	told: Option<([u32; 2], f64)>,
	device: Rc<Device>,
}

impl MetalWindow {
	/// A window drawn on the system's default device into a new layer,
	/// with an atlas of `atlas` (side, pages); hosted by `view`, an
	/// `NSView`, unless `None` (for tests).
	///
	/// # Safety
	///
	/// `view` is an `NSView` that outlives the result, used only from the
	/// main thread, as the result is.
	pub(crate) unsafe fn new(
		view: Option<NonNull<c_void>>,
		atlas: (usize, usize),
	) -> Result<Self, Error> {
		let device = Rc::new(Device::new()?);
		let renderer = Renderer::new(&device, atlas)?;
		let _pool = Pool::new();
		let class = class(c"CAMetalLayer")?;
		// SAFETY: `layer` returns a new layer the caller does not own, or
		// nil; its setters take a device, an enum, `BOOL`s and an
		// `NSUInteger`. A view takes its layer before it is told it wants one,
		// so it hosts that layer rather than making its own.
		let layer = unsafe {
			let layer = retained(class.send(c"layer", ()), "Metal layer")?;
			let _: () = layer.send(c"setDevice:", (device.device.raw(),));
			let _: () = layer.send(c"setPixelFormat:", (BGRA,));
			let _: () = layer.send(c"setFramebufferOnly:", (1 as Bool,));
			let _: () = layer.send(c"setMaximumDrawableCount:", (DRAWABLES,));
			let _: () = layer.send(c"setOpaque:", (1 as Bool,));
			if let Some(view) = view.and_then(|view| Object::retained(view.as_ptr())) {
				let _: () = view.send(c"setLayer:", (layer.raw(),));
				let _: () = view.send(c"setWantsLayer:", (1 as Bool,));
			}
			layer
		};
		Ok(Self {
			renderer,
			layer,
			told: None,
			device,
		})
	}

	/// The name of the device it draws on.
	pub(crate) fn device_name(&self) -> &str {
		self.device.name()
	}

	/// What it draws on; Metal says nothing of drivers or swapchains here.
	pub(crate) fn facts(&self) -> GpuFacts {
		GpuFacts {
			device: self.device_name().to_owned(),
			api: "Metal",
			version: None,
			driver: None,
			swapchain: None,
			atlas: None,
			presenter: "Metal on macOS".to_owned(),
		}
	}

	/// Draws `pen`'s frame in the window, now `size` pixels large at
	/// `scale` pixels a point, and shows it. A frame lacking room in the
	/// atlas grows it and is to be drawn again. A frame the GPU cannot draw
	/// (lacking glyphs no atlas Metal makes holds, or with images past its
	/// budget) is refused with `Error::Draw`. Either way the pen starts over.
	pub(crate) fn draw(
		&mut self,
		pen: &mut QuadPen,
		size: [u32; 2],
		scale: f64,
	) -> Result<Drawn, Error> {
		if !pen.complete {
			let grew = self.renderer.grow(pen.wanted);
			// The glyphs it added never reach the GPU.
			self.renderer.restart(pen);
			return if grew? {
				Ok(Drawn::Again)
			} else {
				Err(Error::Draw)
			};
		}
		if size.contains(&0) {
			return Ok(Drawn::Skipped);
		}
		if let Some((drawables, scale)) = resized(self.told, size, scale) {
			// SAFETY: `setDrawableSize:` takes a `CGSize` and
			// `setContentsScale:` a `CGFloat`.
			unsafe {
				let _: () = self.layer.send(c"setDrawableSize:", (drawables,));
				let _: () = self.layer.send(c"setContentsScale:", (scale,));
			}
			self.told = Some((size, scale));
		}
		let _pool = Pool::new();
		// SAFETY: `nextDrawable` returns a drawable the caller does not own,
		// or nil when none came free in time; a drawable's `texture` is a 2D
		// texture of the layer's format, made to be drawn to, which the
		// drawable holds.
		let (drawable, target) = unsafe {
			let Some(drawable) = Object::retained(self.layer.send(c"nextDrawable", ())) else {
				return Ok(Drawn::Again);
			};
			let texture = retained(drawable.send(c"texture", ()), "drawable texture")?;
			(drawable, Texture::made(texture, Usage::Target))
		};
		self.renderer.frame(pen, &target, |frame| {
			frame.present(&drawable);
			Ok(())
		})?;
		Ok(Drawn::Shown)
	}
}

#[cfg(test)]
#[path = "window_tests.rs"]
pub(crate) mod tests;
