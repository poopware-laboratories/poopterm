//! Tests of what the presenter's dispatcher records from the events on its
//! queue: globals from the registry, our pixel format from `wl_shm`,
//! releases from buffers, and nothing from anything else.
use super::{
	client::{Client, Proxy, Queue},
	events::{Listener, XRGB8888},
	fake_tests::{DISPLAY, client, install, with},
};

/// A listener on a fresh compositor, listening to a registry, a `wl_shm`
/// and a buffer-like proxy on one queue.
struct Setup {
	client: Client,
	queue: Queue,
	listener: Listener,
	proxies: [usize; 3],
}

fn setup() -> Setup {
	install();
	let client = client();
	// SAFETY: the fake's own function.
	let queue = unsafe { (client.create_queue)(DISPLAY) };
	let made = with(|wayland| [0; 3].map(|_| wayland.surface(1) as usize));
	let mut listener = Listener::new();
	for &proxy in &made {
		with(|wayland| wayland.objects.get_mut(&proxy).unwrap().queue = queue as usize);
		// SAFETY: the fake's proxies, dispatched from this thread, never
		// destroyed while the listener lives.
		unsafe { listener.listen(&client, proxy as Proxy) };
	}
	listener.events().registry = made[0] as Proxy;
	listener.events().shm = made[1] as Proxy;
	Setup {
		client,
		queue,
		listener,
		proxies: made,
	}
}

impl Setup {
	fn dispatch(&mut self, events: &[(usize, u32, &[u32], Option<&std::ffi::CStr>)]) {
		for &(target, opcode, uints, text) in events {
			with(|wayland| wayland.event(self.queue as usize, target, opcode, uints, text));
		}
		// SAFETY: the fake's own function, on the queue it made.
		let delivered = unsafe { (self.client.dispatch_pending)(DISPLAY, self.queue) };
		assert_eq!(delivered, events.len() as i32);
	}
}

#[test]
fn globals_are_recorded_in_order_and_removals_ignored() {
	let mut setup = setup();
	let registry = setup.proxies[0];
	setup.dispatch(&[
		(registry, 0, &[1, 6], Some(c"wl_compositor")),
		(registry, 0, &[7, 2], Some(c"wl_shm")),
		// global_remove: the presenter binds once, at start, and keeps it.
		(registry, 1, &[1], None),
	]);
	let events = setup.listener.events();
	assert_eq!(
		events.globals,
		[
			(1, b"wl_compositor".to_vec(), 6),
			(7, b"wl_shm".to_vec(), 2)
		]
	);
	assert!(events.released.is_empty());
}

#[test]
fn only_xrgb8888_among_the_formats_counts() {
	let mut setup = setup();
	let shm = setup.proxies[1];
	setup.dispatch(&[(shm, 0, &[0], None), (shm, 0, &[0x3432_5258], None)]);
	assert!(!setup.listener.events().xrgb);
	setup.dispatch(&[(shm, 0, &[XRGB8888], None), (shm, 0, &[0], None)]);
	assert!(setup.listener.events().xrgb, "and stays once offered");
	assert!(setup.listener.events().released.is_empty());
}

#[test]
fn a_buffers_first_event_is_its_release_and_other_events_are_ignored() {
	let mut setup = setup();
	let buffer = setup.proxies[2];
	setup.dispatch(&[
		(buffer, 0, &[], None),
		(buffer, 1, &[], None),
		(buffer, 0, &[], None),
	]);
	let events = setup.listener.events();
	assert_eq!(events.released, [buffer as Proxy, buffer as Proxy]);
	assert!(events.globals.is_empty());
	assert!(!events.xrgb);
}
