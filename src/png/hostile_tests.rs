//! Our PNG decoder on damaged, truncated, hostile and unusual files: it
//! refuses what is broken without panicking, bounds what it allocates,
//! and agrees with the `image` crate where the specification leaves room.
use super::{
	decode,
	tests::{FORMATS, Spec, chunk, oracle, random, write},
};
use crate::bench::corpus::Rng;

const BIG: usize = 1 << 20;

fn small(rng: &mut Rng, format: (u8, u8)) -> Vec<u8> {
	write(&random(rng, format, (5, 4), false))
}

/// The chunks of `png` after its signature, as (kind, body).
fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
	let mut rest = &png[8..];
	let mut out = Vec::new();
	while rest.len() >= 12 {
		let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
		out.push((rest[4..8].try_into().unwrap(), rest[8..8 + length].to_vec()));
		rest = &rest[12 + length..];
	}
	out
}

/// A PNG of `parts` after the signature.
fn assemble(parts: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
	let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
	for (kind, body) in parts {
		out.extend(chunk(kind, body));
	}
	out
}

#[test]
fn every_truncation_is_refused_without_panicking() {
	let mut rng = Rng::new(11);
	for format in FORMATS {
		let png = small(&mut rng, format);
		let whole = decode(&png, BIG, BIG).unwrap();
		for len in 0..png.len() {
			// Cut inside IEND, the image is already whole; the spec asks for
			// IEND, so it is refused.
			assert!(
				decode(&png[..len], BIG, BIG).is_err(),
				"{format:?} cut at {len}"
			);
		}
		assert_eq!(decode(&png, BIG, BIG), Ok(whole));
	}
}

#[test]
fn damaged_bytes_fail_their_crc_or_decode_without_panicking() {
	let mut rng = Rng::new(12);
	for round in 0..2000 {
		let mut png = small(&mut rng, FORMATS[round % FORMATS.len()]);
		for _ in 0..1 + rng.below(4) {
			let at = rng.below(png.len());
			png[at] ^= 1 << rng.below(8);
		}
		// Whatever it decides, it decides without panicking, and anything
		// it accepts the image crate accepts the same.
		if let Ok(image) = decode(&png, BIG, BIG) {
			assert_eq!(oracle(&png), Some(image), "round {round}");
		}
	}
}

#[test]
fn a_chunk_with_a_wrong_crc_is_refused() {
	let mut png = small(&mut Rng::new(1), (6, 8));
	let end = png.len();
	png[end - 1] ^= 1;
	assert_eq!(decode(&png, BIG, BIG), Err("bad chunk CRC"));
}

#[test]
fn sizes_past_the_limits_are_refused_before_any_image_is_made() {
	let mut rng = Rng::new(2);
	let png = write(&random(&mut rng, (6, 8), (40, 30), false));
	assert!(decode(&png, 40, 1200).is_ok(), "exactly at the limits");
	assert_eq!(decode(&png, 39, BIG), Err("image too large"));
	assert_eq!(decode(&png, BIG, 1199), Err("image too large"));
	// A header claiming 2^31 - 1 a side with almost no data costs nothing.
	let mut parts = chunks(&png);
	parts[0].1[..8].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff, 0x7f, 0xff, 0xff, 0xff]);
	assert_eq!(
		decode(&assemble(&parts), 1 << 14, 1 << 26),
		Err("image too large")
	);
	parts[0].1[..4].copy_from_slice(&0x8000_0000u32.to_be_bytes());
	assert_eq!(decode(&assemble(&parts), BIG, BIG), Err("bad size"));
}

#[test]
fn headers_the_specification_forbids_are_refused() {
	let mut rng = Rng::new(3);
	let png = small(&mut rng, (2, 8));
	let with = |at: usize, value: u8| {
		let mut parts = chunks(&png);
		parts[0].1[at] = value;
		decode(&assemble(&parts), BIG, BIG)
	};
	for (at, value, why) in [
		(8, 4, "RGB of 4 bits"),
		(8, 12, "12 bits"),
		(9, 1, "color type 1"),
		(9, 7, "color type 7"),
		(10, 1, "compression method 1"),
		(11, 1, "filter method 1"),
		(12, 2, "interlace method 2"),
	] {
		assert!(with(at, value).is_err(), "{why}");
	}
	let mut parts = chunks(&png);
	parts[0].1[..4].copy_from_slice(&[0; 4]);
	assert_eq!(
		decode(&assemble(&parts), BIG, BIG),
		Err("bad size"),
		"no width"
	);
	let mut parts = chunks(&png);
	parts[0].1.pop();
	assert_eq!(
		decode(&assemble(&parts), BIG, BIG),
		Err("bad IHDR"),
		"short IHDR"
	);
}

