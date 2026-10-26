//! A fake libxcb and X server for the presenter's tests. Requests wait
//! until the server serves them, as a real one may take its time: tests
//! serve them with `serve`, and calls that wait for a reply serve all sent
//! before. Served puts land in each window's pixels, read from shared
//! memory only then, and every request that breaks a rule of the protocol
//! or of libxcb is recorded, which tests then require to be none.
use super::client::{Client, Cookie, Extension, ExtensionReply, GeometryReply, VersionReply};
use std::{
	cell::RefCell,
	collections::{BTreeMap, BTreeSet},
	ffi::{c_int, c_uint, c_void},
	fs::File,
	os::{
		fd::{FromRawFd, OwnedFd},
		unix::fs::FileExt,
	},
	ptr::{self, NonNull},
};

/// The Xlib display; the fake never reads through it.
pub(super) const DISPLAY: *mut c_void = 1 as *mut c_void;
const CONNECTION: *mut c_void = 2 as *mut c_void;
const Z_PIXMAP: u8 = 2;
/// The bytes of a `PutImage` request before its pixels.
const PUT_HEADER: usize = 24;

/// `xcb_shm_id`, whose address names the extension.
static SHM: u8 = 0;
static PRESENT: ExtensionReply = extension(1);
static ABSENT: ExtensionReply = extension(0);

const fn extension(present: u8) -> ExtensionReply {
	ExtensionReply {
		response_type: 1,
		pad: 0,
		sequence: 0,
		length: 0,
		present,
		major_opcode: 130,
		first_event: 0,
		first_error: 0,
	}
}

pub(super) struct Window {
	pub depth: u8,
	pub size: [usize; 2],
	pub pixels: Vec<u32>,
}

/// A put the server served: whether through shared memory, its size, and
/// its first row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Put {
	pub shared: bool,
	pub size: [usize; 2],
	pub y: usize,
}

enum Request {
	CreateGc {
		gc: u32,
		window: u32,
	},
	FreeGc(u32),
	Attach {
		shmseg: u32,
		file: File,
	},
	Detach(u32),
	ShmPut {
		window: u32,
		gc: u32,
		shmseg: u32,
		size: [usize; 2],
		seen: Vec<u8>,
	},
	Put {
		window: u32,
		gc: u32,
		depth: u8,
		size: [usize; 2],
		y: usize,
		pixels: Vec<u32>,
	},
	/// Any request with a reply and no effect.
	Reply,
}

pub(super) struct Server {
	pub lost: bool,
	pub setup: Vec<u8>,
	/// The longest request, in 4-byte units.
	pub max_units: u32,
	/// MIT-SHM's version, `None` without it.
	pub shm: Option<(u16, u16)>,
	/// Refuse to attach memory, as a server over a network does.
	pub refuse_attach: bool,
	pub refuse_gc: bool,
	pub windows: BTreeMap<u32, Window>,
	pub gcs: BTreeSet<u32>,
	pub segments: BTreeMap<u32, File>,
	pub puts: Vec<Put>,
	pub flushes: usize,
	pub broken: Vec<String>,
	next_id: u32,
	sequence: c_uint,
	pending: Vec<(c_uint, Request)>,
	/// Served requests whose reply nobody took yet, which libxcb keeps.
	pub replies: BTreeSet<c_uint>,
	/// Served checked requests the server refused.
	errors: BTreeSet<c_uint>,
	/// The depth each `GetGeometry` request answers.
	depths: BTreeMap<c_uint, u8>,
}

thread_local! {
	pub(super) static SERVER: RefCell<Server> = RefCell::new(Server::new());
}

impl Server {
	fn new() -> Self {
		Self {
			lost: false,
			setup: setup(&[(1, 1), (24, 32), (32, 32)]),
			max_units: 1 << 16,
			shm: Some((1, 2)),
			refuse_attach: false,
			refuse_gc: false,
			windows: BTreeMap::new(),
			gcs: BTreeSet::new(),
			segments: BTreeMap::new(),
			puts: Vec::new(),
			flushes: 0,
			broken: Vec::new(),
			next_id: 0x0040_0000,
			sequence: 0,
			pending: Vec::new(),
			replies: BTreeSet::new(),
			errors: BTreeSet::new(),
			depths: BTreeMap::new(),
		}
	}

	fn send(&mut self, request: Request) -> Cookie {
		self.sequence += 1;
		self.pending.push((self.sequence, request));
		Cookie {
			sequence: self.sequence,
		}
	}

