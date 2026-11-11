//! Tests of opening libxcb, and of the bounds kept on what the X server
//! and the caller hand the presenter: each symbol fills its own field, a
//! library lacking one is refused by name, and malformed setups and sizes
//! whose bytes wrap around are refused.
use super::{client::Client, fake_tests::setup, setup::takes_our_pixels};
use crate::present::memory::Memory;
use std::{
	ffi::{CStr, c_void},
	io::ErrorKind,
	ptr::NonNull,
};

/// Every symbol the client takes from the libraries.
const SYMBOLS: [&CStr; 22] = [
	c"XGetXCBConnection",
	c"xcb_generate_id",
	c"xcb_flush",
	c"xcb_connection_has_error",
	c"xcb_get_setup",
	c"xcb_get_maximum_request_length",
	c"xcb_get_extension_data",
	c"xcb_shm_id",
	c"xcb_shm_query_version",
	c"xcb_shm_query_version_reply",
	c"xcb_shm_attach_fd_checked",
	c"xcb_shm_detach",
	c"xcb_shm_put_image",
	c"xcb_put_image",
	c"xcb_create_gc_checked",
	c"xcb_free_gc",
	c"xcb_get_geometry",
	c"xcb_get_geometry_reply",
	c"xcb_get_input_focus",
	c"xcb_poll_for_reply",
	c"xcb_discard_reply",
	c"xcb_request_check",
];

/// One byte a symbol, so each has an address of its own; none is called.
static SLOTS: [u8; SYMBOLS.len()] = [0; SYMBOLS.len()];

/// A distinct address for each of `SYMBOLS`, and nothing for any other.
fn distinct(name: &CStr) -> Option<NonNull<c_void>> {
	let index = SYMBOLS.iter().position(|&symbol| symbol == name)?;
	NonNull::new((&raw const SLOTS[index]).cast_mut().cast())
}

fn at(name: &CStr) -> usize {
	distinct(name).unwrap().as_ptr() as usize
}

#[test]
fn each_symbol_fills_the_field_of_its_own_function() {
	let client = Client::find(distinct).expect("every symbol");
	// Several share a type, so a mix-up would still compile: the GC and
	// segment frees, the checked and unchecked requests, the version query
	// and the input focus round trip.
	let fields = [
		(client.connection as usize, c"XGetXCBConnection"),
		(client.generate_id as usize, c"xcb_generate_id"),
		(client.flush as usize, c"xcb_flush"),
		(client.has_error as usize, c"xcb_connection_has_error"),
		(client.setup as usize, c"xcb_get_setup"),
		(
			client.maximum_request_length as usize,
			c"xcb_get_maximum_request_length",
		),
		(client.extension_data as usize, c"xcb_get_extension_data"),
		(client.shm as usize, c"xcb_shm_id"),
		(client.query_version as usize, c"xcb_shm_query_version"),
		(
			client.query_version_reply as usize,
			c"xcb_shm_query_version_reply",
		),
		(client.attach_fd as usize, c"xcb_shm_attach_fd_checked"),
		(client.detach as usize, c"xcb_shm_detach"),
		(client.shm_put_image as usize, c"xcb_shm_put_image"),
		(client.put_image as usize, c"xcb_put_image"),
		(client.create_gc as usize, c"xcb_create_gc_checked"),
		(client.free_gc as usize, c"xcb_free_gc"),
		(client.geometry as usize, c"xcb_get_geometry"),
		(client.geometry_reply as usize, c"xcb_get_geometry_reply"),
		(client.input_focus as usize, c"xcb_get_input_focus"),
		(client.poll_for_reply as usize, c"xcb_poll_for_reply"),
		(client.discard_reply as usize, c"xcb_discard_reply"),
		(client.request_check as usize, c"xcb_request_check"),
	];
	for (field, name) in fields {
		assert_eq!(field, at(name), "{name:?}");
	}
}

#[test]
fn libraries_lacking_any_symbol_are_refused_by_name() {
	for lacking in SYMBOLS {
		let found = Client::find(|name| (name != lacking).then(|| distinct(name)).flatten());
		let name = lacking.to_str().unwrap();
		assert_eq!(found.err(), Some(format!("no {name} in libxcb")));
	}
}

/// Image byte order and bitmap bit order: LSBFirst is 0, MSBFirst 1.
const OURS: u8 = if cfg!(target_endian = "little") { 0 } else { 1 };

/// `setup` with `extra` bytes after its formats, counted in its length,
/// as the screens that follow them are.
fn followed_by(mut bytes: Vec<u8>, extra: &[u8]) -> Vec<u8> {
	bytes.extend_from_slice(extra);
	let length = (bytes.len() - 8) / 4;
	bytes[6..8].copy_from_slice(&(length as u16).to_ne_bytes());
	bytes
}

#[test]
fn the_bitmap_bit_order_beside_the_image_byte_order_is_not_ours_to_check() {
	let mut bytes = setup(&[(24, 32)]);
	bytes[31] = 1 - OURS;
	assert!(takes_our_pixels(&bytes, 24));
}

#[test]
fn only_as_many_formats_as_the_setup_counts_are_read() {
	// A screen's first bytes after the one format, looking like another.
	let bytes = followed_by(setup(&[(24, 24)]), &[24, 32, 32, 0, 0, 0, 0, 0]);
	assert!(!takes_our_pixels(&bytes, 24));
	let mut counted = bytes;
	counted[29] = 2;
	assert!(takes_our_pixels(&counted, 24));
}

#[test]
fn formats_wholly_inside_a_cut_setup_are_still_read() {
	let bytes = setup(&[(24, 32), (32, 32)]);
	let cut = &bytes[..bytes.len() - 1];
	assert!(takes_our_pixels(cut, 24));
	assert!(!takes_our_pixels(cut, 32), "cut short of its last byte");
}

#[test]
fn pixel_counts_whose_bytes_wrap_around_to_a_small_size_are_refused() {
	// Four bytes and eight, modulo the address space, but far more pixels.
	for pixels in [usize::MAX / 4 + 2, usize::MAX / 4 + 3] {
		let refused = Memory::new(pixels).err().map(|error| error.kind());
		assert_eq!(refused, Some(ErrorKind::InvalidInput), "{pixels} pixels");
	}
}

/// The lines of this process's mappings of the file `inode`.
#[cfg(target_os = "linux")]
fn mappings_of(inode: u64) -> Vec<String> {
	let maps = std::fs::read_to_string("/proc/self/maps").expect("/proc/self/maps");
	maps
		.lines()
		.filter(|line| line.split_whitespace().nth(4) == Some(&inode.to_string()))
		.map(str::to_owned)
		.collect()
}

#[cfg(target_os = "linux")]
#[test]
fn the_whole_mapping_is_unmapped_on_drop() {
	// Several pages, so unmapping only the first would leave the rest.
	let memory = Memory::new(4 * 4096).unwrap();
	let file = std::fs::File::from(memory.fd().try_clone_to_owned().unwrap());
	let inode = std::os::unix::fs::MetadataExt::ino(&file.metadata().unwrap());
	drop(file);
	assert_eq!(mappings_of(inode).len(), 1, "mapped once while alive");
	drop(memory);
	assert_eq!(mappings_of(inode), Vec::<String>::new());
}
