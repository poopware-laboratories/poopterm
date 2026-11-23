//! Gzip (RFC 1952) members: the CRC-32 they end with, every optional header
//! field, and the members a package download must refuse.
use super::{
	gzip::{crc32, gzip},
	tests::{LIMIT, sample},
};
use crate::bench::corpus::Rng;

/// A gzip member of `data` with `flags` and the optional fields they ask
/// for, compressed by `miniz_oxide` as raw DEFLATE.
fn member(data: &[u8], flags: u8) -> Vec<u8> {
	let mut out = vec![0x1f, 0x8b, 8, flags, 1, 2, 3, 4, 0, 3];
	if flags & 4 != 0 {
		out.extend_from_slice(&[5, 0, b'P', b'T', 1, 0, 9]);
	}
	if flags & 8 != 0 {
		out.extend_from_slice(b"name.tar\0");
	}
	if flags & 16 != 0 {
		out.extend_from_slice(b"a comment\0");
	}
	if flags & 2 != 0 {
		let crc = crc32(&out) as u16;
		out.extend_from_slice(&crc.to_le_bytes());
	}
	out.extend(miniz_oxide::deflate::compress_to_vec(data, 6));
	out.extend_from_slice(&crc32(data).to_le_bytes());
	out.extend_from_slice(&(data.len() as u32).to_le_bytes());
	out
}

#[test]
fn crc32_matches_the_standards_check_values() {
	assert_eq!(crc32(b""), 0);
	assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
	assert_eq!(crc32(b"a"), 0xe8b7_be43);
	assert_eq!(
		crc32(b"The quick brown fox jumps over the lazy dog"),
		0x414f_a339
	);
}

#[test]
fn a_member_with_no_optional_fields_gives_back_its_data() {
	let data = b"hello, package\n".repeat(50);
	assert_eq!(gzip(&member(&data, 0), LIMIT), Some(data));
	assert_eq!(gzip(&member(b"", 0), LIMIT), Some(Vec::new()));
}

#[test]
fn every_optional_header_field_is_skipped() {
	let data = b"fields".repeat(20);
	for flags in [2, 4, 8, 16, 2 | 4 | 8 | 16, 4 | 16] {
		assert_eq!(
			gzip(&member(&data, flags), LIMIT),
			Some(data.clone()),
			"{flags:#x}"
		);
	}
}

#[test]
fn every_kind_of_data_decodes_to_what_was_compressed() {
	let mut rng = Rng::new(7);
	for kind in 0..4 {
		for size in [1, 100, 70_000] {
			let data = sample(&mut rng, kind, size);
			assert_eq!(gzip(&member(&data, 8), LIMIT), Some(data), "{kind} {size}");
		}
	}
}

#[test]
fn what_the_systems_gzip_writes_decodes() {
	let data = b"made by gzip(1)\n".repeat(300);
	let mut child = std::process::Command::new("gzip")
		.arg("-c9")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.spawn()
		.expect("gzip");
	use std::io::Write;
	child.stdin.take().unwrap().write_all(&data).unwrap();
	let out = child.wait_with_output().unwrap();
	assert_eq!(gzip(&out.stdout, LIMIT), Some(data));
}

#[test]
fn a_wrong_magic_method_or_reserved_flag_is_refused() {
	let good = member(b"data", 0);
	for (at, value) in [(0, 0x1e), (1, 0x8c), (2, 7), (3, 0x20), (3, 0x80)] {
		let mut bad = good.clone();
		bad[at] = value;
		assert_eq!(gzip(&bad, LIMIT), None, "byte {at} = {value:#x}");
	}
}

#[test]
fn a_damaged_crc_or_size_is_refused() {
	let good = member(b"checked data", 0);
	let len = good.len();
	for at in [len - 8, len - 5, len - 4, len - 1] {
		let mut bad = good.clone();
		bad[at] ^= 1;
		assert_eq!(gzip(&bad, LIMIT), None, "byte {at}");
	}
	let mut header = member(b"x", 2);
	header[10] ^= 1;
	assert_eq!(gzip(&header, LIMIT), None, "a damaged header CRC");
}

#[test]
fn a_truncated_member_is_refused_wherever_it_stops() {
	let good = member(&b"cut short ".repeat(30), 8 | 16);
	for end in 0..good.len() {
		assert_eq!(gzip(&good[..end], LIMIT), None, "{end} of {}", good.len());
	}
}

#[test]
fn output_past_the_limit_is_refused() {
	let data = vec![b'z'; 10_000];
	let stream = member(&data, 0);
	assert_eq!(gzip(&stream, 9_999), None);
	assert_eq!(gzip(&stream, 10_000), Some(data));
}

#[test]
fn unterminated_name_or_comment_is_refused() {
	let mut stream = vec![0x1f, 0x8b, 8, 8, 0, 0, 0, 0, 0, 3];
	stream.extend_from_slice(b"no end");
	assert_eq!(gzip(&stream, LIMIT), None);
	let mut extra = vec![0x1f, 0x8b, 8, 4, 0, 0, 0, 0, 0, 3, 200, 0];
	extra.extend_from_slice(&[0; 10]);
	assert_eq!(
		gzip(&extra, LIMIT),
		None,
		"an extra field longer than the data"
	);
}
