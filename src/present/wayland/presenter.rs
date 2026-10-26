//! A window's frames shown on its `wl_surface` through `wl_shm`: buffers
//! in shared memory, which `Pool` picks and `History` says which rows to
//! copy into, attached with the rows that changed as damage. Our objects
//! live on our own event queue, so the window system's event loop never
//! dispatches their events; we do, before each frame.
use super::{
	client::{Argument, Client, Proxy, Queue},
	events::{Listener, XRGB8888},
};
use crate::present::memory::Memory;
use crate::present::{
	copy::{History, Rows},
	pool::{Pick, Pool},
};
use std::{io, ptr};

// Request opcodes, from wayland.xml.
const GET_REGISTRY: u32 = 1;
const BIND: u32 = 0;
const CREATE_POOL: u32 = 0;
const RELEASE_SHM: u32 = 1;
const CREATE_BUFFER: u32 = 0;
const DESTROY_POOL: u32 = 1;
const DESTROY_BUFFER: u32 = 0;
const ATTACH: u32 = 1;
const DAMAGE: u32 = 2;
const COMMIT: u32 = 6;
const DAMAGE_BUFFER: u32 = 9;
/// The `wl_surface` version that brought `damage_buffer`.
const DAMAGE_BUFFER_SINCE: u32 = 4;
/// The `wl_shm` version that brought `release`.
const RELEASE_SHM_SINCE: u32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Error {
	/// The compositor offers no `wl_shm`, or not our pixel format.
	NoShm,
	/// The connection to the compositor failed.
	Lost,
	/// A frame too large for the protocol, or memory the system refused.
	Memory(String),
}

/// What presenting a frame came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Presented {
	Shown,
	/// The compositor holds every buffer, or the window has no area: try
	/// again once it releases one. What changed is kept for then.
	Busy,
}

struct Buffer {
	id: u64,
	proxy: Proxy,
	pool: Proxy,
	memory: Memory,
}

pub(crate) struct Presenter {
	client: Client,
	display: Proxy,
	surface: Proxy,
	queue: Queue,
	shm: Proxy,
	buffers: Vec<Buffer>,
	pool: Pool,
	history: History,
	/// Dropped after `Drop` destroyed every proxy that writes to it.
	listener: Listener,
}

impl Presenter {
	/// A presenter for `surface` of `display`, at `size` pixels.
	///
	/// # Safety
	///
	/// `display` and `surface` are a live `wl_display` and a `wl_surface`
	/// made on it, which outlive the presenter and are used from this
	/// thread alone.
	pub(crate) unsafe fn new(
		client: Client,
		display: Proxy,
		surface: Proxy,
		size: [u32; 2],
	) -> Result<Self, Error> {
		// SAFETY: the display is live.
		let queue = unsafe { (client.create_queue)(display) };
		if queue.is_null() {
			return Err(Error::Lost);
		}
		let mut presenter = Self {
			client,
			display,
			surface,
			queue,
			shm: ptr::null_mut(),
			buffers: Vec::new(),
			pool: Pool::new(size),
			history: History::new(),
			listener: Listener::new(),
		};
		presenter.bind_shm()?;
		Ok(presenter)
	}

