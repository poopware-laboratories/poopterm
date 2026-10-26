//! A window's frames shown on an X11 window, through XCB on the Xlib
//! display winit opened. With MIT-SHM 1.2 the server reads them from
//! buffers of memory we share with it, which `Pool` picks and `History`
//! says which rows to copy into; a buffer is free again once a round trip
//! sent after its `ShmPutImage` is answered, as the server has read it by
//! then. Without it, frames go in `PutImage` requests. Windows lose what
//! they show when covered, so every frame is put whole.
use super::{
	client::{Client, Connection, Cookie, GenericError},
	setup::{put_rows, takes_our_pixels},
};
use crate::present::{
	copy::{History, Rows},
	memory::Memory,
	pool::{Pick, Pool},
};
use std::{
	ffi::{c_uint, c_void},
	ptr, slice,
};

/// `XCB_IMAGE_FORMAT_Z_PIXMAP`.
const Z_PIXMAP: u8 = 2;
/// The MIT-SHM version that brought `AttachFd`.
const ATTACH_FD_SINCE: (u16, u16) = (1, 2);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Error {
	/// The connection to the server failed, or it refused a request.
	Lost,
	/// The server takes no 32-bit pixels in our byte order for the window.
	Format,
	/// A frame too wide for a request, or memory the system refused.
	Memory(String),
}

/// What presenting a frame came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Presented {
	Shown,
	/// The server has not read any buffer yet: try again soon. What
	/// changed is kept for then.
	Busy,
}

/// Where frames go.
struct Target {
	client: Client,
	connection: Connection,
	window: u32,
	gc: u32,
	depth: u8,
}

struct Segment {
	id: u64,
	/// The server's name for it.
	shmseg: u32,
	memory: Memory,
	/// The round trip sent after its last put, until answered.
	read: Option<c_uint>,
}

/// Frames put through memory shared with the server.
struct Shared {
	segments: Vec<Segment>,
	pool: Pool,
	history: History,
}

pub(crate) struct Presenter {
	target: Target,
	/// The longest request the server takes, in bytes.
	max_bytes: usize,
	/// `None` without MIT-SHM 1.2, or once the server failed to attach our
	/// memory.
	shared: Option<Shared>,
}

impl Presenter {
	/// A presenter for `window` of the Xlib `display`.
	///
	/// # Safety
	///
	/// `display` is a live Xlib display whose connection is XCB's, `window`
	/// one of its windows, and both outlive the presenter, which is used
	/// from this thread alone.
	pub(crate) unsafe fn new(
		client: Client,
		display: *mut c_void,
		window: u32,
	) -> Result<Self, Error> {
		// SAFETY: the display is live.
		let connection = unsafe { (client.connection)(display) };
		// SAFETY: a connection, even a failed one, may be asked about.
		if connection.is_null() || unsafe { (client.has_error)(connection) } != 0 {
			return Err(Error::Lost);
		}
		let depth = depth(&client, connection, window)?;
		// SAFETY: the setup lives as long as the connection; its length
		// field counts the 4-byte units after its first 8 bytes.
		let setup = unsafe {
			let setup = (client.setup)(connection);
			let length = u16::from_ne_bytes([*setup.add(6), *setup.add(7)]);
			slice::from_raw_parts(setup, 8 + 4 * usize::from(length))
		};
		if !takes_our_pixels(setup, depth) {
			return Err(Error::Format);
		}
		// SAFETY: the connection is live; a GC on the window, without values.
		let gc = unsafe {
			let gc = (client.generate_id)(connection);
			let cookie = (client.create_gc)(connection, gc, window, 0, ptr::null());
			checked(&client, connection, cookie).map(|()| gc)?
		};
		// SAFETY: the connection is live.
		let units = unsafe { (client.maximum_request_length)(connection) };
		let shared = shares_memory(&client, connection).then(|| Shared {
			segments: Vec::new(),
			pool: Pool::new([0, 0]),
			history: History::new(),
		});
		let target = Target {
			client,
			connection,
			window,
			gc,
			depth,
		};
		Ok(Self {
			target,
			max_bytes: units as usize * 4,
			shared,
		})
	}

	/// Whether frames go through shared memory.
	pub(crate) fn shares_memory(&self) -> bool {
		self.shared.is_some()
	}

