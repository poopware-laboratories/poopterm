//! A fake libwayland-client and compositor for the presenter's tests: it
//! keeps the objects a compositor would, answers the registry and `wl_shm`
//! as one does, reads each committed buffer back from its shared memory,
//! and records every request that breaks a rule of the protocol or of
//! libwayland, which tests then require to be none.
use super::client::{
	AddDispatcher, Argument, Client, CreateQueue, CreateWrapper, DESTROY, DestroyProxy, DestroyQueue,
	DestroyWrapper, DispatchQueue, Dispatcher, Flush, GetVersion, Interface, Marshal, Proxy, Queue,
	SetQueue,
};
use std::{
	cell::RefCell,
	collections::BTreeMap,
	ffi::{CStr, CString, c_int, c_void},
	os::fd::{AsRawFd, FromRawFd, OwnedFd},
	ptr::{self, NonNull},
};

/// The `wl_display`; the fake never reads through it.
pub(super) const DISPLAY: Proxy = 1 as Proxy;
/// The display's own queue, which ours must never be.
pub(super) const DEFAULT_QUEUE: usize = 0;
pub(super) const XRGB8888: u32 = 1;
const ARGB8888: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
	Wrapper,
	Registry,
	Shm,
	Pool,
	Buffer,
	Surface,
}

pub(super) struct Object {
	pub kind: Kind,
	pub version: u32,
	pub queue: usize,
	pub live: bool,
	dispatcher: Option<(Dispatcher, *const c_void)>,
	/// A pool's memory, as the compositor keeps it, and its size.
	memory: Option<(OwnedFd, i32)>,
	/// A buffer's pool, offset, width, height and stride.
	pub shape: Option<(usize, [i32; 4])>,
	/// A buffer the compositor shows and has not released.
	pub held: bool,
	/// A surface's pending buffer and damage: opcode and rectangle.
	attached: Option<usize>,
	damage: Vec<(u32, [i32; 4])>,
}

/// One commit of a buffer, as the compositor saw it.
#[derive(Clone, Debug)]
pub(super) struct Commit {
	pub buffer: usize,
	pub size: [usize; 2],
	/// Row by row, without the stride's padding.
	pub pixels: Vec<u32>,
	pub damage: Vec<(u32, [i32; 4])>,
}

enum Value {
	Uint(u32),
	Text(CString),
}

struct Event {
	queue: usize,
	target: usize,
	opcode: u32,
	values: Vec<Value>,
}

