//! Tests of the fake libwayland itself: each rule it checks must flag the
//! request that breaks it, so the presenter's tests passing means the
//! presenter keeps them.
use super::{
	client::{Argument, Client, DESTROY, Proxy, Queue},
	fake_tests::{DISPLAY, client, install, with},
	memory::Memory,
};
use std::ptr;

/// The objects a well-behaved client makes, up to a buffer on a surface.
struct World {
	client: Client,
	queue: Queue,
	registry: Proxy,
	shm: Proxy,
	pool: Proxy,
	buffer: Proxy,
	surface: Proxy,
	_memory: Memory,
}

const WIDTH: i32 = 4;
const HEIGHT: i32 = 3;

fn send(
	client: &Client,
	proxy: Proxy,
	opcode: u32,
	version: u32,
	flags: u32,
	arguments: &mut [Argument],
) -> Proxy {
	// SAFETY: the fake reads only the arguments of each request's
	// signature, which the tests pass.
	unsafe {
		(client.marshal)(
			proxy,
			opcode,
			ptr::null(),
			version,
			flags,
			arguments.as_mut_ptr(),
		)
	}
}

/// Sends request `opcode` making an object of `interface`.
fn make(
	client: &Client,
	proxy: Proxy,
	opcode: u32,
	interface: *const super::client::Interface,
	version: u32,
	arguments: &mut [Argument],
) -> Proxy {
	// SAFETY: as in `send`.
	unsafe { (client.marshal)(proxy, opcode, interface, version, 0, arguments.as_mut_ptr()) }
}

fn int(value: i32) -> Argument {
	Argument { int: value }
}

fn new_id() -> Argument {
	Argument {
		object: ptr::null_mut(),
	}
}

fn world(surface_version: u32) -> World {
	install();
	let client = client();
	let surface = with(|wayland| wayland.surface(surface_version));
	// SAFETY: the fake's functions, on objects it made.
	let (queue, registry) = unsafe {
		let queue = (client.create_queue)(DISPLAY);
		let wrapper = (client.create_wrapper)(DISPLAY);
		(client.set_queue)(wrapper, queue);
		let registry = make(&client, wrapper, 1, client.registry, 0, &mut [new_id()]);
		(client.destroy_wrapper)(wrapper);
		(queue, registry)
	};
	let mut arguments = [
		Argument { uint: 2 },
		Argument {
			string: c"wl_shm".as_ptr(),
		},
		Argument { uint: 2 },
		new_id(),
	];
	let shm = make(&client, registry, 0, client.shm, 2, &mut arguments);
	let memory = Memory::new((WIDTH * HEIGHT) as usize).unwrap();
	let mut arguments = [
		new_id(),
		Argument { fd: memory.fd() },
		int(WIDTH * HEIGHT * 4),
	];
	let pool = make(&client, shm, 0, client.shm_pool, 2, &mut arguments);
	let mut arguments = [
		new_id(),
		int(0),
		int(WIDTH),
		int(HEIGHT),
		int(WIDTH * 4),
		Argument { uint: 1 },
	];
	let buffer = make(&client, pool, 0, client.buffer, 2, &mut arguments);
	assert_eq!(
		broken(),
		Vec::<String>::new(),
		"the world itself keeps the rules"
	);
	World {
		client,
		queue,
		registry,
		shm,
		pool,
		buffer,
		surface,
		_memory: memory,
	}
}

fn broken() -> Vec<String> {
	with(|wayland| std::mem::take(&mut wayland.broken))
}

/// Asserts exactly one rule broke, with `words` in its message.
fn flagged(words: &str) {
	let broken = broken();
	assert!(
		broken.len() == 1 && broken[0].contains(words),
		"{words:?} in {broken:#?}"
	);
}

#[test]
fn requests_on_destroyed_proxies_at_the_wrong_version_or_unknown_are_flagged() {
	let world = world(6);
	send(&world.client, world.buffer, 0, 2, DESTROY, &mut []);
	assert_eq!(broken(), Vec::<String>::new());
	send(&world.client, world.buffer, 0, 2, DESTROY, &mut []);
	flagged("dead or unknown proxy");
	send(&world.client, world.pool, 1, 1, DESTROY, &mut []);
	flagged("sent at v1, not v2");
	send(&world.client, world.shm, 5, 2, 0, &mut []);
	flagged("unknown Shm request 5");
}

