//! SHA-256 against the standard's examples and the system's `shasum`.
use super::sha256::{hex, sha256};
use crate::bench::corpus::Rng;

#[test]
fn the_standards_examples_hash_as_published() {
	assert_eq!(
		hex(&sha256(b"")),
		"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
	);
	assert_eq!(
		hex(&sha256(b"abc")),
		"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
	);
	assert_eq!(
		hex(&sha256(
			b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
		)),
		"248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
	);
	assert_eq!(
		hex(&sha256(&vec![b'a'; 1_000_000])),
		"cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
	);
}

#[test]
fn every_length_around_a_block_boundary_matches_shasum() {
	let mut rng = Rng::new(3);
	let lengths = [55, 56, 57, 63, 64, 65, 119, 120, 128, 1000];
	let inputs: Vec<Vec<u8>> = lengths
		.iter()
		.map(|&len| (0..len).map(|_| rng.below(256) as u8).collect())
		.collect();
	for data in inputs {
		assert_eq!(hex(&sha256(&data)), shasum(&data), "{} bytes", data.len());
	}
}

/// The system's SHA-256 of `data`: macOS and most Linuxes have `shasum`,
/// FreeBSD and Alpine `sha256sum`.
fn shasum(data: &[u8]) -> String {
	use std::io::Write;
	let spawn = |name: &str, args: &[&str]| {
		std::process::Command::new(name)
			.args(args)
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.spawn()
	};
	let mut child = spawn("shasum", &["-a", "256"])
		.or_else(|_| spawn("sha256sum", &[]))
		.expect("shasum or sha256sum");
	child.stdin.take().unwrap().write_all(data).unwrap();
	let out = child.wait_with_output().unwrap();
	String::from_utf8(out.stdout).unwrap()[..64].to_owned()
}

#[test]
fn hex_is_lower_case_two_digits_a_byte() {
	assert_eq!(hex(&[0, 1, 0xab, 0xff]), "0001abff");
	assert_eq!(hex(&[]), "");
}