	/// Serves every request sent so far, in order.
	pub(super) fn serve(&mut self) {
		for (sequence, request) in std::mem::take(&mut self.pending) {
			match self.apply(request) {
				Ok(true) => _ = self.replies.insert(sequence),
				Ok(false) => {}
				Err(()) => _ = self.errors.insert(sequence),
			}
		}
	}

	/// Applies one request: whether it has a reply, or an error.
	fn apply(&mut self, request: Request) -> Result<bool, ()> {
		match request {
			Request::CreateGc { gc, window } => {
				self.require(self.windows.contains_key(&window), "a GC on no window");
				if self.refuse_gc {
					return Err(());
				}
				self.gcs.insert(gc);
			}
			Request::FreeGc(gc) => {
				let freed = self.gcs.remove(&gc);
				self.require(freed, "freeing a GC twice");
			}
			Request::Attach { shmseg, file } => {
				if self.refuse_attach {
					return Err(());
				}
				self.segments.insert(shmseg, file);
			}
			Request::Detach(shmseg) => {
				let detached = self.segments.remove(&shmseg).is_some();
				self.require(detached, "detaching a segment twice");
			}
			Request::ShmPut {
				window,
				gc,
				shmseg,
				size,
				seen,
			} => {
				let read = self.read(shmseg, size);
				let unchanged = read.as_deref().is_none_or(|read| read == seen);
				self.require(unchanged, "a segment written before the server read it");
				if let (Some(read), true) = (read, self.drawable(window, gc, None)) {
					let pixels = read
						.chunks_exact(4)
						.map(|bytes| u32::from_ne_bytes(bytes.try_into().expect("four bytes")));
					self.draw(window, size, 0, &pixels.collect::<Vec<_>>(), true);
				}
			}
			Request::Put {
				window,
				gc,
				depth,
				size,
				y,
				pixels,
			} => {
				let fits = PUT_HEADER + pixels.len() * 4 <= self.max_units as usize * 4;
				self.require(fits, "a PutImage longer than the server takes");
				if self.drawable(window, gc, Some(depth)) {
					self.draw(window, size, y, &pixels, false);
				}
			}
			Request::Reply => return Ok(true),
		}
		Ok(false)
	}

	/// The bytes of `size` pixels from segment `shmseg`'s start.
	fn read(&mut self, shmseg: u32, size: [usize; 2]) -> Option<Vec<u8>> {
		let mut bytes = vec![0; size[0] * size[1] * 4];
		let file = self.segments.get(&shmseg);
		let read = file.is_some_and(|file| file.read_exact_at(&mut bytes, 0).is_ok());
		self.require(read, "a put from a segment without those pixels");
		read.then_some(bytes)
	}

	fn drawable(&mut self, window: u32, gc: u32, depth: Option<u8>) -> bool {
		let found = self.windows.get(&window).map(|window| window.depth);
		let drawable = found.is_some() && self.gcs.contains(&gc);
		self.require(drawable, "a put without its window or GC");
		let matched = depth.is_none_or(|depth| Some(depth) == found);
		self.require(matched, "a put of another depth than the window's");
		drawable && matched
	}

	/// Draws `pixels`, rows of `size[0]`, at row `y`, clipped to the window.
	fn draw(&mut self, id: u32, size: [usize; 2], y: usize, pixels: &[u32], shared: bool) {
		let window = self.windows.get_mut(&id).expect("checked");
		let [width, height] = window.size;
		for (row, line) in pixels.chunks_exact(size[0].max(1)).enumerate() {
			if y + row < height {
				let start = (y + row) * width;
				let length = width.min(size[0]);
				window.pixels[start..start + length].copy_from_slice(&line[..length]);
			}
		}
		self.puts.push(Put { shared, size, y });
	}

	fn require(&mut self, holds: bool, rule: &str) {
		if !holds {
			self.broken.push(rule.to_owned());
		}
	}

	/// Takes the reply or error of `sequence`: whether there was one.
	fn take(&mut self, sequence: c_uint) -> bool {
		let pending = self.pending.iter().any(|(sent, _)| *sent == sequence);
		if pending {
			self.serve();
		}
		let taken = self.replies.remove(&sequence) | self.errors.remove(&sequence);
		self.require(taken, "a reply taken twice, or never asked for");
		taken
	}
}

/// A setup listing these `(depth, bits_per_pixel)` formats, in our byte
/// order.
pub(super) fn setup(formats: &[(u8, u8)]) -> Vec<u8> {
	let mut bytes = vec![0; 40];
	bytes[0] = 1;
	bytes[29] = formats.len() as u8;
	bytes[30] = if cfg!(target_endian = "little") { 0 } else { 1 };
	for &(depth, bits) in formats {
		bytes.extend_from_slice(&[depth, bits, 32, 0, 0, 0, 0, 0]);
	}
	let length = (bytes.len() - 8) / 4;
	bytes[6..8].copy_from_slice(&(length as u16).to_ne_bytes());
	bytes
}