pub(super) struct Wayland {
	pub objects: BTreeMap<usize, Object>,
	/// Every queue made, and whether it is still live.
	pub queues: BTreeMap<usize, bool>,
	next: usize,
	/// Each global's name, interface and version.
	pub globals: Vec<(u32, &'static CStr, u32)>,
	pub formats: Vec<u32>,
	events: Vec<Event>,
	pub commits: Vec<Commit>,
	pub broken: Vec<String>,
	/// The connection broke: dispatching and flushing fail.
	pub lost: bool,
	/// `wl_display_create_queue` fails.
	pub no_queue: bool,
	/// Requests, by the kind of proxy and opcode, that make no object, as
	/// when libwayland runs out of memory; `(Wrapper, 0)` for wrappers.
	pub refuse: Vec<(Kind, u32)>,
	pub requests: usize,
	/// Requests sent since the last flush, which the compositor has not
	/// seen.
	pub unflushed: usize,
}

impl Default for Wayland {
	fn default() -> Self {
		Self {
			objects: BTreeMap::new(),
			queues: BTreeMap::new(),
			next: 0x100,
			globals: vec![
				(1, c"wl_compositor", 6),
				(2, c"wl_shm", 2),
				(3, c"xdg_wm_base", 6),
			],
			formats: vec![ARGB8888, XRGB8888],
			events: Vec::new(),
			commits: Vec::new(),
			broken: Vec::new(),
			lost: false,
			no_queue: false,
			refuse: Vec::new(),
			requests: 0,
			unflushed: 0,
		}
	}
}

thread_local! {
	static WAYLAND: RefCell<Wayland> = RefCell::default();
}

/// Starts this thread over with a fresh compositor.
pub(super) fn install() {
	WAYLAND.with(|wayland| *wayland.borrow_mut() = Wayland::default());
}

pub(super) fn with<R>(f: impl FnOnce(&mut Wayland) -> R) -> R {
	WAYLAND.with(|wayland| f(&mut wayland.borrow_mut()))
}

static REGISTRY: u8 = 0;
static SHM: u8 = 0;
static SHM_POOL: u8 = 0;
static BUFFER: u8 = 0;

fn interface(of: &'static u8) -> *const Interface {
	ptr::from_ref(of).cast()
}

/// The client whose every function is the fake's.
pub(super) fn client() -> Client {
	Client::find(lookup).expect("the fake has every symbol")
}

/// The fake's symbol `name`.
pub(super) fn lookup(name: &CStr) -> Option<NonNull<c_void>> {
	let address: *const c_void = match name.to_bytes() {
		b"wl_display_create_queue" => create_queue as CreateQueue as _,
		b"wl_event_queue_destroy" => destroy_queue as DestroyQueue as _,
		b"wl_proxy_create_wrapper" => create_wrapper as CreateWrapper as _,
		b"wl_proxy_wrapper_destroy" => destroy_wrapper as DestroyWrapper as _,
		b"wl_proxy_set_queue" => set_queue as SetQueue as _,
		b"wl_proxy_marshal_array_flags" => marshal as Marshal as _,
		b"wl_proxy_add_dispatcher" => add_dispatcher as AddDispatcher as _,
		b"wl_proxy_destroy" => destroy_proxy as DestroyProxy as _,
		b"wl_proxy_get_version" => version as GetVersion as _,
		b"wl_display_dispatch_queue_pending" => dispatch as DispatchQueue as _,
		b"wl_display_roundtrip_queue" => roundtrip as DispatchQueue as _,
		b"wl_display_flush" => flush as Flush as _,
		b"wl_registry_interface" => interface(&REGISTRY).cast(),
		b"wl_shm_interface" => interface(&SHM).cast(),
		b"wl_shm_pool_interface" => interface(&SHM_POOL).cast(),
		b"wl_buffer_interface" => interface(&BUFFER).cast(),
		_ => return None,
	};
	NonNull::new(address.cast_mut())
}

impl Wayland {
	fn broke(&mut self, rule: String) {
		self.broken.push(rule);
	}

	fn make(&mut self, kind: Kind, version: u32, queue: usize) -> usize {
		self.next += 1;
		self.objects.insert(
			self.next,
			Object {
				kind,
				version,
				queue,
				live: true,
				dispatcher: None,
				memory: None,
				shape: None,
				held: false,
				attached: None,
				damage: Vec::new(),
			},
		);
		self.next
	}

	/// A `wl_surface` of `version` on the display's queue, as the window
	/// system makes it.
	pub(super) fn surface(&mut self, version: u32) -> Proxy {
		self.make(Kind::Surface, version, DEFAULT_QUEUE) as Proxy
	}

	/// The live object `proxy`, or a broken rule naming `what`.
	fn live(&mut self, proxy: Proxy, what: &str) -> Option<&mut Object> {
		let id = proxy as usize;
		if !self.objects.get(&id).is_some_and(|object| object.live) {
			self.broke(format!("{what} on dead or unknown proxy {id:#x}"));
			return None;
		}
		self.objects.get_mut(&id)
	}

	/// Objects not destroyed, besides surfaces.
	pub(super) fn leaked(&self) -> Vec<(usize, Kind)> {
		let live = self.objects.iter().filter(|(_, object)| object.live);
		live
			.filter(|(_, object)| object.kind != Kind::Surface)
			.map(|(&id, o)| (id, o.kind))
			.collect()
	}

	/// The buffers the compositor holds, oldest first.
	pub(super) fn held(&self) -> Vec<usize> {
		let held = self
			.objects
			.iter()
			.filter(|(_, object)| object.held && object.live);
		held.map(|(&id, _)| id).collect()
	}

	/// The compositor lets go of `buffer`: its release event waits on the
	/// buffer's queue.
	pub(super) fn release(&mut self, buffer: usize) {
		let object = self.objects.get_mut(&buffer).expect("a buffer");
		assert!(object.held, "{buffer:#x} is not held");
		object.held = false;
		let queue = object.queue;
		if object.live {
			self.events.push(Event {
				queue,
				target: buffer,
				opcode: 0,
				values: Vec::new(),
			});
		}
	}

	pub(super) fn release_all(&mut self) {
		for buffer in self.held() {
			self.release(buffer);
		}
	}