#[test]
fn chunks_out_of_place_or_missing_are_refused() {
	let mut rng = Rng::new(4);
	let png = small(&mut rng, (3, 8));
	let parts = chunks(&png);
	let mut swapped = parts.clone();
	swapped.swap(0, 1);
	assert_eq!(
		decode(&assemble(&swapped), BIG, BIG),
		Err("IHDR is not first")
	);
	let no_palette: Vec<_> = parts
		.iter()
		.filter(|(k, _)| k != b"PLTE")
		.cloned()
		.collect();
	assert_eq!(decode(&assemble(&no_palette), BIG, BIG), Err("no palette"));
	let no_data: Vec<_> = parts
		.iter()
		.filter(|(k, _)| k != b"IDAT")
		.cloned()
		.collect();
	assert_eq!(decode(&assemble(&no_data), BIG, BIG), Err("no image data"));
	let no_end: Vec<_> = parts
		.iter()
		.filter(|(k, _)| k != b"IEND")
		.cloned()
		.collect();
	assert_eq!(decode(&assemble(&no_end), BIG, BIG), Err("truncated"));
	let mut critical = parts.clone();
	critical.insert(1, (*b"ABCD", vec![1, 2]));
	assert_eq!(
		decode(&assemble(&critical), BIG, BIG),
		Err("unknown critical chunk")
	);
	assert!(decode(b"GIF89a", BIG, BIG).is_err());
	assert!(decode(&[], BIG, BIG).is_err());
}

#[test]
fn ancillary_chunks_split_data_and_bytes_after_the_end_change_nothing() {
	let mut rng = Rng::new(5);
	for format in FORMATS {
		let png = small(&mut rng, format);
		let expected = decode(&png, BIG, BIG).unwrap();
		let mut parts = chunks(&png);
		// Gamma, text and a private chunk, all ancillary.
		parts.insert(1, (*b"gAMA", 45455u32.to_be_bytes().to_vec()));
		parts.insert(1, (*b"tEXt", b"Comment\0hi".to_vec()));
		parts.insert(1, (*b"prIv", vec![9; 7]));
		// The image data split into one-byte IDATs.
		let at = parts.iter().position(|(k, _)| k == b"IDAT").unwrap();
		let data = parts.remove(at).1;
		for (i, byte) in data.iter().enumerate() {
			parts.insert(at + i, (*b"IDAT", vec![*byte]));
		}
		let mut file = assemble(&parts);
		file.extend(b"trailing garbage");
		assert_eq!(decode(&file, BIG, BIG), Ok(expected.clone()), "{format:?}");
		assert_eq!(oracle(&file), Some(expected), "{format:?}, the oracle");
	}
}

#[test]
fn too_little_or_bad_image_data_is_refused() {
	let mut rng = Rng::new(6);
	let png = small(&mut rng, (6, 8));
	let mut parts = chunks(&png);
	let at = parts.iter().position(|(k, _)| k == b"IDAT").unwrap();
	// A valid zlib stream of one row too few.
	let short = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 4 * (1 + 5 * 4) - 1], 6);
	parts[at].1 = short;
	assert_eq!(
		decode(&assemble(&parts), BIG, BIG),
		Err("too little image data")
	);
	parts[at].1 = vec![0x78, 0x9c, 0xff, 0xff];
	assert_eq!(decode(&assemble(&parts), BIG, BIG), Err("bad image data"));
	// More data than the image needs is ignored, as the oracle does.
	let long = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 4 * (1 + 5 * 4) + 100], 6);
	parts[at].1 = long;
	let file = assemble(&parts);
	assert_eq!(decode(&file, BIG, BIG).map(|i| i.pixels), Ok(vec![0; 20]));
	assert_eq!(oracle(&file).map(|i| i.pixels), Some(vec![0; 20]));
}

#[test]
fn palette_indices_past_the_palette_agree_with_the_oracle() {
	// Two entries but index 3 used: the spec calls it an error; what we do
	// must match the image crate, which callers saw so far.
	let spec = Spec {
		width: 2,
		height: 1,
		depth: 2,
		color: 3,
		interlaced: false,
		palette: vec![[10, 20, 30], [40, 50, 60]],
		transparency: None,
		passes: vec![vec![(0, vec![0b0011_0000])]],
	};
	let png = write(&spec);
	assert_eq!(decode(&png, BIG, BIG).ok(), oracle(&png));
}

#[test]
fn a_transparency_chunk_of_the_wrong_size_is_as_the_oracle_reads_it() {
	let mut rng = Rng::new(8);
	for (format, trns) in [
		((0, 8), vec![0, 1, 2]),
		((2, 8), vec![0, 1]),
		((3, 4), vec![7; 20]),
	] {
		let mut spec = random(&mut rng, format, (4, 3), false);
		spec.transparency = Some(trns);
		let png = write(&spec);
		assert_eq!(decode(&png, BIG, BIG).ok(), oracle(&png), "{format:?}");
	}
}