#[test]
fn destructors_without_the_destroy_flag_and_the_flag_elsewhere_are_flagged() {
	let world = world(6);
	send(&world.client, world.pool, 1, 2, 0, &mut []);
	flagged("destroy flag 0 on Pool request 1");
	send(&world.client, world.surface, 6, 6, DESTROY, &mut []);
	flagged("destroy flag 1 on Surface request 6");
}

#[test]
fn objects_with_destructors_must_not_be_destroyed_as_bare_proxies() {
	let world = world(6);
	// SAFETY: the fake's own functions.
	unsafe {
		(world.client.destroy_proxy)(world.buffer);
		flagged("destroy_proxy on a v2 Buffer");
		(world.client.destroy_proxy)(world.shm);
		flagged("destroy_proxy on a v2 Shm");
		// The registry has no destructor request.
		(world.client.destroy_proxy)(world.registry);
	}
	assert_eq!(broken(), Vec::<String>::new());
}

#[test]
fn releasing_a_version_1_wl_shm_is_flagged() {
	let world = world(6);
	let mut arguments = [
		Argument { uint: 2 },
		Argument {
			string: c"wl_shm".as_ptr(),
		},
		Argument { uint: 1 },
		new_id(),
	];
	let old = make(
		&world.client,
		world.registry,
		0,
		world.client.shm,
		1,
		&mut arguments,
	);
	assert_eq!(broken(), Vec::<String>::new());
	send(&world.client, old, 1, 1, DESTROY, &mut []);
	flagged("wl_shm.release on a v1 wl_shm");
}

#[test]
fn binding_what_is_not_offered_or_above_its_version_is_flagged() {
	let world = world(6);
	for (name, interface, version, words) in [
		(2, c"wl_shm", 3, "bound global 2 as \"wl_shm\" v3"),
		(9, c"wl_shm", 1, "bound global 9"),
		(1, c"wl_shm", 1, "bound global 1"),
	] {
		let mut arguments = [
			Argument { uint: name },
			Argument {
				string: interface.as_ptr(),
			},
			Argument { uint: version },
			new_id(),
		];
		make(
			&world.client,
			world.registry,
			0,
			world.client.shm,
			version,
			&mut arguments,
		);
		flagged(words);
	}
}

#[test]
fn pools_larger_than_their_memory_or_of_bad_descriptors_are_flagged() {
	let world = world(6);
	let memory = Memory::new(4).unwrap();
	let mut arguments = [new_id(), Argument { fd: memory.fd() }, int(17)];
	make(
		&world.client,
		world.shm,
		0,
		world.client.shm_pool,
		2,
		&mut arguments,
	);
	flagged("pool of 17 bytes over a 16-byte file");
	let mut arguments = [new_id(), Argument { fd: -1 }, int(16)];
	make(
		&world.client,
		world.shm,
		0,
		world.client.shm_pool,
		2,
		&mut arguments,
	);
	flagged("bad descriptor -1");
}

#[test]
fn buffers_outside_their_pool_with_short_strides_or_unoffered_formats_are_flagged() {
	let world = world(6);
	let cases = [
		(
			[int(4), int(WIDTH), int(HEIGHT), int(WIDTH * 4)],
			1,
			"stride 16 at 4",
		),
		(
			[int(0), int(WIDTH), int(HEIGHT), int(WIDTH * 4 - 1)],
			1,
			"stride 15",
		),
		(
			[int(0), int(0), int(HEIGHT), int(WIDTH * 4)],
			1,
			"0×3 buffer",
		),
		([int(-4), int(1), int(1), int(4)], 1, "at -4"),
		(
			[int(0), int(WIDTH), int(HEIGHT), int(WIDTH * 4)],
			7,
			"format 7",
		),
	];
	for ([offset, width, height, stride], format, words) in cases {
		let mut arguments = [
			new_id(),
			offset,
			width,
			height,
			stride,
			Argument { uint: format },
		];
		make(
			&world.client,
			world.pool,
			0,
			world.client.buffer,
			2,
			&mut arguments,
		);
		flagged(words);
	}
}