	/// Shows `pixels`, rows of `size[0]` pixels, `size[1]` of them, in the
	/// window: the whole frame, of which only `bands` of pixel rows
	/// changed since the last, or all.
	pub(crate) fn present(
		&mut self,
		pixels: &[u32],
		size: [u32; 2],
		bands: Option<&[(usize, usize)]>,
	) -> Result<Presented, Error> {
		let target = &self.target;
		// SAFETY: the connection is live.
		if unsafe { (target.client.has_error)(target.connection) } != 0 {
			return Err(Error::Lost);
		}
		if size.iter().any(|&side| side > u32::from(u16::MAX)) {
			return Err(Error::Memory(format!(
				"a frame of {}×{} pixels",
				size[0], size[1]
			)));
		}
		if size.contains(&0) {
			return Ok(Presented::Shown);
		}
		let shown = match &mut self.shared {
			Some(shared) => match shared.present(target, pixels, size, bands) {
				Err(Refused) => {
					// The server cannot map our memory, as over a network.
					shared.close(target);
					self.shared = None;
					target.put(pixels, size, self.max_bytes)
				}
				Ok(shown) => shown,
			},
			None => target.put(pixels, size, self.max_bytes),
		};
		// SAFETY: the connection is live.
		unsafe { (target.client.flush)(target.connection) };
		shown
	}
}

impl Drop for Presenter {
	fn drop(&mut self) {
		let target = &self.target;
		if let Some(shared) = &mut self.shared {
			shared.close(target);
		}
		// SAFETY: the connection and GC are live, and nothing uses the GC
		// after this.
		unsafe {
			(target.client.free_gc)(target.connection, target.gc);
			(target.client.flush)(target.connection);
		}
	}
}

impl Target {
	/// Puts the frame in `PutImage` requests.
	fn put(&self, pixels: &[u32], size: [u32; 2], max_bytes: usize) -> Result<Presented, Error> {
		let [width, height] = size.map(|side| side as usize);
		let rows = put_rows(width, height.min(pixels.len() / width.max(1)), max_bytes);
		let rows = rows.ok_or_else(|| Error::Memory(format!("rows of {width} pixels")))?;
		for (start, end) in rows {
			let bytes = &pixels[start * width..end * width];
			// SAFETY: the connection, window and GC are live, and the pixels
			// fill `end - start` rows of the window's width, as the request
			// says, within the server's maximum (`put_rows`).
			unsafe {
				(self.client.put_image)(
					self.connection,
					Z_PIXMAP,
					self.window,
					self.gc,
					width as u16,
					(end - start) as u16,
					0,
					start as i16,
					0,
					self.depth,
					(bytes.len() * 4) as u32,
					bytes.as_ptr().cast(),
				)
			};
		}
		Ok(Presented::Shown)
	}
}

/// The server failed to attach our memory.
struct Refused;

impl Shared {
	/// Puts the frame through a buffer of shared memory.
	fn present(
		&mut self,
		target: &Target,
		pixels: &[u32],
		size: [u32; 2],
		bands: Option<&[(usize, usize)]>,
	) -> Result<Result<Presented, Error>, Refused> {
		// Buffers made after a resize get every row on their first frame,
		// so the history needs no clearing.
		if size != self.pool.size() {
			for id in self.pool.resize(size) {
				self.destroy(target, id);
			}
		}
		self.history.changed(bands);
		self.collect_reads(target);
		let (id, age) = match self.pool.next() {
			Pick::Wait => return Ok(Ok(Presented::Busy)),
			Pick::Reuse { id, age } => (id, age),
			Pick::Make { id } => match self.make(target, id, size) {
				Ok(Ok(())) => (id, 0),
				failed => {
					self.pool.forget(id);
					return failed.map(|made| made.map(|()| Presented::Busy));
				}
			},
		};
		let [width, height] = size.map(|side| side as usize);
		let rows = match self.history.copy(age, height) {
			Rows::All => vec![(0, height)],
			Rows::Some(rows) => rows,
		};
		let segment = self.segments.iter_mut().find(|segment| segment.id == id);
		let segment = segment.expect("made");
		let buffer = segment.memory.pixels();
		let end = buffer.len().min(pixels.len());
		for (start, stop) in rows {
			let range = (start * width).min(end)..(stop * width).min(end);
			buffer[range.clone()].copy_from_slice(&pixels[range]);
		}
		let [w, h] = size.map(|side| side as u16);
		let (client, connection) = (&target.client, target.connection);
		// SAFETY: the connection, window, GC and segment are live; the
		// segment holds `w × h` pixels from offset 0.
		let read = unsafe {
			let (window, gc, depth, shmseg) = (target.window, target.gc, target.depth, segment.shmseg);
			(client.shm_put_image)(
				connection, window, gc, w, h, 0, 0, w, h, 0, 0, depth, Z_PIXMAP, 0, shmseg, 0,
			);
			(client.input_focus)(connection)
		};
		segment.read = Some(read.sequence);
		self.pool.presented(id);
		self.history.shown();
		Ok(Ok(Presented::Shown))
	}

