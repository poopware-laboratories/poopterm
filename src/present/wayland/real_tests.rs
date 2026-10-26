//! The presenter against the system's libwayland-client and a running
//! compositor: frames on a surface of our own, resized and partly
//! damaged, must show without a protocol error. Skipped without
//! `WAYLAND_DISPLAY`, unless `POOPTERM_REQUIRE_WAYLAND` is set, as in CI
//! (headless weston).
use super::client::{Argument, Client, LIBRARY, Proxy};
use super::presenter::{Presented, Presenter};
use crate::library::Library;
use std::{
	ffi::{CStr, c_char, c_int, c_void},
	ptr::{self, NonNull},
};

type Connect = unsafe extern "C" fn(*const c_char) -> Proxy;
type Disconnect = unsafe extern "C" fn(Proxy);
type Roundtrip = unsafe extern "C" fn(Proxy) -> c_int;
type GetError = unsafe extern "C" fn(Proxy) -> c_int;

/// The functions the test needs besides the presenter's.
struct Extra {
	connect: Connect,
	disconnect: Disconnect,
	roundtrip: Roundtrip,
	error: GetError,
	compositor: NonNull<c_void>,
	surface: NonNull<c_void>,
	_library: Library,
}

fn extra() -> Extra {
	let library = Library::open(LIBRARY).expect("libwayland-client");
	let symbol = |name: &CStr| library.symbol(name).unwrap_or_else(|| panic!("{name:?}"));
	// SAFETY: each is the function wayland-client-core.h declares with
	// that type.
	unsafe {
		Extra {
			connect: std::mem::transmute::<NonNull<c_void>, Connect>(symbol(c"wl_display_connect")),
			disconnect: std::mem::transmute::<NonNull<c_void>, Disconnect>(symbol(
				c"wl_display_disconnect",
			)),
			roundtrip: std::mem::transmute::<NonNull<c_void>, Roundtrip>(symbol(c"wl_display_roundtrip")),
			error: std::mem::transmute::<NonNull<c_void>, GetError>(symbol(c"wl_display_get_error")),
			compositor: symbol(c"wl_compositor_interface"),
			surface: symbol(c"wl_surface_interface"),
			_library: library,
		}
	}
}

/// Notes the `wl_compositor` global's name and version.
unsafe extern "C" fn global(
	data: *const c_void,
	_: Proxy,
	opcode: u32,
	_: *const c_void,
	arguments: *mut Argument,
) -> c_int {
	// SAFETY: `data` is the test's `Option<(u32, u32)>`, and `global`
	// events carry name, interface and version.
	unsafe {
		let found = &mut *data.cast_mut().cast::<Option<(u32, u32)>>();
		let argument = |index| *arguments.add(index);
		if opcode == 0 && CStr::from_ptr(argument(1).string) == c"wl_compositor" {
			*found = Some((argument(0).uint, argument(2).uint));
		}
	}
	0
}

#[test]
fn frames_show_on_a_real_compositor_without_a_protocol_error() {
	let required = std::env::var_os("POOPTERM_REQUIRE_WAYLAND").is_some();
	if std::env::var_os("WAYLAND_DISPLAY").is_none() && !required {
		eprintln!("skipped: no WAYLAND_DISPLAY");
		return;
	}
	let client = Client::load().expect("libwayland-client");
	let extra = extra();
	let (marshal, version_of, destroy_proxy) = (client.marshal, client.version, client.destroy_proxy);
	// SAFETY: the display is connected and checked; every request passes
	// its signature's arguments, and every proxy is destroyed before the
	// display is disconnected.
	unsafe {
		let display = (extra.connect)(ptr::null());
		assert!(!display.is_null(), "no compositor to connect to");
		let mut new = [Argument {
			object: ptr::null_mut(),
		}];
		let registry = marshal(display, 1, client.registry, 0, 0, new.as_mut_ptr());
		let mut found: Option<(u32, u32)> = None;
		let data = ptr::from_mut(&mut found).cast_const().cast();
		(client.add_dispatcher)(registry, global, data, ptr::null_mut());
		assert!((extra.roundtrip)(display) >= 0);
		let (name, offered) = found.expect("a wl_compositor");
		let version = offered.min(4);
		let mut arguments = [
			Argument { uint: name },
			Argument {
				string: c"wl_compositor".as_ptr(),
			},
			Argument { uint: version },
			Argument {
				object: ptr::null_mut(),
			},
		];
		let interface = extra.compositor.as_ptr().cast_const().cast();
		let compositor = marshal(registry, 0, interface, version, 0, arguments.as_mut_ptr());
		let interface = extra.surface.as_ptr().cast_const().cast();
		let surface = marshal(compositor, 0, interface, version, 0, new.as_mut_ptr());
		assert!(!surface.is_null());

		let mut presenter = Presenter::new(client, display, surface, [64, 48]).expect("wl_shm");
		let sizes = [[64, 48], [64, 48], [32, 80], [32, 80], [32, 80]];
		let mut shown = 0;
		for (number, size) in sizes.into_iter().enumerate() {
			let pixels = vec![0x00ff_8000 + number as u32; (size[0] * size[1]) as usize];
			let bands = (number % 2 == 1).then_some(&[(3, 9)][..]);
			match presenter.present(&pixels, size, bands).expect("presented") {
				Presented::Shown => shown += 1,
				Presented::Busy => {}
			}
			assert!((extra.roundtrip)(display) >= 0);
		}
		// At least as many as the presenter may make buffers for.
		assert!(shown >= 3, "{shown} frames shown");
		drop(presenter);
		assert!((extra.roundtrip)(display) >= 0);
		assert_eq!((extra.error)(display), 0, "protocol error");
		// wl_surface.destroy is request 0; the registry and wl_compositor
		// have no destructor.
		marshal(
			surface,
			0,
			ptr::null(),
			version_of(surface),
			1,
			ptr::null_mut(),
		);
		destroy_proxy(compositor);
		destroy_proxy(registry);
		(extra.disconnect)(display);
	}
}
