//! Tests of what the connection setup says about our pixels: the formats
//! the server lists and its image byte order, read from setups built as
//! the X protocol lays them out, and setups too short to hold what they
//! claim.
use super::setup::{put_rows, takes_our_pixels};

/// Image byte order: LSBFirst is 0 and MSBFirst 1.
const OURS: u8 = if cfg!(target_endian = "little") { 0 } else { 1 };
const THEIRS: u8 = 1 - OURS;

/// A setup as the server sends it: `vendor`, then each `(depth,
/// bits_per_pixel)` format, with no screens after them.
fn setup(vendor: &[u8], formats: &[(u8, u8)], byte_order: u8) -> Vec<u8> {
	let padded = vendor.len().div_ceil(4) * 4;
	let mut bytes = vec![0; 40];
	bytes[0] = 1;
	bytes[2..4].copy_from_slice(&11u16.to_ne_bytes());
	bytes[24..26].copy_from_slice(&(vendor.len() as u16).to_ne_bytes());
	bytes[29] = formats.len() as u8;
	bytes[30] = byte_order;
	bytes.extend_from_slice(vendor);
	bytes.resize(40 + padded, 0);
	for &(depth, bits) in formats {
		bytes.extend_from_slice(&[depth, bits, 32, 0, 0, 0, 0, 0]);
	}
	let length = (bytes.len() - 8) / 4;
	bytes[6..8].copy_from_slice(&(length as u16).to_ne_bytes());
	bytes
}

const COMMON: [(u8, u8); 4] = [(1, 1), (8, 8), (24, 32), (32, 32)];

#[test]
fn depth_24_and_32_windows_take_32_bit_pixels_in_our_byte_order() {
	let bytes = setup(b"The X.Org Foundation", &COMMON, OURS);
	assert!(takes_our_pixels(&bytes, 24));
	assert!(takes_our_pixels(&bytes, 32));
}

#[test]
fn depths_the_server_packs_otherwise_or_does_not_list_are_refused() {
	let bytes = setup(b"Xvfb", &[(16, 16), (24, 24), (30, 32)], OURS);
	assert!(!takes_our_pixels(&bytes, 16), "16 bits per pixel");
	assert!(!takes_our_pixels(&bytes, 24), "packed 24-bit pixels");
	assert!(!takes_our_pixels(&bytes, 32), "not listed");
	assert!(takes_our_pixels(&bytes, 30), "listed with 32 bits");
}

#[test]
fn a_server_of_the_other_byte_order_is_refused() {
	let bytes = setup(b"X", &COMMON, THEIRS);
	assert!(!takes_our_pixels(&bytes, 24));
}

#[test]
fn the_vendor_string_is_skipped_with_its_padding_whatever_its_length() {
	for length in 0..=9 {
		let vendor = vec![b'v'; length];
		let bytes = setup(&vendor, &COMMON, OURS);
		assert!(takes_our_pixels(&bytes, 24), "vendor of {length} bytes");
		assert!(!takes_our_pixels(&bytes, 16), "vendor of {length} bytes");
	}
}

#[test]
fn setups_too_short_for_what_they_claim_are_refused() {
	let bytes = setup(b"vendor", &COMMON, OURS);
	for cut in 0..bytes.len() {
		// Every cut loses the depth 32 format, the last one.
		assert!(!takes_our_pixels(&bytes[..cut], 32), "cut at {cut}");
	}
	// A setup whose length says less than its formats need.
	let mut short = bytes.clone();
	short[6..8].copy_from_slice(&5u16.to_ne_bytes());
	assert!(!takes_our_pixels(&short, 32));
	// A failed setup lists nothing.
	let mut failed = bytes;
	failed[0] = 0;
	assert!(!takes_our_pixels(&failed, 24));
}

#[test]
fn whole_rows_fill_each_request_up_to_the_server_maximum() {
	// A request is 24 bytes of header, then 4 bytes a pixel.
	assert_eq!(
		put_rows(10, 7, 24 + 3 * 40),
		Some(vec![(0, 3), (3, 6), (6, 7)])
	);
	assert_eq!(put_rows(10, 6, 24 + 3 * 40), Some(vec![(0, 3), (3, 6)]));
	assert_eq!(
		put_rows(10, 6, 24 + 3 * 40 + 39),
		Some(vec![(0, 3), (3, 6)])
	);
	assert_eq!(put_rows(10, 3, 1 << 20), Some(vec![(0, 3)]));
}

#[test]
fn rows_wider_than_a_request_cannot_be_put() {
	assert_eq!(put_rows(10, 2, 24 + 40), Some(vec![(0, 1), (1, 2)]));
	assert_eq!(put_rows(10, 2, 24 + 39), None);
	assert_eq!(put_rows(10, 2, 0), None);
}

#[test]
fn a_frame_without_area_needs_no_request() {
	assert_eq!(put_rows(0, 5, 1 << 20), Some(Vec::new()));
	assert_eq!(put_rows(10, 0, 1 << 20), Some(Vec::new()));
}