	/// Binds the compositor's `wl_shm` on our queue, and learns its formats.
	fn bind_shm(&mut self) -> Result<(), Error> {
		let client = &self.client;
		// SAFETY: the display is live. The wrapper sends from our queue
		// without moving the display off the window system's, and goes once
		// it made the registry, which stays on our queue.
		let registry = unsafe {
			let wrapper = (client.create_wrapper)(self.display);
			if wrapper.is_null() {
				return Err(Error::Lost);
			}
			(client.set_queue)(wrapper, self.queue);
			let registry = client.send(wrapper, GET_REGISTRY, client.registry, &mut [Argument::NEW]);
			(client.destroy_wrapper)(wrapper);
			registry
		};
		if registry.is_null() {
			return Err(Error::Lost);
		}
		// SAFETY: the registry is ours, on our queue, destroyed below.
		unsafe { self.listener.listen(client, registry) };
		self.listener.events().registry = registry;
		let found = self.roundtrip().map(|()| {
			let globals = &self.listener.events().globals;
			let shm = globals
				.iter()
				.find(|(_, interface, _)| interface == b"wl_shm");
			shm.map(|&(name, _, version)| (name, version.min(RELEASE_SHM_SINCE)))
		});
		let client = &self.client;
		let shm = found.map(|found| {
			found.map(|(name, version)| {
				let interface = Argument {
					string: c"wl_shm".as_ptr(),
				};
				let uint = Argument::uint;
				let mut arguments = [uint(name), interface, uint(version), Argument::NEW];
				// SAFETY: the registry is live; the arguments match `bind`'s
				// signature, for a global it offers at a version it offers,
				// which is also the new object's.
				unsafe {
					(client.marshal)(
						registry,
						BIND,
						client.shm,
						version,
						0,
						arguments.as_mut_ptr(),
					)
				}
			})
		});
		// SAFETY: the registry has no destructor request; its proxy goes,
		// and the listener forgets it.
		unsafe { (client.destroy_proxy)(registry) };
		self.listener.events().registry = ptr::null_mut();
		let shm = shm?.filter(|shm| !shm.is_null()).ok_or(Error::NoShm)?;
		self.shm = shm;
		// SAFETY: wl_shm is ours, on our queue, destroyed on drop.
		unsafe { self.listener.listen(&self.client, shm) };
		self.listener.events().shm = shm;
		self.roundtrip()?;
		match self.listener.events().xrgb {
			true => Ok(()),
			false => Err(Error::NoShm),
		}
	}

	/// Shows `pixels`, rows of `size[0]` pixels, `size[1]` of them: the
	/// whole frame, or only `bands` of pixel rows changed since the last.
	pub(crate) fn present(
		&mut self,
		pixels: &[u32],
		size: [u32; 2],
		bands: Option<&[(usize, usize)]>,
	) -> Result<Presented, Error> {
		if size != self.pool.size() {
			for id in self.pool.resize(size) {
				self.destroy(id);
			}
			self.history.clear();
		}
		self.history.changed(bands);
		// SAFETY: the display and our queue are live.
		if unsafe { (self.client.dispatch_pending)(self.display, self.queue) } < 0 {
			return Err(Error::Lost);
		}
		for proxy in std::mem::take(&mut self.listener.events().released) {
			let buffer = self.buffers.iter().find(|buffer| buffer.proxy == proxy);
			if let Some(id) = buffer.map(|buffer| buffer.id)
				&& self.pool.released(id)
			{
				self.destroy(id);
			}
		}
		let (id, age) = match self.pool.next() {
			Pick::Wait => return Ok(Presented::Busy),
			Pick::Make { id } => match self.make(id, size) {
				Ok(()) => (id, 0),
				Err(error) => {
					self.pool.forget(id);
					return Err(error);
				}
			},
			Pick::Reuse { id, age } => (id, age),
		};
		let [width, height] = size.map(|side| side as usize);
		let buffer = self
			.buffers
			.iter_mut()
			.find(|buffer| buffer.id == id)
			.expect("made");
		let target = buffer.memory.pixels();
		let end = target.len().min(pixels.len());
		for (start, stop) in all_rows(self.history.copy(age, height), height) {
			let range = (start * width).min(end)..(stop * width).min(end);
			target[range.clone()].copy_from_slice(&pixels[range]);
		}
		let proxy = buffer.proxy;
		self.show(proxy, width, &all_rows(self.history.damage(height), height));
		self.pool.presented(id);
		self.history.shown();
		Ok(Presented::Shown)
	}

	/// Attaches `buffer` to the surface with `damage` rows of `width`
	/// pixels changed, commits, and sends it.
	fn show(&self, buffer: Proxy, width: usize, damage: &[(usize, usize)]) {
		let (client, surface) = (&self.client, self.surface);
		// SAFETY: the surface is live (`new`'s contract), and each request
		// passes the arguments of its signature.
		let send = |opcode, arguments: &mut [Argument]| unsafe {
			client.send(surface, opcode, ptr::null(), arguments);
		};
		let int = Argument::int;
		send(ATTACH, &mut [Argument { object: buffer }, int(0), int(0)]);
		// SAFETY: the surface is live.
		let damage_opcode = match unsafe { (client.version)(surface) } {
			DAMAGE_BUFFER_SINCE.. => DAMAGE_BUFFER,
			// Older compositors take surface coordinates, which are buffer
			// coordinates as the window system leaves the buffer scale at 1.
			_ => DAMAGE,
		};
		let side = |value: usize| int(value.min(i32::MAX as usize) as i32);
		for &(start, end) in damage {
			send(
				damage_opcode,
				&mut [side(0), side(start), side(width), side(end - start)],
			);
		}
		send(COMMIT, &mut []);
		// Sent now or with the next dispatch; a broken connection shows then.
		// SAFETY: the display is live.
		unsafe { (client.flush)(self.display) };
	}

