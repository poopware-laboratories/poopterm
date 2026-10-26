//! The presenter against the system's libxcb and a running X server:
//! frames put in a window of our own, through shared memory and through
//! `PutImage` requests, read back with `GetImage`. Skipped without
//! `DISPLAY`, unless `POOPTERM_REQUIRE_X11` is set, as in CI (Xvfb).
use super::client::{Client, Connection, Cookie, Extension, ExtensionReply};
use super::presenter::{Presented, Presenter};
use crate::library::Library;
use std::{
	ffi::{CStr, c_char, c_int, c_void},
	ptr::{self, NonNull},
	slice,
};

type OpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type CloseDisplay = unsafe extern "C" fn(*mut c_void) -> c_int;
type CreateWindow = unsafe extern "C" fn(
	Connection,
	u8,
	u32,
	u32,
	i16,
	i16,
	u16,
	u16,
	u16,
	u16,
	u32,
	u32,
	*const u32,
) -> Cookie;
type MapWindow = unsafe extern "C" fn(Connection, u32) -> Cookie;
type GetImage = unsafe extern "C" fn(Connection, u8, u32, i16, i16, u16, u16, u32) -> Cookie;
type GetImageReply = unsafe extern "C" fn(Connection, Cookie, *mut *mut c_void) -> *mut c_void;
type ImageData = unsafe extern "C" fn(*const c_void) -> *const u8;
type ImageLength = unsafe extern "C" fn(*const c_void) -> c_int;

/// `xcb_screen_iterator_t`.
#[repr(C)]
struct Screens {
	data: *const u32,
	rem: c_int,
	index: c_int,
}
type Roots = unsafe extern "C" fn(*const u8) -> Screens;

/// The functions the test needs besides the presenter's.
struct Extra {
	open: OpenDisplay,
	close: CloseDisplay,
	create_window: CreateWindow,
	map_window: MapWindow,
	get_image: GetImage,
	get_image_reply: GetImageReply,
	image_data: ImageData,
	image_length: ImageLength,
	roots: Roots,
	_libraries: [Library; 2],
}

fn extra() -> Extra {
	let x11 = Library::open(c"libX11.so.6").expect("libX11");
	let xcb = Library::open(c"libxcb.so.1").expect("libxcb");
	let symbol =
		|library: &Library, name: &CStr| library.symbol(name).unwrap_or_else(|| panic!("{name:?}"));
	// SAFETY: each is the function Xlib.h or xproto.h declares with that
	// type.
	unsafe {
		use std::mem::transmute as cast;
		type Address = NonNull<c_void>;
		Extra {
			open: cast::<Address, OpenDisplay>(symbol(&x11, c"XOpenDisplay")),
			close: cast::<Address, CloseDisplay>(symbol(&x11, c"XCloseDisplay")),
			create_window: cast::<Address, CreateWindow>(symbol(&xcb, c"xcb_create_window")),
			map_window: cast::<Address, MapWindow>(symbol(&xcb, c"xcb_map_window")),
			get_image: cast::<Address, GetImage>(symbol(&xcb, c"xcb_get_image")),
			get_image_reply: cast::<Address, GetImageReply>(symbol(&xcb, c"xcb_get_image_reply")),
			image_data: cast::<Address, ImageData>(symbol(&xcb, c"xcb_get_image_data")),
			image_length: cast::<Address, ImageLength>(symbol(&xcb, c"xcb_get_image_data_length")),
			roots: cast::<Address, Roots>(symbol(&xcb, c"xcb_setup_roots_iterator")),
			_libraries: [x11, xcb],
		}
	}
}

/// Says MIT-SHM is missing, so the presenter puts images.
extern "C" fn absent(_: Connection, _: *const Extension) -> *const ExtensionReply {
	static ABSENT: ExtensionReply = ExtensionReply {
		response_type: 1,
		pad: 0,
		sequence: 0,
		length: 0,
		present: 0,
		major_opcode: 0,
		first_event: 0,
		first_error: 0,
	};
	&ABSENT
}