/// Runs `f` on the server of this thread.
pub(super) fn server<R>(f: impl FnOnce(&mut Server) -> R) -> R {
	SERVER.with(|server| f(&mut server.borrow_mut()))
}

/// A window of `depth` and `size`, black.
pub(super) fn window(depth: u8, size: [usize; 2]) -> u32 {
	server(|server| {
		server.next_id += 1;
		let pixels = vec![0; size[0] * size[1]];
		server.windows.insert(
			server.next_id,
			Window {
				depth,
				size,
				pixels,
			},
		);
		server.next_id
	})
}

/// The client libxcb would be, talking to this thread's server.
pub(super) fn client() -> Client {
	Client::find(|name| {
		let address: *const c_void = match name.to_bytes() {
			b"XGetXCBConnection" => connection as XcbConnection as _,
			b"xcb_generate_id" => generate_id as GenerateId as _,
			b"xcb_flush" => flush as Flush as _,
			b"xcb_connection_has_error" => has_error as Flush as _,
			b"xcb_get_setup" => get_setup as GetSetup as _,
			b"xcb_get_maximum_request_length" => max_length as MaxLength as _,
			b"xcb_get_extension_data" => extension_data as ExtensionData as _,
			b"xcb_shm_id" => (&raw const SHM).cast(),
			b"xcb_shm_query_version" => query as Query as _,
			b"xcb_shm_query_version_reply" => version_reply as VersionAnswer as _,
			b"xcb_shm_attach_fd_checked" => attach_fd as AttachFd as _,
			b"xcb_shm_detach" => detach as Id as _,
			b"xcb_shm_put_image" => shm_put_image as ShmPutImage as _,
			b"xcb_put_image" => put_image as PutImage as _,
			b"xcb_create_gc_checked" => create_gc as CreateGc as _,
			b"xcb_free_gc" => free_gc as Id as _,
			b"xcb_get_geometry" => geometry as Id as _,
			b"xcb_get_geometry_reply" => geometry_reply as GeometryAnswer as _,
			b"xcb_get_input_focus" => query as Query as _,
			b"xcb_poll_for_reply" => poll_for_reply as Poll as _,
			b"xcb_discard_reply" => discard_reply as Discard as _,
			b"xcb_request_check" => request_check as Check as _,
			_ => ptr::null(),
		};
		NonNull::new(address.cast_mut())
	})
	.expect("every function")
}

type XcbConnection = extern "C" fn(*mut c_void) -> *mut c_void;
type GenerateId = extern "C" fn(*mut c_void) -> u32;
type Flush = extern "C" fn(*mut c_void) -> c_int;
type GetSetup = extern "C" fn(*mut c_void) -> *const u8;
type MaxLength = extern "C" fn(*mut c_void) -> u32;
type ExtensionData = extern "C" fn(*mut c_void, *const Extension) -> *const ExtensionReply;
type Query = extern "C" fn(*mut c_void) -> Cookie;
type VersionAnswer = extern "C" fn(*mut c_void, Cookie, *mut *mut c_void) -> *mut VersionReply;
type AttachFd = extern "C" fn(*mut c_void, u32, c_int, u8) -> Cookie;
type Id = extern "C" fn(*mut c_void, u32) -> Cookie;
type ShmPutImage = extern "C" fn(
	*mut c_void,
	u32,
	u32,
	u16,
	u16,
	u16,
	u16,
	u16,
	u16,
	i16,
	i16,
	u8,
	u8,
	u8,
	u32,
	u32,
) -> Cookie;
type PutImage =
	extern "C" fn(*mut c_void, u8, u32, u32, u16, u16, i16, i16, u8, u8, u32, *const u8) -> Cookie;
type CreateGc = extern "C" fn(*mut c_void, u32, u32, u32, *const c_void) -> Cookie;
type GeometryAnswer = extern "C" fn(*mut c_void, Cookie, *mut *mut c_void) -> *mut GeometryReply;
type Poll = extern "C" fn(*mut c_void, c_uint, *mut *mut c_void, *mut *mut c_void) -> c_int;
type Discard = extern "C" fn(*mut c_void, c_uint);
type Check = extern "C" fn(*mut c_void, Cookie) -> *mut c_void;

