//! Adam7 interlacing: each pass's size against the specification's 8×8
//! pattern, and an image written interlaced decoding to the same pixels as
//! written plainly, for every format and many sizes.
use super::{
	chunks::Header,
	decode, passes,
	tests::{FORMATS, Spec, random, write},
};
use crate::bench::corpus::Rng;

/// The pass (1 to 7) of each pixel of an 8×8 block, as the PNG
/// specification draws it.
const PATTERN: [[u8; 8]; 8] = [
	[1, 6, 4, 6, 2, 6, 4, 6],
	[7, 7, 7, 7, 7, 7, 7, 7],
	[5, 6, 5, 6, 5, 6, 5, 6],
	[7, 7, 7, 7, 7, 7, 7, 7],
	[3, 6, 4, 6, 3, 6, 4, 6],
	[7, 7, 7, 7, 7, 7, 7, 7],
	[5, 6, 5, 6, 5, 6, 5, 6],
	[7, 7, 7, 7, 7, 7, 7, 7],
];

#[test]
fn each_pass_holds_the_pixels_the_pattern_gives_it() {
	for width in 1..=20 {
		for height in 1..=20 {
			let header = Header {
				width,
				height,
				depth: 8,
				color: 0,
				interlaced: true,
			};
			let sizes: Vec<(usize, usize)> = passes(header).collect();
			for (pass, &(w, h)) in sizes.iter().enumerate() {
				let pass = pass as u8 + 1;
				let columns = (0..width).filter(|&x| (0..height).any(|y| PATTERN[y % 8][x % 8] == pass));
				let rows = (0..height).filter(|&y| (0..width).any(|x| PATTERN[y % 8][x % 8] == pass));
				let count = (0..width * height).filter(|i| PATTERN[i / width % 8][i % width % 8] == pass);
				assert_eq!(w * h, count.count(), "pass {pass} of {width}×{height}");
				if w * h > 0 {
					assert_eq!(
						(w, h),
						(columns.count(), rows.count()),
						"pass {pass} of {width}×{height}"
					);
				}
			}
			let plain = Header {
				interlaced: false,
				..header
			};
			assert_eq!(passes(plain).collect::<Vec<_>>(), [(width, height)]);
		}
	}
}

/// `spec`'s pixels rewritten as the other kind of file: interlaced if it
/// was plain, plain if it was interlaced.
fn relaid(spec: &Spec, decoded: &[u32]) -> Spec {
	// The decoded pixels are not raw samples, so write them as 8-bit RGBA,
	// which holds any of them exactly.
	let (width, height) = (spec.width as usize, spec.height as usize);
	let header = Header {
		width,
		height,
		depth: 8,
		color: 6,
		interlaced: !spec.interlaced,
	};
	let rows = passes(header)
		.enumerate()
		.map(|(pass, (w, h))| {
			let at = |x: usize, y: usize| match header.interlaced {
				false => decoded[y * width + x],
				true => {
					let (x0, y0, dx, dy) = super::ADAM7[pass];
					decoded[(y0 + y * dy) * width + x0 + x * dx]
				}
			};
			// A pass without columns has no rows, not empty ones.
			(0..if w == 0 { 0 } else { h })
				.map(|y| {
					let bytes = (0..w).flat_map(|x| {
						let [a, r, g, b] = at(x, y).to_be_bytes();
						[r, g, b, a]
					});
					((y % 5) as u8, bytes.collect())
				})
				.collect()
		})
		.collect();
	Spec {
		depth: 8,
		color: 6,
		interlaced: header.interlaced,
		palette: Vec::new(),
		transparency: None,
		passes: rows,
		..spec.clone()
	}
}

#[test]
fn interlaced_and_plain_files_of_one_image_decode_alike() {
	let mut rng = Rng::new(77);
	for round in 0..300 {
		let format = FORMATS[round % FORMATS.len()];
		let size = (1 + rng.below(30) as u32, 1 + rng.below(30) as u32);
		let interlaced = round % 2 == 0;
		let spec = random(&mut rng, format, size, interlaced);
		let first = decode(&write(&spec), 1 << 20, 1 << 20).unwrap();
		let other = decode(&write(&relaid(&spec, &first.pixels)), 1 << 20, 1 << 20).unwrap();
		assert_eq!(first, other, "round {round}: {format:?} {size:?}");
	}
}

#[test]
fn images_smaller_than_a_block_leave_passes_empty_and_still_decode() {
	let mut rng = Rng::new(9);
	for size in [(1, 1), (1, 9), (9, 1), (2, 2), (3, 5)] {
		let spec = random(&mut rng, (6, 8), size, true);
		let empty = spec.passes.iter().filter(|pass| pass.is_empty()).count();
		assert!(size != (1, 1) || empty == 6, "a single pixel is all pass 1");
		let png = write(&spec);
		assert_eq!(
			decode(&png, 64, 64).ok(),
			super::tests::oracle(&png),
			"{size:?}"
		);
	}
}
