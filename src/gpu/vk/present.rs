//! Showing frames in a window: acquiring a swapchain image to draw into,
//! and submitting what was drawn so the presentation engine shows it once
//! the GPU is done, ordered by binary semaphores.
use super::{
	call::{Error, check},
	image::Image,
	recording::{Layout, Recording},
	semaphore::Semaphore,
	submit_types::{STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO, SemaphoreSubmitInfo},
	swapchain::Swapchain,
	swapchain_types::{PresentInfo, STRUCTURE_TYPE_PRESENT_INFO},
	timeline::Timeline,
};
use std::{ptr, rc::Rc, time::Duration};

/// `VK_NOT_READY`, `VK_TIMEOUT`, `VK_SUBOPTIMAL_KHR` and
/// `VK_ERROR_OUT_OF_DATE_KHR`.
const NOT_READY: i32 = 1;
const TIMEOUT: i32 = 2;
const SUBOPTIMAL: i32 = 1_000_001_003;
const OUT_OF_DATE: i32 = -1_000_001_004;
/// `VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT`.
const ALL_COMMANDS: u64 = 0x1_0000;

/// A semaphore that acquiring an image signals.
pub(crate) struct Acquire {
	semaphore: Semaphore,
	state: State,
}

#[derive(Clone)]
enum State {
	Free,
	/// Waited for by the submission that raises this timeline to this
	/// value: free once it gets there.
	Waited(Rc<Timeline>, u64),
	/// Signalled for a frame never submitted: nothing will wait for it.
	Stuck,
}

/// What acquiring an image came to.
pub(crate) enum Acquired<'a> {
	Frame(Frame<'a>),
	/// The window changed: remake the swapchain and try again.
	OutOfDate,
	/// No image came free in time.
	Timeout,
}

/// An image acquired to draw a frame into, until it is presented. One
/// dropped without being presented keeps its image: after a few, remake
/// the swapchain.
pub(crate) struct Frame<'a> {
	swapchain: &'a mut Swapchain,
	index: u32,
	acquire: usize,
	/// The swapchain still shows, but no longer matches the surface:
	/// remake it after presenting.
	pub suboptimal: bool,
}

/// What presenting a frame came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presented {
	Shown,
	/// Shown, but the swapchain should be remade.
	Suboptimal,
	/// Not shown: remake the swapchain.
	OutOfDate,
}

impl Swapchain {
	/// The next image to draw into, waiting up to `timeout` for one.
	pub(crate) fn acquire(&mut self, timeout: Duration) -> Result<Acquired<'_>, Error> {
		let acquire = self.free_acquire()?;
		let device = &self.chain.device;
		let semaphore = self.acquires[acquire].semaphore.handle();
		let nanoseconds = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
		let mut index = 0;
		// SAFETY: the swapchain is the device's and not retired, the semaphore
		// is unsignalled with nothing pending, and `index` is writable.
		let code = unsafe {
			(self.functions.acquire)(
				device.handle,
				self.chain.handle,
				nanoseconds,
				semaphore,
				0,
				&mut index,
			)
		};
		match check(code) {
			Err(Error::Failed(OUT_OF_DATE)) => Ok(Acquired::OutOfDate),
			Ok(NOT_READY | TIMEOUT) => Ok(Acquired::Timeout),
			Ok(code) => {
				// Waited for once the frame is submitted, else never.
				self.acquires[acquire].state = State::Stuck;
				Ok(Acquired::Frame(Frame {
					swapchain: self,
					index,
					acquire,
					suboptimal: code == SUBOPTIMAL,
				}))
			}
			Err(error) => Err(error),
		}
	}

	/// The index of an acquire semaphore free to signal, made if none is.
	fn free_acquire(&mut self) -> Result<usize, Error> {
		for (index, acquire) in self.acquires.iter_mut().enumerate() {
			if let State::Waited(timeline, value) = &acquire.state
				&& timeline.reached()? >= *value
			{
				acquire.state = State::Free;
			}
			if matches!(acquire.state, State::Free) {
				return Ok(index);
			}
		}
		self.acquires.push(Acquire {
			semaphore: Semaphore::new(&self.chain.device)?,
			state: State::Free,
		});
		Ok(self.acquires.len() - 1)
	}
}

impl Frame<'_> {
	/// Which of the swapchain's images it is.
	pub(crate) fn index(&self) -> usize {
		self.index as usize
	}

	pub(crate) fn image(&self) -> &Rc<Image> {
		&self.swapchain.images()[self.index as usize]
	}
}

impl Recording<'_> {
	/// Submits the commands, which draw into `frame`'s image, and presents
	/// it once they are done.
	pub(crate) fn present(mut self, frame: Frame<'_>) -> Result<Presented, Error> {
		let swapchain = &mut *frame.swapchain;
		if !Rc::ptr_eq(&swapchain.chain.device, &self.commands.device) {
			return Err(Error::Foreign);
		}
		let image = swapchain.images()[frame.index as usize].clone();
		self.transition(&image, Layout::Present);
		let semaphore = |semaphore: &Semaphore, stage_mask| SemaphoreSubmitInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO,
			next: ptr::null(),
			semaphore: semaphore.handle(),
			value: 0,
			stage_mask,
			device_index: 0,
		};
		let acquired = &swapchain.acquires[frame.acquire];
		// The image is written no earlier than the stages its first barrier
		// waits for.
		let wait = semaphore(&acquired.semaphore, Layout::Present.scope().0);
		let drawn = &swapchain.drawn[frame.index as usize];
		let signal = semaphore(drawn, ALL_COMMANDS);
		let timeline = self.commands.timeline.clone();
		let queue = self.commands.device.queue();
		let value = self.submit_with(Some(&wait), Some(&signal))?;
		swapchain.acquires[frame.acquire].state = State::Waited(timeline, value);
		let waits = [drawn.handle()];
		let info = PresentInfo {
			s_type: STRUCTURE_TYPE_PRESENT_INFO,
			next: ptr::null(),
			wait_semaphore_count: 1,
			wait_semaphores: waits.as_ptr(),
			swapchain_count: 1,
			swapchains: &swapchain.chain.handle,
			image_indices: &frame.index,
			results: ptr::null_mut(),
		};
		// SAFETY: the queue is the device's and used from this thread alone,
		// the image was acquired and is moved to the present layout by the
		// work just submitted, which signals the semaphore waited for, and
		// `info` and what it points to outlive the call.
		let code = unsafe { (swapchain.functions.present)(queue, &info) };
		match check(code) {
			Ok(SUBOPTIMAL) => Ok(Presented::Suboptimal),
			Ok(_) => Ok(Presented::Shown),
			Err(Error::Failed(OUT_OF_DATE)) => Ok(Presented::OutOfDate),
			Err(error) => Err(error),
		}
	}
}
