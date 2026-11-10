//! A window drawn by the GPU: the device chosen for its surface, the grid
//! renderer, and a swapchain made at the window's size, remade when the
//! window changes size or the presentation engine says it no longer fits.
use super::{
	call::Error,
	choose::{choose, suits},
	device::Device,
	entry::Entry,
	image::Format,
	optional::{self, Optional},
	pipeline::QuadPipeline,
	present::{Acquired, Presented},
	present_regions::rects,
	renderer::Renderer,
	surface::{Surface, Window},
	swapchain::Swapchain,
	swapchain_plan::Unfit,
	timeline::Timeline,
	types::RawPhysicalDevice,
};
use crate::gpu::{facts::GpuFacts, quads::QuadPen};
use std::{rc::Rc, time::Duration};

/// How long a frame waits for an image to draw into: about two frames, so
/// a compositor holding every image never stalls the window for long.
pub(crate) const ACQUIRE_WAIT: Duration = Duration::from_millis(32);
/// No window's size, as a window without area draws nothing.
const REMAKE: [u32; 2] = [0; 2];

/// What drawing a frame came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
	Shown,
	/// Nothing shown, as the window has no area: draw once it has some.
	Skipped,
	/// Nothing shown, as no image came free in time or the swapchain was
	/// out of date: draw again now.
	Again,
}

pub(crate) struct GpuWindow {
	/// Dropped first: it waits for the frames it drew.
	pub(super) renderer: Renderer,
	/// `None` until the window first has area, and after a remake failed.
	swapchain: Option<Swapchain>,
	/// The window size the swapchain was made for; [`REMAKE`] to remake
	/// it before the next frame whatever the size.
	made_for: [u32; 2],
	surface: Rc<Surface>,
	device: Rc<Device>,
	physical: RawPhysicalDevice,
	/// Whether the swapchain has shown an image, which later ones change.
	shown: bool,
	/// What it draws on, without the swapchain, which may be remade.
	facts: GpuFacts,
}

impl GpuWindow {
	/// Draws in `window` on the device that suits it best, allowing a CPU
	/// device when `software`, with an atlas of `atlas` (side, pages);
	/// `Error::NoDevice` says why no device suits.
	///
	/// # Safety
	///
	/// The window's handles stay live until the result is dropped.
	pub(crate) unsafe fn new(
		entry: Entry,
		window: Window,
		software: bool,
		atlas: (usize, usize),
	) -> Result<Self, Error> {
		let instance = Rc::new(entry.create_window_instance(c"poopterm", window.platform())?);
		// SAFETY: as the caller vouches.
		let surface = Rc::new(unsafe { Surface::new(&instance, window) }?);
		let handles = instance.physical_devices()?;
		let mut devices = Vec::new();
		for &physical in &handles {
			let mut device = instance.describe(physical)?;
			surface.mark_present(physical, &mut device)?;
			devices.push(device);
		}
		let Some(choice) = choose(&devices, true, software) else {
			let why = devices.into_iter().filter_map(|device| {
				let why = suits(&device, true, software).err()?;
				Some((device.name, why))
			});
			return Err(Error::NoDevice(why.collect()));
		};
		let physical = handles[choice.device];
		let chosen = devices.swap_remove(choice.device);
		let facts = GpuFacts {
			device: chosen.name,
			api: "Vulkan",
			version: Some(chosen.api),
			driver: instance.driver(physical),
			swapchain: None,
			atlas: None,
			presenter: window.platform().presenter().to_owned(),
		};
		let disable = std::env::var(optional::VARIABLE).ok();
		let offered = instance.device_extensions(physical)?;
		let mut optional = Optional::found(&offered, disable.as_deref());
		optional.keep_supported(|feature| instance.has_feature(physical, feature));
		let family = choice.family as u32;
		let device = Device::with_optional(instance, physical, family, optional)?;
		let device = Rc::new(device);
		let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra)?);
		let timeline = Rc::new(Timeline::new(&device)?);
		Ok(Self {
			renderer: Renderer::new(&pipeline, &timeline, atlas)?,
			swapchain: None,
			made_for: REMAKE,
			shown: false,
			surface,
			device,
			physical,
			facts,
		})
	}

	/// The name of the device it draws on.
	pub(crate) fn device_name(&self) -> &str {
		&self.facts.device
	}

	/// The GPU time of the frame before the last, where the device times
	/// them.
	pub(crate) fn gpu_time(&self) -> Option<std::time::Duration> {
		self.renderer.gpu_time()
	}

	/// What it draws on and how, with its swapchain once made.
	pub(crate) fn facts(&self) -> GpuFacts {
		let swapchain = self.swapchain.as_ref();
		GpuFacts {
			swapchain: swapchain.map(|chain| chain.plan().facts(chain.images().len())),
			..self.facts.clone()
		}
	}

	/// Draws `pen`'s frame in the window, now `size` pixels large, and
	/// shows it. A frame lacking room in the atlas grows it and is to be
	/// drawn again. A frame the GPU cannot draw (lacking glyphs no atlas
	/// it can make holds, or with images past its budget or its device) is
	/// refused with `Error::Draw`. Either way the pen starts over.
	pub(crate) fn draw(&mut self, pen: &mut QuadPen, size: [u32; 2]) -> Result<Drawn, Error> {
		// For this attempt only: one drawn again is presented whole.
		let changed = pen.changed.take();
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
		// Not left to the plan: a Wayland surface takes any size, so the
		// driver would make the smallest it allows.
		if size.contains(&0) {
			return Ok(Drawn::Skipped);
		}
		if self.made_for != size {
			let latest_ready = self.device.optional.fifo_latest_ready.is_some();
			let plan = match self.surface.plan(self.physical, size, latest_ready) {
				Err(Error::Unfit(Unfit::Hidden)) => return Ok(Drawn::Skipped),
				// The shaders write BGRA: RGBA images would swap red and blue.
				Ok(plan) if plan.format != Format::Bgra => return Err(Error::Unfit(Unfit::Format)),
				planned => planned?,
			};
			// Retired even if the remake fails: then made anew.
			self.made_for = REMAKE;
			self.shown = false;
			let made = match self.swapchain.take() {
				Some(old) => old.recreate(plan),
				None => Swapchain::new(&self.device, &self.surface, plan),
			};
			self.swapchain = Some(made?);
			self.made_for = size;
		}
		let Some(swapchain) = &mut self.swapchain else {
			unreachable!("made above");
		};
		let frame = match swapchain.acquire(ACQUIRE_WAIT)? {
			Acquired::Frame(frame) => frame,
			Acquired::OutOfDate => {
				self.made_for = REMAKE;
				return Ok(Drawn::Again);
			}
			Acquired::Timeout => return Ok(Drawn::Again),
		};
		let suboptimal = frame.suboptimal;
		let image = frame.image().clone();
		// Changes are since the last image this swapchain showed: the first
		// it shows is new whole.
		let mut frame = frame;
		if let (true, Some(bands)) = (self.shown, &changed) {
			frame.changed = rects(bands, size);
		}
		// A frame dropped unpresented keeps its image: remake the swapchain.
		self.made_for = REMAKE;
		let recorded = self.renderer.frame(pen, &image);
		let presented = recorded.and_then(|recording| recording.present(frame));
		let presented = presented.inspect_err(|_| self.renderer.restart(pen))?;
		if !suboptimal && presented == Presented::Shown {
			self.made_for = size;
		}
		self.shown = presented != Presented::OutOfDate;
		Ok(match presented {
			Presented::OutOfDate => Drawn::Again,
			Presented::Shown | Presented::Suboptimal => Drawn::Shown,
		})
	}
}