/// Runs `f` on the server, after checking the connection is ours; a lost
/// connection sends nothing, and `f` is not run.
fn on<R: Default>(connection: *mut c_void, f: impl FnOnce(&mut Server) -> R) -> R {
	server(|server| {
		server.require(connection == CONNECTION, "a request on another connection");
		if server.lost { R::default() } else { f(server) }
	})
}

/// `value`, copied into memory from `malloc`, as libxcb returns replies.
fn allocated<T>(value: T) -> *mut T {
	// SAFETY: a fresh allocation of `T`'s size, aligned for any type.
	unsafe {
		let memory = libc::malloc(size_of::<T>()).cast::<T>();
		memory.write(value);
		memory
	}
}

extern "C" fn connection(display: *mut c_void) -> *mut c_void {
	if display == DISPLAY {
		CONNECTION
	} else {
		ptr::null_mut()
	}
}

extern "C" fn generate_id(connection: *mut c_void) -> u32 {
	on(connection, |server| {
		server.next_id += 1;
		server.next_id
	})
}

extern "C" fn flush(connection: *mut c_void) -> c_int {
	on(connection, |server| {
		server.flushes += 1;
		1
	})
}

extern "C" fn has_error(connection: *mut c_void) -> c_int {
	server(|server| c_int::from(server.lost || connection != CONNECTION))
}

extern "C" fn get_setup(connection: *mut c_void) -> *const u8 {
	// The setup lives as long as the thread's server, past the presenter.
	server(|server| {
		server.require(connection == CONNECTION, "a setup of another connection");
		server.setup.as_ptr()
	})
}

extern "C" fn max_length(connection: *mut c_void) -> u32 {
	on(connection, |server| server.max_units)
}

extern "C" fn extension_data(
	connection: *mut c_void,
	id: *const Extension,
) -> *const ExtensionReply {
	let present = on(connection, |server| {
		server.require(id.cast() == &raw const SHM, "another extension");
		server.shm.is_some()
	});
	if present { &PRESENT } else { &ABSENT }
}

extern "C" fn query(connection: *mut c_void) -> Cookie {
	on(connection, |server| server.send(Request::Reply))
}

extern "C" fn version_reply(
	connection: *mut c_void,
	cookie: Cookie,
	_error: *mut *mut c_void,
) -> *mut VersionReply {
	let version = on(connection, |server| {
		server.take(cookie.sequence).then_some(server.shm)
	});
	let Some(Some((major, minor))) = version else {
		return ptr::null_mut();
	};
	allocated(VersionReply {
		response_type: 1,
		shared_pixmaps: 1,
		sequence: cookie.sequence as u16,
		length: 0,
		major,
		minor,
		uid: 0,
		gid: 0,
		pixmap_format: Z_PIXMAP,
		pad: [0; 15],
	})
}

extern "C" fn attach_fd(connection: *mut c_void, shmseg: u32, fd: c_int, read_only: u8) -> Cookie {
	// SAFETY: libxcb owns the descriptor it is given and closes it once
	// sent; one that is not open fails `fcntl`, and is then not owned.
	let open = unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0;
	let file = open.then(|| File::from(unsafe { OwnedFd::from_raw_fd(fd) }));
	on(connection, |server| {
		server.require(read_only == 1, "memory the server could write");
		server.require(open, "a descriptor that is not open");
		let Some(file) = file else {
			return Cookie::default();
		};
		server.send(Request::Attach { shmseg, file })
	})
}

extern "C" fn detach(connection: *mut c_void, shmseg: u32) -> Cookie {
	on(connection, |server| server.send(Request::Detach(shmseg)))
}

#[expect(clippy::too_many_arguments, reason = "the request's fields")]
extern "C" fn shm_put_image(
	connection: *mut c_void,
	window: u32,
	gc: u32,
	total_width: u16,
	total_height: u16,
	src_x: u16,
	src_y: u16,
	width: u16,
	height: u16,
	dst_x: i16,
	dst_y: i16,
	depth: u8,
	format: u8,
	send_event: u8,
	shmseg: u32,
	offset: u32,
) -> Cookie {
	on(connection, |server| {
		let whole = [total_width, total_height] == [width, height];
		let origin = [
			src_x,
			src_y,
			dst_x as u16,
			dst_y as u16,
			u16::from(send_event),
		];
		server.require(
			whole && origin == [0; 5] && offset == 0,
			"a put of part of a frame",
		);
		server.require(format == Z_PIXMAP, "an image that is not a ZPixmap");
		let found = server.windows.get(&window).map(|window| window.depth);
		server.require(
			found == Some(depth),
			"a put of another depth than the window's",
		);
		let size = [usize::from(width), usize::from(height)];
		let seen = server.read(shmseg, size).unwrap_or_default();
		server.send(Request::ShmPut {
			window,
			gc,
			shmseg,
			size,
			seen,
		})
	})
}