	fn request(
		&mut self,
		proxy: Proxy,
		opcode: u32,
		new: *const Interface,
		version: u32,
		flags: u32,
		arguments: &[Argument],
	) -> Proxy {
		self.requests += 1;
		self.unflushed += 1;
		let Some(object) = self.live(proxy, &format!("request {opcode}")) else {
			return ptr::null_mut();
		};
		let (kind, queue, own) = (object.kind, object.queue, object.version);
		let id = proxy as usize;
		// The version is the new object's: `bind`'s own argument, else the
		// proxy's, as the generated wrappers pass it.
		let expected = match (kind, opcode) {
			// SAFETY: `bind` has its version third.
			(Kind::Registry, 0) => unsafe { arguments[2].uint },
			_ => own,
		};
		if version != expected {
			self.broke(format!(
				"{kind:?} request {opcode} sent at v{version}, not v{expected}"
			));
		}
		let destructor = matches!(
			(kind, opcode),
			(Kind::Shm, 1) | (Kind::Pool, 1) | (Kind::Buffer, 0)
		);
		if (flags & DESTROY != 0) != destructor {
			self.broke(format!("destroy flag {flags} on {kind:?} request {opcode}"));
		}
		// SAFETY: each arm reads the arguments of its request's signature,
		// which the presenter vouched for by sending it.
		let made = unsafe {
			match (kind, opcode) {
				(Kind::Wrapper, 1) => self.get_registry(new, version, queue),
				(Kind::Registry, 0) => self.bind(new, queue, arguments),
				(Kind::Shm, 0) => self.create_pool(new, version, queue, arguments),
				(Kind::Shm, 1) if own < 2 => {
					self.broke(format!("wl_shm.release on a v{own} wl_shm"));
					None
				}
				(Kind::Pool, 0) => self.create_buffer(id, new, version, queue, arguments),
				(Kind::Shm, 1) | (Kind::Pool, 1) | (Kind::Buffer, 0) => None,
				(Kind::Surface, 1) => {
					self.attach(id, arguments[0].object);
					None
				}
				(Kind::Surface, 2 | 9) => {
					if opcode == 9 && own < 4 {
						self.broke(format!("damage_buffer on a v{own} wl_surface"));
					}
					let rectangle = [0, 1, 2, 3].map(|index| arguments[index].int);
					self
						.objects
						.get_mut(&id)
						.expect("live")
						.damage
						.push((opcode, rectangle));
					None
				}
				(Kind::Surface, 6) => {
					self.commit(id);
					None
				}
				_ => {
					self.broke(format!("unknown {kind:?} request {opcode}"));
					None
				}
			}
		};
		if flags & DESTROY != 0 {
			self.objects.get_mut(&id).expect("live").live = false;
		}
		if self.refuse.contains(&(kind, opcode)) {
			// Refused before the compositor saw it: nothing was made.
			if let Some(made) = made {
				self.objects.remove(&made);
			}
			return ptr::null_mut();
		}
		made.map_or(ptr::null_mut(), |made| made as Proxy)
	}

	fn get_registry(&mut self, new: *const Interface, version: u32, queue: usize) -> Option<usize> {
		if new != interface(&REGISTRY) {
			self.broke("get_registry for another interface".into());
		}
		let registry = self.make(Kind::Registry, version, queue);
		for &(name, interface, version) in &self.globals {
			let values = vec![
				Value::Uint(name),
				Value::Text(interface.into()),
				Value::Uint(version),
			];
			self.events.push(Event {
				queue,
				target: registry,
				opcode: 0,
				values,
			});
		}
		Some(registry)
	}

	/// # Safety
	///
	/// `arguments` are `bind`'s: name, interface, version, new id.
	unsafe fn bind(
		&mut self,
		new: *const Interface,
		queue: usize,
		arguments: &[Argument],
	) -> Option<usize> {
		// SAFETY: as the caller vouches.
		let (name, wanted, version) = unsafe {
			(
				arguments[0].uint,
				CStr::from_ptr(arguments[1].string),
				arguments[2].uint,
			)
		};
		let global = self.globals.iter().find(|global| global.0 == name);
		match global {
			Some(&(_, interface, offered)) if interface == wanted && (1..=offered).contains(&version) => {
			}
			_ => self.broke(format!("bound global {name} as {wanted:?} v{version}")),
		}
		if wanted != c"wl_shm" || new != interface(&SHM) {
			self.broke(format!(
				"bound {wanted:?}, which the presenter has no use for"
			));
			return None;
		}
		let shm = self.make(Kind::Shm, version, queue);
		for &format in &self.formats {
			self.events.push(Event {
				queue,
				target: shm,
				opcode: 0,
				values: vec![Value::Uint(format)],
			});
		}
		Some(shm)
	}

