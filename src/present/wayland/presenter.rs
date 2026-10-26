//! A window's frames shown on its `wl_surface` through `wl_shm`: buffers
//! in shared memory, which `Pool` picks and `History` says which rows to
//! copy into, attached with the rows that changed as damage. Our objects
//! live on our own event queue, so the window system's event loop never
//! dispatches their events; we do, before each frame.
use super::{
	client::{Argument, Client, DESTROY, Proxy, Queue},
	memory::Memory,
};
use crate::present::{
	copy::{History, Rows},
	pool::{Pick, Pool},
};
use std::{
	ffi::{CStr, c_int, c_void},
	io,
	ptr::{self, NonNull},
};

/// `WL_SHM_FORMAT_XRGB8888`: our pixels, `0x00RRGGBB` in native order.
/// Every compositor must offer it.
const XRGB8888: u32 = 1;
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

/// What our event queue's events told us; written by `dispatch` alone,
/// and read only between dispatches.
#[derive(Default)]
struct Events {
	registry: Proxy,
	shm: Proxy,
	/// Each global's name, interface and version.
	globals: Vec<(u32, Vec<u8>, u32)>,
	xrgb: bool,
	released: Vec<Proxy>,
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
	/// Boxed, so its address stays put while proxies point to it; freed on
	/// drop, after them.
	events: NonNull<Events>,
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
		let events = NonNull::from(Box::leak(Box::default()));
		let mut presenter = Self {
			client,
			display,
			surface,
			queue,
			shm: ptr::null_mut(),
			buffers: Vec::new(),
			pool: Pool::new(size),
			history: History::new(),
			events,
		};
		presenter.bind_shm()?;
		Ok(presenter)
	}

	/// Binds the compositor's `wl_shm` on our queue, and learns its formats.
	fn bind_shm(&mut self) -> Result<(), Error> {
		let client = &self.client;
		// SAFETY: the display is live; the wrapper sends from our queue
		// without moving the display off the window system's.
		let wrapper = unsafe { (client.create_wrapper)(self.display) };
		if wrapper.is_null() {
			return Err(Error::Lost);
		}
		// SAFETY: the wrapper and queue are live, the argument array matches
		// `get_registry`'s signature (one new id), and the wrapper is
		// destroyed after, which leaves the registry alone.
		let registry = unsafe {
			(client.set_queue)(wrapper, self.queue);
			let version = (client.version)(wrapper);
			let mut new = [Argument {
				object: ptr::null_mut(),
			}];
			let registry = (client.marshal)(
				wrapper,
				GET_REGISTRY,
				client.registry,
				version,
				0,
				new.as_mut_ptr(),
			);
			(client.destroy_wrapper)(wrapper);
			registry
		};
		if registry.is_null() {
			return Err(Error::Lost);
		}
		self.listen(registry);
		// SAFETY: written only by `dispatch`, which runs inside the roundtrip.
		unsafe { (*self.events.as_ptr()).registry = registry };
		let found = self.roundtrip().map(|()| {
			// SAFETY: as above; the roundtrip is over.
			let globals = unsafe { &(*self.events.as_ptr()).globals };
			globals
				.iter()
				.find(|(_, interface, _)| interface == b"wl_shm")
				.map(|&(name, _, version)| (name, version.min(RELEASE_SHM_SINCE)))
		});
		let shm = found.map(|name| {
			name.map(|(name, version)| {
				let mut arguments = [
					Argument { uint: name },
					Argument {
						string: c"wl_shm".as_ptr(),
					},
					Argument { uint: version },
					Argument {
						object: ptr::null_mut(),
					},
				];
				// SAFETY: the registry is live; the arguments match `bind`'s
				// signature for a new object of an interface named in it,
				// at a version it offers that we know.
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
		// and `dispatch` forgets it.
		unsafe {
			(client.destroy_proxy)(registry);
			(*self.events.as_ptr()).registry = ptr::null_mut();
		}
		let shm = shm?.filter(|shm| !shm.is_null()).ok_or(Error::NoShm)?;
		self.shm = shm;
		self.listen(shm);
		// SAFETY: as above.
		unsafe { (*self.events.as_ptr()).shm = shm };
		self.roundtrip()?;
		// SAFETY: as above.
		match unsafe { (*self.events.as_ptr()).xrgb } {
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
		// SAFETY: dispatching is over.
		let released = std::mem::take(unsafe { &mut (*self.events.as_ptr()).released });
		for proxy in released {
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
		let proxy = buffer.proxy;
		let target = buffer.memory.pixels();
		let rows = match self.history.copy(age, height) {
			Rows::All => vec![(0, height)],
			Rows::Some(rows) => rows,
		};
		let end = target.len().min(pixels.len());
		for (start, stop) in rows {
			let range = (start * width).min(end)..(stop * width).min(end);
			target[range.clone()].copy_from_slice(&pixels[range]);
		}
		let damage = match self.history.damage(height) {
			Rows::All => vec![(0, height)],
			Rows::Some(rows) => rows,
		};
		self.show(proxy, width, &damage);
		self.pool.presented(id);
		self.history.shown();
		Ok(Presented::Shown)
	}

	/// Attaches `buffer` to the surface with `damage` rows of `width`
	/// pixels changed, commits, and sends it.
	fn show(&self, buffer: Proxy, width: usize, damage: &[(usize, usize)]) {
		let client = &self.client;
		let surface = self.surface;
		let send = |opcode, arguments: &mut [Argument]| {
			// SAFETY: the surface is live (`new`'s contract), and each call
			// below passes the arguments of its request's signature.
			unsafe {
				let version = (client.version)(surface);
				(client.marshal)(
					surface,
					opcode,
					ptr::null(),
					version,
					0,
					arguments.as_mut_ptr(),
				)
			};
		};
		send(
			ATTACH,
			&mut [
				Argument { object: buffer },
				Argument { int: 0 },
				Argument { int: 0 },
			],
		);
		// SAFETY: the surface is live.
		let damage_opcode = match unsafe { (client.version)(surface) } {
			DAMAGE_BUFFER_SINCE.. => DAMAGE_BUFFER,
			// Older compositors take surface coordinates, which are buffer
			// coordinates as the window system leaves the buffer scale at 1.
			_ => DAMAGE,
		};
		for &(start, end) in damage {
			let side = |value: usize| Argument {
				int: value.min(i32::MAX as usize) as i32,
			};
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
		let pixels = width
			.checked_mul(height)
			.ok_or_else(|| failed(io::ErrorKind::InvalidInput.into()))?;
		let memory = Memory::new(pixels).map_err(failed)?;
		let bytes = pixels * 4;
		let client = &self.client;
		let mut arguments = [
			Argument {
				object: ptr::null_mut(),
			},
			Argument { fd: memory.fd() },
			Argument { int: bytes as i32 },
		];
		// SAFETY: `wl_shm` is live; the arguments match `create_pool`'s
		// signature, and sending duplicates the descriptor, which `memory`
		// keeps open meanwhile.
		let pool = unsafe {
			let version = (client.version)(self.shm);
			(client.marshal)(
				self.shm,
				CREATE_POOL,
				client.shm_pool,
				version,
				0,
				arguments.as_mut_ptr(),
			)
		};
		if pool.is_null() {
			return Err(Error::Lost);
		}
		let side = |value: usize| Argument { int: value as i32 };
		let mut arguments = [
			Argument {
				object: ptr::null_mut(),
			},
			side(0),
			side(width),
			side(height),
			side(width * 4),
			Argument { uint: XRGB8888 },
		];
		// SAFETY: the pool was just made; the arguments match
		// `create_buffer`'s signature, for a buffer that fills the pool.
		let proxy = unsafe {
			let version = (client.version)(pool);
			(client.marshal)(
				pool,
				CREATE_BUFFER,
				client.buffer,
				version,
				0,
				arguments.as_mut_ptr(),
			)
		};
		if proxy.is_null() {
			// SAFETY: the pool is live and nothing else uses it.
			unsafe { destroy(client, pool, DESTROY_POOL) };
			return Err(Error::Lost);
		}
		self.listen(proxy);
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
			destroy(&self.client, buffer.proxy, DESTROY_BUFFER);
			destroy(&self.client, buffer.pool, DESTROY_POOL);
		}
	}

	/// Dispatches `proxy`'s events to `dispatch`.
	fn listen(&self, proxy: Proxy) {
		// SAFETY: the proxy is live and on our queue; `events` outlives it,
		// as every proxy is destroyed before it is freed.
		unsafe {
			(self.client.add_dispatcher)(
				proxy,
				dispatch,
				self.events.as_ptr().cast_const().cast(),
				ptr::null_mut(),
			)
		};
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

impl Drop for Presenter {
	fn drop(&mut self) {
		while let Some(buffer) = self.buffers.last() {
			self.destroy(buffer.id);
		}
		let client = &self.client;
		// SAFETY: every proxy on the queue is destroyed first, as libwayland
		// requires, and nothing dispatches to `events` after.
		unsafe {
			if !self.shm.is_null() {
				match (client.version)(self.shm) {
					RELEASE_SHM_SINCE.. => destroy(client, self.shm, RELEASE_SHM),
					_ => (client.destroy_proxy)(self.shm),
				}
			}
			(client.destroy_queue)(self.queue);
			drop(Box::from_raw(self.events.as_ptr()));
		}
	}
}

/// Sends `proxy`'s destructor request `opcode`, which takes no arguments,
/// and destroys the proxy.
///
/// # Safety
///
/// The proxy is live, and `opcode` is its interface's destructor.
unsafe fn destroy(client: &Client, proxy: Proxy, opcode: u32) {
	// SAFETY: as the caller vouches; no arguments are read.
	unsafe {
		let version = (client.version)(proxy);
		(client.marshal)(
			proxy,
			opcode,
			ptr::null(),
			version,
			DESTROY,
			ptr::null_mut(),
		);
	}
}

/// Records each event on our proxies in the `Events` at `data`.
unsafe extern "C" fn dispatch(
	data: *const c_void,
	target: Proxy,
	opcode: u32,
	_message: *const c_void,
	arguments: *mut Argument,
) -> c_int {
	// SAFETY: `data` is the presenter's `Events`, which nothing else
	// borrows while our queue dispatches; the arguments follow each event's
	// signature in wayland.xml.
	unsafe {
		let events = &mut *data.cast_mut().cast::<Events>();
		let argument = |index| *arguments.add(index);
		if target == events.registry && opcode == 0 {
			// global: name, interface, version.
			let interface = CStr::from_ptr(argument(1).string).to_bytes().to_vec();
			events
				.globals
				.push((argument(0).uint, interface, argument(2).uint));
		} else if target == events.shm && opcode == 0 {
			// format.
			events.xrgb |= argument(0).uint == XRGB8888;
		} else if target != events.registry && target != events.shm && opcode == 0 {
			// A buffer's release.
			events.released.push(target);
		}
	}
	0
}