#[expect(clippy::too_many_arguments, reason = "the request's fields")]
extern "C" fn put_image(
	connection: *mut c_void,
	format: u8,
	window: u32,
	gc: u32,
	width: u16,
	height: u16,
	dst_x: i16,
	dst_y: i16,
	left_pad: u8,
	depth: u8,
	length: u32,
	data: *const u8,
) -> Cookie {
	let size = [usize::from(width), usize::from(height)];
	let count = size[0] * size[1];
	// SAFETY: the caller passes `length` bytes; they are read as pixels
	// only when that many, whose alignment `read_unaligned` does not need.
	let pixels = (length as usize == count * 4).then(|| {
		(0..count)
			.map(|index| unsafe { data.cast::<u32>().add(index).read_unaligned() })
			.collect()
	});
	on(connection, |server| {
		server.require(
			format == Z_PIXMAP && left_pad == 0,
			"an image that is not a ZPixmap",
		);
		server.require(
			dst_x == 0 && dst_y >= 0,
			"rows put off the window's left edge",
		);
		server.require(pixels.is_some(), "a length that is not the image's");
		let pixels = pixels.unwrap_or_default();
		let y = dst_y as usize;
		server.send(Request::Put {
			window,
			gc,
			depth,
			size,
			y,
			pixels,
		})
	})
}

extern "C" fn create_gc(
	connection: *mut c_void,
	gc: u32,
	window: u32,
	mask: u32,
	_: *const c_void,
) -> Cookie {
	on(connection, |server| {
		server.require(mask == 0, "GC values the presenter does not need");
		server.send(Request::CreateGc { gc, window })
	})
}

extern "C" fn free_gc(connection: *mut c_void, gc: u32) -> Cookie {
	on(connection, |server| server.send(Request::FreeGc(gc)))
}

extern "C" fn geometry(connection: *mut c_void, window: u32) -> Cookie {
	on(connection, |server| {
		let depth = server.windows.get(&window).map(|window| window.depth);
		server.require(depth.is_some(), "the geometry of no window");
		let cookie = server.send(Request::Reply);
		server
			.depths
			.extend(depth.map(|depth| (cookie.sequence, depth)));
		cookie
	})
}

extern "C" fn geometry_reply(
	connection: *mut c_void,
	cookie: Cookie,
	_error: *mut *mut c_void,
) -> *mut GeometryReply {
	let depth = on(connection, |server| {
		let depth = server.depths.remove(&cookie.sequence);
		server.take(cookie.sequence).then_some(depth).flatten()
	});
	let Some(depth) = depth else {
		return ptr::null_mut();
	};
	allocated(GeometryReply {
		response_type: 1,
		depth,
		sequence: cookie.sequence as u16,
		length: 0,
		root: 1,
		x: 0,
		y: 0,
		width: 1,
		height: 1,
		border: 0,
		pad: [0; 2],
	})
}

extern "C" fn poll_for_reply(
	connection: *mut c_void,
	sequence: c_uint,
	reply: *mut *mut c_void,
	error: *mut *mut c_void,
) -> c_int {
	let answered = on(connection, |server| {
		let pending = server.pending.iter().any(|(sent, _)| *sent == sequence);
		!pending && server.take(sequence)
	});
	// SAFETY: the caller passes places for a reply and an error.
	unsafe {
		*reply = if answered {
			allocated([0u8; 32]).cast()
		} else {
			ptr::null_mut()
		};
		*error = ptr::null_mut();
	}
	c_int::from(answered)
}

extern "C" fn discard_reply(connection: *mut c_void, sequence: c_uint) {
	on(connection, |server| {
		let pending = server.pending.iter().any(|(sent, _)| *sent == sequence);
		if pending {
			server.serve();
		}
		let discarded = server.replies.remove(&sequence);
		server.require(discarded, "discarding a reply never asked for");
	});
}

extern "C" fn request_check(connection: *mut c_void, cookie: Cookie) -> *mut c_void {
	let refused = on(connection, |server| {
		server.serve();
		server.errors.remove(&cookie.sequence)
	});
	if refused {
		allocated([0u8; 32]).cast()
	} else {
		ptr::null_mut()
	}
}

impl Default for Cookie {
	fn default() -> Self {
		Self { sequence: 0 }
	}
}