	/// Frees the buffers whose round trip is answered: the server has read
	/// them.
	fn collect_reads(&mut self, target: &Target) {
		let mut read = Vec::new();
		for segment in &mut self.segments {
			let Some(sequence) = segment.read else {
				continue;
			};
			let (mut reply, mut error) = (ptr::null_mut(), ptr::null_mut());
			// SAFETY: the connection is live, the sequence is a request of
			// ours not yet answered to us, and what it returns is ours to
			// free.
			let answered = unsafe {
				let poll = target.client.poll_for_reply;
				let answered = poll(target.connection, sequence, &mut reply, &mut error);
				libc::free(reply);
				libc::free(error);
				answered
			};
			if answered != 0 {
				segment.read = None;
				read.push(segment.id);
			}
		}
		for id in read {
			if self.pool.released(id) {
				self.destroy(target, id);
			}
		}
	}

	/// Makes buffer `id` of `size` pixels, attached to the server.
	fn make(
		&mut self,
		target: &Target,
		id: u64,
		size: [u32; 2],
	) -> Result<Result<(), Error>, Refused> {
		let [width, height] = size.map(|side| side as usize);
		let failed = |error| Error::Memory(format!("{width}×{height} pixels: {error}"));
		let memory = match Memory::new(width * height) {
			Ok(memory) => memory,
			Err(error) => return Ok(Err(failed(error))),
		};
		// SAFETY: the descriptor is open; libxcb closes the copy once sent.
		let fd = unsafe { libc::fcntl(memory.fd(), libc::F_DUPFD_CLOEXEC, 0) };
		if fd < 0 {
			return Ok(Err(failed(std::io::Error::last_os_error())));
		}
		let (client, connection) = (&target.client, target.connection);
		// SAFETY: the connection is live, and the descriptor ours to give.
		let shmseg = unsafe {
			let shmseg = (client.generate_id)(connection);
			let cookie = (client.attach_fd)(connection, shmseg, fd, 1);
			checked(client, connection, cookie).map(|()| shmseg)
		};
		let shmseg = shmseg.map_err(|_| Refused)?;
		self.segments.push(Segment {
			id,
			shmseg,
			memory,
			read: None,
		});
		Ok(Ok(()))
	}

	/// Destroys buffer `id`: the server lets go of its memory.
	fn destroy(&mut self, target: &Target, id: u64) {
		let found = self.segments.iter().position(|segment| segment.id == id);
		let Some(index) = found else {
			return;
		};
		let segment = self.segments.swap_remove(index);
		// SAFETY: the connection and segment are live; the server keeps its
		// own mapping until it detaches, after any put still reading it.
		unsafe {
			if let Some(sequence) = segment.read {
				(target.client.discard_reply)(target.connection, sequence);
			}
			(target.client.detach)(target.connection, segment.shmseg);
		}
	}

	/// Destroys every buffer.
	fn close(&mut self, target: &Target) {
		while let Some(segment) = self.segments.last() {
			self.destroy(target, segment.id);
		}
	}
}

/// The depth of `window`.
fn depth(client: &Client, connection: Connection, window: u32) -> Result<u8, Error> {
	let mut error = ptr::null_mut();
	// SAFETY: the connection is live; the reply and error are ours to free.
	unsafe {
		let cookie = (client.geometry)(connection, window);
		let reply = (client.geometry_reply)(connection, cookie, &mut error);
		libc::free(error);
		let depth = reply.as_ref().map(|reply| reply.depth);
		libc::free(reply.cast());
		depth.ok_or(Error::Lost)
	}
}

/// Whether the server has MIT-SHM, at a version that attaches memory by
/// file descriptor.
fn shares_memory(client: &Client, connection: Connection) -> bool {
	// SAFETY: the connection is live; the extension data is libxcb's, and
	// the reply and error are ours to free.
	unsafe {
		let extension = (client.extension_data)(connection, client.shm);
		if extension
			.as_ref()
			.is_none_or(|extension| extension.present == 0)
		{
			return false;
		}
		let mut error = ptr::null_mut();
		let cookie = (client.query_version)(connection);
		let reply = (client.query_version_reply)(connection, cookie, &mut error);
		libc::free(error);
		let version = reply.as_ref().map(|reply| (reply.major, reply.minor));
		libc::free(reply.cast());
		version.is_some_and(|version| version >= ATTACH_FD_SINCE)
	}
}

/// Waits for the checked request `cookie`: an error if the server refused
/// it.
///
/// # Safety
///
/// The connection is live, and `cookie` a checked request of ours not
/// checked before.
unsafe fn checked(client: &Client, connection: Connection, cookie: Cookie) -> Result<(), Error> {
	// SAFETY: as the caller vouches; the error is ours to free.
	let error: *mut GenericError = unsafe { (client.request_check)(connection, cookie) };
	if error.is_null() {
		return Ok(());
	}
	// SAFETY: libxcb allocated it with malloc.
	unsafe { libc::free(error) };
	Err(Error::Lost)
}