	/// # Safety
	///
	/// `arguments` are `create_pool`'s: new id, fd, size.
	unsafe fn create_pool(
		&mut self,
		new: *const Interface,
		version: u32,
		queue: usize,
		arguments: &[Argument],
	) -> Option<usize> {
		// SAFETY: as the caller vouches.
		let (fd, size) = unsafe { (arguments[1].fd, arguments[2].int) };
		if new != interface(&SHM_POOL) {
			self.broke("create_pool for another interface".into());
		}
		// libwayland sends a duplicate, which the compositor keeps.
		// SAFETY: duplicating checks the descriptor; the copy is ours.
		let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
		if copy < 0 {
			self.broke(format!("create_pool with bad descriptor {fd}"));
			return None;
		}
		// SAFETY: just duplicated, owned by nothing else.
		let copy = unsafe { OwnedFd::from_raw_fd(copy) };
		// SAFETY: a zeroed `stat` is valid, and the descriptor is open.
		let length = unsafe {
			let mut stat = std::mem::zeroed::<libc::stat>();
			libc::fstat(copy.as_raw_fd(), &mut stat);
			stat.st_size
		};
		if size <= 0 || i64::from(size) > length as i64 {
			self.broke(format!("pool of {size} bytes over a {length}-byte file"));
		}
		let pool = self.make(Kind::Pool, version, queue);
		self.objects.get_mut(&pool).expect("made").memory = Some((copy, size));
		Some(pool)
	}

	/// # Safety
	///
	/// `arguments` are `create_buffer`'s: new id, offset, width, height,
	/// stride, format.
	unsafe fn create_buffer(
		&mut self,
		pool: usize,
		new: *const Interface,
		version: u32,
		queue: usize,
		arguments: &[Argument],
	) -> Option<usize> {
		// SAFETY: as the caller vouches.
		let [offset, width, height, stride] = [1, 2, 3, 4].map(|index| unsafe { arguments[index].int });
		// SAFETY: as above.
		let format = unsafe { arguments[5].uint };
		if new != interface(&BUFFER) {
			self.broke("create_buffer for another interface".into());
		}
		let size = self.objects[&pool]
			.memory
			.as_ref()
			.map_or(0, |memory| memory.1);
		let end = i64::from(offset) + i64::from(stride) * i64::from(height);
		if width <= 0
			|| height <= 0
			|| offset < 0
			|| i64::from(stride) < i64::from(width) * 4
			|| end > i64::from(size)
		{
			self.broke(format!(
				"{width}×{height} buffer, stride {stride} at {offset}, in a {size}-byte pool"
			));
		}
		if !self.formats.contains(&format) {
			self.broke(format!("buffer of format {format}, which was not offered"));
		}
		let buffer = self.make(Kind::Buffer, version, queue);
		self.objects.get_mut(&buffer).expect("made").shape =
			Some((pool, [offset, width, height, stride]));
		Some(buffer)
	}

	fn attach(&mut self, surface: usize, buffer: Proxy) {
		let attached = (!buffer.is_null()).then_some(buffer as usize);
		if attached.is_some_and(|buffer| {
			self
				.objects
				.get(&buffer)
				.is_none_or(|o| o.kind != Kind::Buffer || !o.live)
		}) {
			self.broke(format!("attached dead or unknown buffer {buffer:?}"));
		}
		self.objects.get_mut(&surface).expect("live").attached = attached;
	}

