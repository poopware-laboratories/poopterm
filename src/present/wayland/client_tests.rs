//! Tests of opening libwayland-client: a library lacking any function or
//! interface the presenter uses is refused, naming what it lacks.
use super::{client::Client, fake_tests::lookup};
use std::ffi::CStr;

const SYMBOLS: [&CStr; 16] = [
	c"wl_display_create_queue",
	c"wl_event_queue_destroy",
	c"wl_proxy_create_wrapper",
	c"wl_proxy_wrapper_destroy",
	c"wl_proxy_set_queue",
	c"wl_proxy_marshal_array_flags",
	c"wl_proxy_add_dispatcher",
	c"wl_proxy_destroy",
	c"wl_proxy_get_version",
	c"wl_display_dispatch_queue_pending",
	c"wl_display_roundtrip_queue",
	c"wl_display_flush",
	c"wl_registry_interface",
	c"wl_shm_interface",
	c"wl_shm_pool_interface",
	c"wl_buffer_interface",
];

#[test]
fn a_library_lacking_any_symbol_is_refused_by_name() {
	assert!(Client::find(lookup).is_ok());
	for lacking in SYMBOLS {
		let found = Client::find(|name| (name != lacking).then(|| lookup(name)).flatten());
		let name = lacking.to_str().unwrap();
		assert_eq!(
			found.err(),
			Some(format!("libwayland-client.so.0 lacks {name}"))
		);
	}
}

#[test]
fn the_systems_library_has_every_symbol_when_installed() {
	let installed = crate::library::Library::open(super::client::LIBRARY).is_ok();
	if installed || std::env::var_os("POOPTERM_REQUIRE_WAYLAND").is_some() {
		Client::load().expect("every symbol");
	}
}