	/// Makes buffer `id` of `size` pixels.
	fn make(&mut self, id: u64, size: [u32; 2]) -> Result<(), Error> {
		let [width, height] = size.map(|side| side as usize);
		// `Memory` keeps the size within what the protocol's `int` holds.
		let failed = |error| Error::Memory(format!("{width}×{height} pixels: {error}"));
		let pixels = width.checked_mul(height);
		let pixels = pixels.ok_or_else(|| failed(io::ErrorKind::InvalidInput.into()))?;
		let memory = Memory::new(pixels).map_err(failed)?;
		let (client, int) = (&self.client, |value: usize| Argument::int(value as i32));
		let mut arguments = [Argument::NEW, Argument { fd: memory.fd() }, int(pixels * 4)];
		// SAFETY: `wl_shm` is live; the arguments match `create_pool`'s
		// signature, and sending duplicates the descriptor, which `memory`
		// keeps open meanwhile.
		let pool = unsafe { client.send(self.shm, CREATE_POOL, client.shm_pool, &mut arguments) };
		if pool.is_null() {
			return Err(Error::Lost);
		}
		let format = Argument::uint(XRGB8888);
		let shape = [int(0), int(width), int(height), int(width * 4)];
		let mut arguments = [
			Argument::NEW,
			shape[0],
			shape[1],
			shape[2],
			shape[3],
			format,
		];
		// SAFETY: the pool was just made; the arguments match
		// `create_buffer`'s signature, for a buffer that fills the pool.
		let proxy = unsafe { client.send(pool, CREATE_BUFFER, client.buffer, &mut arguments) };
		if proxy.is_null() {
			// SAFETY: the pool is live and nothing else uses it.
			unsafe { client.destroy(pool, DESTROY_POOL) };
			return Err(Error::Lost);
		}
		// SAFETY: the buffer is ours, on our queue, destroyed by `destroy`.
		unsafe { self.listener.listen(client, proxy) };
		self.buffers.push(Buffer {
			id,
			proxy,
			pool,
			memory,
		});
		Ok(())
	}

	/// Destroys buffer `id`, its pool and its memory.
	fn destroy(&mut self, id: u64) {
		let Some(index) = self.buffers.iter().position(|buffer| buffer.id == id) else {
			return;
		};
		let buffer = self.buffers.swap_remove(index);
		// SAFETY: both are live and ours; the compositor keeps what it shows
		// of a buffer destroyed while it holds it, and its own mapping of
		// the memory, which is unmapped here.
		unsafe {
			self.client.destroy(buffer.proxy, DESTROY_BUFFER);
			self.client.destroy(buffer.pool, DESTROY_POOL);
		}
	}

	/// Sends what is pending and dispatches our queue until the compositor
	/// has answered all of it.
	fn roundtrip(&self) -> Result<(), Error> {
		// SAFETY: the display and our queue are live.
		match unsafe { (self.client.roundtrip)(self.display, self.queue) } {
			..0 => Err(Error::Lost),
			_ => Ok(()),
		}
	}
}

/// `rows` of a buffer `height` rows high, as bands.
fn all_rows(rows: Rows, height: usize) -> Vec<(usize, usize)> {
	match rows {
		Rows::All => vec![(0, height)],
		Rows::Some(rows) => rows,
	}
}

impl Drop for Presenter {
	fn drop(&mut self) {
		while let Some(buffer) = self.buffers.last() {
			self.destroy(buffer.id);
		}
		let client = &self.client;
		// SAFETY: every proxy on the queue is destroyed first, as libwayland
		// requires; the listener is freed after this, with nothing left to
		// write to it.
		unsafe {
			if !self.shm.is_null() {
				match (client.version)(self.shm) {
					RELEASE_SHM_SINCE.. => client.destroy(self.shm, RELEASE_SHM),
					_ => (client.destroy_proxy)(self.shm),
				}
			}
			(client.destroy_queue)(self.queue);
		}
	}
}