	fn commit(&mut self, surface: usize) {
		let object = self.objects.get_mut(&surface).expect("live");
		let damage = std::mem::take(&mut object.damage);
		let Some(buffer) = object.attached.take() else {
			return;
		};
		let Some(target) = self.objects.get(&buffer).filter(|object| object.live) else {
			return self.broke(format!("committed dead buffer {buffer:#x}"));
		};
		let (held, shape) = (target.held, target.shape);
		if held {
			self.broke(format!(
				"committed buffer {buffer:#x}, which the compositor holds"
			));
		}
		let (pool, [offset, width, height, stride]) = shape.expect("a buffer");
		for &(opcode, [x, y, w, h]) in &damage {
			// Surface damage is in surface coordinates, which equal the
			// buffer's at scale 1.
			let inside = x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= width && y + h <= height;
			if !inside {
				self.broke(format!(
					"damage {opcode} {x},{y} {w}×{h} outside a {width}×{height} buffer"
				));
			}
		}
		let fd = self.objects[&pool]
			.memory
			.as_ref()
			.expect("a pool")
			.0
			.as_raw_fd();
		let mut bytes = vec![0u8; (stride * height) as usize];
		// SAFETY: reads into a buffer of that many bytes.
		let read = unsafe { libc::pread(fd, bytes.as_mut_ptr().cast(), bytes.len(), offset.into()) };
		assert_eq!(read, bytes.len() as isize, "the pool holds the buffer");
		let rows = bytes.chunks(stride as usize);
		let pixels = rows
			.flat_map(|row| row[..width as usize * 4].chunks(4))
			.map(|pixel| u32::from_ne_bytes(pixel.try_into().unwrap()));
		let pixels = pixels.collect();
		self.objects.get_mut(&buffer).expect("live").held = true;
		let size = [width as usize, height as usize];
		self.commits.push(Commit {
			buffer,
			size,
			pixels,
			damage,
		});
	}
}

unsafe extern "C" fn create_queue(display: Proxy) -> Queue {
	with(|wayland| {
		if display != DISPLAY {
			wayland.broke("create_queue on something not the display".into());
		}
		if wayland.no_queue {
			return ptr::null_mut();
		}
		wayland.next += 1;
		wayland.queues.insert(wayland.next, true);
		wayland.next as Queue
	})
}

unsafe extern "C" fn destroy_queue(queue: Queue) {
	with(|wayland| {
		let id = queue as usize;
		if wayland.queues.get(&id) != Some(&true) {
			wayland.broke(format!("destroyed dead or unknown queue {id:#x}"));
		}
		let on = wayland
			.objects
			.values()
			.filter(|object| object.live && object.queue == id);
		if on.count() > 0 {
			wayland.broke(format!("destroyed queue {id:#x} with live proxies on it"));
		}
		wayland.queues.insert(id, false);
	});
}

unsafe extern "C" fn create_wrapper(display: Proxy) -> Proxy {
	with(|wayland| {
		if display != DISPLAY {
			wayland.broke("wrapped something not the display".into());
		}
		if wayland.refuse.contains(&(Kind::Wrapper, 0)) {
			return ptr::null_mut();
		}
		// The display is version 0, and so are its wrappers.
		wayland.make(Kind::Wrapper, 0, DEFAULT_QUEUE) as Proxy
	})
}

unsafe extern "C" fn destroy_wrapper(wrapper: Proxy) {
	with(|wayland| {
		if let Some(object) = wayland.live(wrapper, "destroy_wrapper") {
			let kind = object.kind;
			object.live = false;
			if kind != Kind::Wrapper {
				wayland.broke(format!("destroy_wrapper on a {kind:?}"));
			}
		}
	});
}

unsafe extern "C" fn set_queue(proxy: Proxy, queue: Queue) {
	with(|wayland| {
		let id = queue as usize;
		if wayland.queues.get(&id) != Some(&true) {
			wayland.broke(format!("set dead or unknown queue {id:#x}"));
		}
		if let Some(object) = wayland.live(proxy, "set_queue") {
			object.queue = id;
		}
	});
}

unsafe extern "C" fn marshal(
	proxy: Proxy,
	opcode: u32,
	new: *const Interface,
	version: u32,
	flags: u32,
	arguments: *mut Argument,
) -> Proxy {
	// Every request the presenter sends has at most six arguments; the
	// fake reads only those of each request's signature.
	let arguments = match arguments.is_null() {
		true => &[][..],
		// SAFETY: the presenter passes at least as many as the request has,
		// and those past them are never read.
		false => unsafe { std::slice::from_raw_parts(arguments, max_arguments(proxy, opcode)) },
	};
	with(|wayland| wayland.request(proxy, opcode, new, version, flags, arguments))
}

/// How many arguments request `opcode` of `proxy` takes.
fn max_arguments(proxy: Proxy, opcode: u32) -> usize {
	let kind = with(|wayland| {
		wayland
			.objects
			.get(&(proxy as usize))
			.map(|object| object.kind)
	});
	match (kind, opcode) {
		(Some(Kind::Wrapper), 1) => 1,
		(Some(Kind::Registry), 0) => 4,
		(Some(Kind::Shm), 0) => 3,
		(Some(Kind::Pool), 0) => 6,
		(Some(Kind::Surface), 1) => 3,
		(Some(Kind::Surface), 2 | 9) => 4,
		_ => 0,
	}
}

unsafe extern "C" fn add_dispatcher(
	proxy: Proxy,
	dispatcher: Dispatcher,
	data: *const c_void,
	_: *mut c_void,
) -> c_int {
	with(|wayland| {
		let Some(object) = wayland.live(proxy, "add_dispatcher") else {
			return -1;
		};
		if object.dispatcher.replace((dispatcher, data)).is_some() {
			wayland.broke(format!("second dispatcher on {:#x}", proxy as usize));
			return -1;
		}
		0
	})
}

unsafe extern "C" fn destroy_proxy(proxy: Proxy) {
	with(|wayland| {
		if let Some(object) = wayland.live(proxy, "destroy_proxy") {
			object.live = false;
			let (kind, version) = (object.kind, object.version);
			// Objects with a destructor request must be destroyed with it.
			if matches!(kind, Kind::Pool | Kind::Buffer) || (kind == Kind::Shm && version >= 2) {
				wayland.broke(format!("destroy_proxy on a v{version} {kind:?}"));
			}
		}
	});
}

unsafe extern "C" fn version(proxy: Proxy) -> u32 {
	with(|wayland| match proxy == DISPLAY {
		true => 0,
		false => wayland
			.live(proxy, "get_version")
			.map_or(0, |object| object.version),
	})
}

/// `dispatch_queue_pending`: the events waiting on `queue`, which the fake
/// queues as it answers each request.
unsafe extern "C" fn dispatch(display: Proxy, queue: Queue) -> c_int {
	let id = queue as usize;
	let events = with(|wayland| {
		if display != DISPLAY || wayland.queues.get(&id) != Some(&true) {
			wayland.broke(format!("dispatched dead or unknown queue {id:#x}"));
		}
		if wayland.lost {
			return None;
		}
		let (mine, rest) = std::mem::take(&mut wayland.events)
			.into_iter()
			.partition(|event| event.queue == id);
		wayland.events = rest;
		Some(mine)
	});
	let Some(events) = events else {
		return -1;
	};
	let count = events.len();
	for event in events {
		let found = with(|wayland| {
			wayland
				.objects
				.get(&event.target)
				.filter(|o| o.live)
				.map(|o| o.dispatcher)
		});
		// Events for destroyed proxies are dropped, as libwayland does.
		let Some(dispatcher) = found else {
			continue;
		};
		let Some((dispatcher, data)) = dispatcher else {
			with(|wayland| {
				wayland.broke(format!(
					"event for {:#x}, which has no dispatcher",
					event.target
				))
			});
			continue;
		};
		let mut arguments: Vec<Argument> = (event.values.iter())
			.map(|value| match value {
				Value::Uint(uint) => Argument { uint: *uint },
				Value::Text(text) => Argument {
					string: text.as_ptr(),
				},
			})
			.collect();
		// SAFETY: the dispatcher and its data were given for this proxy, and
		// the arguments are the event's, alive for the call.
		unsafe {
			dispatcher(
				data,
				event.target as Proxy,
				event.opcode,
				ptr::null(),
				arguments.as_mut_ptr(),
			)
		};
	}
	count as c_int
}

/// Flushes, then dispatches what waits on `queue`: the fake answers every
/// request at once.
unsafe extern "C" fn roundtrip(display: Proxy, queue: Queue) -> c_int {
	// SAFETY: the fake's own functions, which read nothing through these.
	unsafe {
		match flush(display) {
			0 => dispatch(display, queue),
			failed => failed,
		}
	}
}

unsafe extern "C" fn flush(display: Proxy) -> c_int {
	with(|wayland| {
		if display != DISPLAY {
			wayland.broke("flushed something not the display".into());
		}
		match wayland.lost {
			true => -1,
			false => {
				wayland.unflushed = 0;
				0
			}
		}
	})
}