/// Runs `f` with a presenter on a mapped window of `size`, its client
/// sharing memory unless told otherwise, and a function reading back what
/// the window shows; `None` without an X server.
fn on_real_window<R>(
	size: [u16; 2],
	share: bool,
	f: impl FnOnce(&mut Presenter, &dyn Fn() -> Vec<u32>) -> R,
) -> Option<R> {
	let required = std::env::var_os("POOPTERM_REQUIRE_X11").is_some();
	if std::env::var_os("DISPLAY").is_none() {
		assert!(!required, "POOPTERM_REQUIRE_X11 is set, but DISPLAY is not");
		return None;
	}
	let extra = extra();
	let mut client = Client::load().expect("libxcb");
	if !share {
		client.extension_data = absent;
	}
	// SAFETY: a fresh display, closed below after the presenter drops; the
	// window is the root's child, of the root's depth and visual. The
	// image reply is malloc'd and holds the bytes it says.
	unsafe {
		let display = (extra.open)(ptr::null());
		assert!(!display.is_null(), "no X server at DISPLAY");
		let connection = (client.connection)(display);
		let root = *(extra.roots)((client.setup)(connection)).data;
		let window = (client.generate_id)(connection);
		// Override redirect keeps window managers from moving or resizing it.
		let values = [1u32];
		let [width, height] = size;
		(extra.create_window)(
			connection,
			0,
			window,
			root,
			0,
			0,
			width,
			height,
			0,
			1,
			0,
			0x200,
			values.as_ptr(),
		);
		(extra.map_window)(connection, window);
		let read = || {
			let cookie = (extra.get_image)(connection, 2, window, 0, 0, width, height, !0);
			let reply = (extra.get_image_reply)(connection, cookie, ptr::null_mut());
			assert!(!reply.is_null(), "GetImage refused");
			let length = (extra.image_length)(reply) as usize;
			let bytes = slice::from_raw_parts((extra.image_data)(reply), length);
			let pixels = bytes
				.chunks_exact(4)
				.map(|pixel| u32::from_ne_bytes(pixel.try_into().expect("four bytes")) & 0xff_ffff);
			let pixels = pixels.collect();
			libc::free(reply);
			pixels
		};
		read();
		let mut presenter = Presenter::new(client, display, window).expect("a presenter");
		let result = f(&mut presenter, &read);
		drop(presenter);
		read();
		(extra.close)(display);
		Some(result)
	}
}

/// A frame whose rows differ by `seed`.
fn frame(seed: u32, [width, height]: [u16; 2]) -> Vec<u32> {
	let pixels = usize::from(width) * usize::from(height);
	(0..pixels as u32)
		.map(|index| (seed * 0x10_1010 + index * 7) & 0xff_ffff)
		.collect()
}

/// One test, as windows of tests run at once would cover each other
/// without a compositor.
#[test]
fn frames_show_on_a_real_server() {
	through_shared_memory();
	through_put_image();
}

fn through_shared_memory() {
	let size = [96, 40];
	let shown = on_real_window(size, true, |presenter, read| {
		assert!(presenter.shares_memory());
		let mut pixels = frame(1, size);
		assert_eq!(
			presenter.present(&pixels, [96, 40], None),
			Ok(Presented::Shown)
		);
		assert_eq!(read(), pixels);
		for step in 2..8 {
			let rows = (step * 5 % 40, step * 5 % 40 + 3);
			for pixel in &mut pixels[rows.0 * 96..rows.1 * 96] {
				*pixel ^= step as u32;
			}
			// Reading back waits for the server, which then read every buffer.
			let shown = presenter.present(&pixels, [96, 40], Some(&[rows]));
			assert_eq!(shown, Ok(Presented::Shown));
			assert_eq!(read(), pixels, "step {step}");
		}
		// A smaller frame shows in the window's corner.
		let small = frame(9, [10, 4]);
		assert_eq!(
			presenter.present(&small, [10, 4], None),
			Ok(Presented::Shown)
		);
		read()[..10].to_vec() == small[..10]
	});
	assert!(shown.is_none_or(|shown| shown));
}

fn through_put_image() {
	let size = [700, 30];
	let shown = on_real_window(size, false, |presenter, read| {
		assert!(!presenter.shares_memory());
		let pixels = frame(3, size);
		assert_eq!(
			presenter.present(&pixels, [700, 30], None),
			Ok(Presented::Shown)
		);
		read() == pixels
	});
	assert!(shown.is_none_or(|shown| shown));
}