#[test]
fn committing_a_held_or_destroyed_buffer_or_damage_outside_it_is_flagged() {
	let world = world(6);
	let attach = |buffer| {
		send(
			&world.client,
			world.surface,
			1,
			6,
			0,
			&mut [Argument { object: buffer }, int(0), int(0)],
		)
	};
	let commit = || send(&world.client, world.surface, 6, 6, 0, &mut []);
	attach(world.buffer);
	commit();
	assert_eq!(broken(), Vec::<String>::new());
	attach(world.buffer);
	commit();
	flagged("which the compositor holds");
	with(|wayland| wayland.release(world.buffer as usize));
	attach(world.buffer);
	for (x, y, w, h) in [
		(0, 0, WIDTH + 1, 1),
		(0, HEIGHT, WIDTH, 1),
		(-1, 0, 1, 1),
		(0, 0, 0, 1),
	] {
		send(
			&world.client,
			world.surface,
			9,
			6,
			0,
			&mut [int(x), int(y), int(w), int(h)],
		);
	}
	commit();
	assert_eq!(broken().len(), 4);
	with(|wayland| wayland.release(world.buffer as usize));
	attach(world.buffer);
	send(&world.client, world.buffer, 0, 2, DESTROY, &mut []);
	commit();
	flagged("committed dead buffer");
	attach(world.buffer);
	flagged("attached dead or unknown buffer");
}

#[test]
fn buffer_damage_on_surfaces_before_version_4_is_flagged() {
	let world = world(3);
	send(
		&world.client,
		world.surface,
		9,
		3,
		0,
		&mut [int(0), int(0), int(1), int(1)],
	);
	flagged("damage_buffer on a v3 wl_surface");
	send(
		&world.client,
		world.surface,
		2,
		3,
		0,
		&mut [int(0), int(0), int(1), int(1)],
	);
	assert_eq!(broken(), Vec::<String>::new());
}

#[test]
fn destroying_a_queue_with_proxies_on_it_or_twice_is_flagged() {
	let world = world(6);
	// SAFETY: the fake's own functions.
	unsafe {
		(world.client.destroy_queue)(world.queue);
		flagged("with live proxies on it");
		(world.client.destroy_queue)(world.queue);
		// Twice, and still with them.
		let twice = broken();
		assert_eq!(twice.len(), 2);
		assert!(twice[0].contains("destroyed dead or unknown queue"));
		(world.client.dispatch_pending)(DISPLAY, world.queue);
		// The world's events wait without dispatchers, which is flagged too.
		assert!(broken()[0].contains("dispatched dead or unknown queue"));
	}
}

#[test]
fn a_second_dispatcher_and_events_for_proxies_without_one_are_flagged() {
	unsafe extern "C" fn ignore(
		_: *const std::ffi::c_void,
		_: Proxy,
		_: u32,
		_: *const std::ffi::c_void,
		_: *mut Argument,
	) -> i32 {
		0
	}
	let world = world(6);
	// SAFETY: the fake's own functions.
	unsafe {
		// The world's objects have no dispatchers: the registry's globals
		// and wl_shm's formats wait undelivered.
		(world.client.dispatch_pending)(DISPLAY, world.queue);
		let broken = broken();
		assert!(
			!broken.is_empty() && broken.iter().all(|rule| rule.contains("has no dispatcher")),
			"{broken:#?}"
		);
		assert_eq!(
			(world.client.add_dispatcher)(world.buffer, ignore, ptr::null(), ptr::null_mut()),
			0
		);
		assert_eq!(
			(world.client.add_dispatcher)(world.buffer, ignore, ptr::null(), ptr::null_mut()),
			-1
		);
		flagged("second dispatcher");
	}
}

#[test]
fn events_for_destroyed_proxies_are_dropped_as_libwayland_drops_them() {
	let world = world(6);
	let mut attach = [
		Argument {
			object: world.buffer,
		},
		int(0),
		int(0),
	];
	send(&world.client, world.surface, 1, 6, 0, &mut attach);
	send(&world.client, world.surface, 6, 6, 0, &mut []);
	with(|wayland| wayland.release(world.buffer as usize));
	send(&world.client, world.buffer, 0, 2, DESTROY, &mut []);
	send(&world.client, world.shm, 1, 2, DESTROY, &mut []);
	// SAFETY: the fake's own functions.
	unsafe { (world.client.destroy_proxy)(world.registry) };
	broken();
	// SAFETY: as above.
	let delivered = unsafe { (world.client.dispatch_pending)(DISPLAY, world.queue) };
	assert!(delivered > 0);
	assert_eq!(broken(), Vec::<String>::new());
}
