//! Our PNG decoder's sample handling, by hand from the specification:
//! sizes of rows and pixels per format, bits unpacked from each position of
//! a byte, samples scaled to eight bits, palettes and transparency.
use super::{
	chunks::Header,
	decode,
	tests::{Spec, oracle, write},
};

const BIG: usize = 1 << 20;

fn header(color: u8, depth: u8) -> Header {
	Header {
		width: 1,
		height: 1,
		depth,
		color,
		interlaced: false,
	}
}

#[test]
fn rows_hold_whole_bytes_of_packed_samples() {
	// (color, depth, channels, bytes a pixel for filters, bytes in 1, 3, 9 pixels)
	let table: [(u8, u8, usize, usize, [usize; 3]); 15] = [
		(0, 1, 1, 1, [1, 1, 2]),
		(0, 2, 1, 1, [1, 1, 3]),
		(0, 4, 1, 1, [1, 2, 5]),
		(0, 8, 1, 1, [1, 3, 9]),
		(0, 16, 1, 2, [2, 6, 18]),
		(2, 8, 3, 3, [3, 9, 27]),
		(2, 16, 3, 6, [6, 18, 54]),
		(3, 1, 1, 1, [1, 1, 2]),
		(3, 2, 1, 1, [1, 1, 3]),
		(3, 4, 1, 1, [1, 2, 5]),
		(3, 8, 1, 1, [1, 3, 9]),
		(4, 8, 2, 2, [2, 6, 18]),
		(4, 16, 2, 4, [4, 12, 36]),
		(6, 8, 4, 4, [4, 12, 36]),
		(6, 16, 4, 8, [8, 24, 72]),
	];
	for (color, depth, channels, bpp, rows) in table {
		let h = header(color, depth);
		assert_eq!(
			(h.channels(), h.bytes_per_pixel()),
			(channels, bpp),
			"{color}/{depth}"
		);
		let widths = [1, 3, 9].map(|w| h.row_bytes(w).unwrap());
		assert_eq!(widths, rows, "{color}/{depth}");
		assert_eq!(h.row_bytes(0), None, "an empty pass has no rows");
	}
}

/// A one-row gray image of `depth`, its bytes `row`, `width` pixels.
fn gray(depth: u8, width: u32, row: Vec<u8>, transparency: Option<Vec<u8>>) -> Vec<u32> {
	let spec = Spec {
		width,
		height: 1,
		depth,
		color: 0,
		interlaced: false,
		palette: Vec::new(),
		transparency,
		passes: vec![vec![(0, row)]],
	};
	let png = write(&spec);
	let ours = decode(&png, BIG, BIG).unwrap();
	assert_eq!(Some(&ours), oracle(&png).as_ref(), "{depth} bits");
	ours.pixels
}

fn grays(levels: &[u32]) -> Vec<u32> {
	levels
		.iter()
		.map(|&g| 0xff00_0000 | g << 16 | g << 8 | g)
		.collect()
}

#[test]
fn low_bit_samples_unpack_from_the_high_bits_first_and_scale_to_full_range() {
	assert_eq!(
		gray(1, 8, vec![0b1011_0001], None),
		grays(&[255, 0, 255, 255, 0, 0, 0, 255])
	);
	assert_eq!(
		gray(2, 4, vec![0b00_01_10_11], None),
		grays(&[0, 85, 170, 255])
	);
	assert_eq!(gray(4, 2, vec![0x0f], None), grays(&[0, 255]));
	assert_eq!(
		gray(4, 3, vec![0x18, 0x70], None),
		grays(&[17, 136, 119]),
		"an odd width leaves padding"
	);
}

#[test]
fn sixteen_bit_samples_round_to_the_nearest_eight_bit_value() {
	let row = |values: &[u16]| values.iter().flat_map(|v| v.to_be_bytes()).collect();
	// 257 maps each 8-bit value to 16 bits exactly; halfway rounds up.
	let values = [0, 128, 129, 257, 385, 386, 32896, 65406, 65407, 65535];
	let pixels = gray(16, values.len() as u32, row(&values), None);
	assert_eq!(pixels, grays(&[0, 0, 1, 1, 1, 2, 128, 254, 255, 255]));
}

#[test]
fn a_gray_transparency_key_clears_only_its_exact_sample() {
	// Two-bit gray where 2 is transparent: compared before scaling.
	let pixels = gray(2, 4, vec![0b00_01_10_11], Some(vec![0, 2]));
	assert_eq!(pixels, [0xff00_0000, 0xff55_5555, 0x00aa_aaaa, 0xffff_ffff]);
	// Sixteen bits compare all sixteen: 0x0101 is not 0x0100.
	let row = vec![0x01, 0x00, 0x01, 0x01];
	assert_eq!(
		gray(16, 2, row, Some(vec![0x01, 0x00])),
		[0x0001_0101, 0xff01_0101]
	);
}

#[test]
fn palette_entries_take_their_alpha_from_transparency_or_stay_opaque() {
	let spec = Spec {
		width: 4,
		height: 1,
		depth: 2,
		color: 3,
		interlaced: false,
		palette: vec![[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]],
		transparency: Some(vec![0, 0x80]),
		passes: vec![vec![(0, vec![0b00_01_10_11])]],
	};
	let png = write(&spec);
	let pixels = decode(&png, BIG, BIG).unwrap().pixels;
	assert_eq!(pixels, [0x0001_0203, 0x8004_0506, 0xff07_0809, 0xff0a_0b0c]);
	assert_eq!(oracle(&png).unwrap().pixels, pixels);
}

#[test]
fn rgb_keys_need_all_three_samples_to_match() {
	let spec = |row: Vec<u8>| Spec {
		width: 3,
		height: 1,
		depth: 8,
		color: 2,
		interlaced: false,
		palette: Vec::new(),
		transparency: Some(vec![0, 10, 0, 20, 0, 30]),
		passes: vec![vec![(0, row)]],
	};
	let png = write(&spec(vec![10, 20, 30, 10, 20, 31, 30, 20, 10]));
	let pixels = decode(&png, BIG, BIG).unwrap().pixels;
	assert_eq!(pixels, [0x000a_141e, 0xff0a_141f, 0xff1e_140a]);
	assert_eq!(oracle(&png).unwrap().pixels, pixels);
}

#[test]
fn gray_alpha_and_rgba_keep_their_own_alpha() {
	let spec = |color: u8, row: Vec<u8>| Spec {
		width: 2,
		height: 1,
		depth: 8,
		color,
		interlaced: false,
		palette: Vec::new(),
		transparency: None,
		passes: vec![vec![(0, row)]],
	};
	let gray_alpha = write(&spec(4, vec![10, 0, 200, 0x7f]));
	assert_eq!(
		decode(&gray_alpha, BIG, BIG).unwrap().pixels,
		[0x000a_0a0a, 0x7fc8_c8c8]
	);
	let rgba = write(&spec(6, vec![1, 2, 3, 4, 5, 6, 7, 8]));
	assert_eq!(
		decode(&rgba, BIG, BIG).unwrap().pixels,
		[0x0401_0203, 0x0805_0607]
	);
	for png in [gray_alpha, rgba] {
		assert_eq!(oracle(&png), decode(&png, BIG, BIG).ok());
	}
}
