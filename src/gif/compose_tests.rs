//! Random animations against a reference: a second, plain reading of the
//! specification's composition rules written here, a pixel at a time, so
//! the decoder's frames are checked by something it does not share code
//! with.
use super::{
	Limits, decode,
	tests::{Image, image, write},
};
use crate::bench::corpus::Rng;

const ROOMY: Limits = Limits {
	max_side: 1 << 14,
	max_pixels: 1 << 26,
	max_frames: 100,
};

/// What a viewer shows after each of `images` on a `width` × `height`
/// screen with `global` colors: the screen starts transparent; each image
/// draws its opaque indices where they land on the screen; then it is
/// kept (0, 1, 4–7), cleared to transparent (2) or undone (3).
fn reference(width: usize, height: usize, global: &[u32], images: &[Image]) -> Vec<Vec<u32>> {
	let mut screen = vec![0u32; width * height];
	let mut shown = Vec::new();
	for img in images {
		let colors = img.local.as_deref().unwrap_or(global);
		let (disposal, transparent) = img.control.map_or((0, None), |(d, _, t)| (d, t));
		let before = screen.clone();
		for y in 0..usize::from(img.height) {
			for x in 0..usize::from(img.width) {
				let index = img.indices[y * usize::from(img.width) + x];
				let (sx, sy) = (usize::from(img.left) + x, usize::from(img.top) + y);
				if sx < width && sy < height && Some(index) != transparent {
					let padded = colors.len().next_power_of_two().max(2);
					let color = match usize::from(index) {
						i if i < colors.len() => colors[i],
						// Padding entries of the table, and past it, are black.
						i if i < padded => 0xff00_0000,
						_ => 0xff00_0000,
					};
					screen[sy * width + sx] = color;
				}
			}
		}
		shown.push(screen.clone());
		match disposal {
			2 => {
				for y in 0..usize::from(img.height) {
					for x in 0..usize::from(img.width) {
						let (sx, sy) = (usize::from(img.left) + x, usize::from(img.top) + y);
						if sx < width && sy < height {
							screen[sy * width + sx] = 0;
						}
					}
				}
			}
			3 => screen = before,
			_ => {}
		}
	}
	shown
}

/// A random table of 1 to 6 opaque colors.
fn colors(rng: &mut Rng) -> Vec<u32> {
	(0..1 + rng.below(6))
		.map(|_| 0xff00_0000 | rng.next() as u32 & 0xff_ffff)
		.collect()
}

#[test]
fn random_animations_show_what_the_reference_shows() {
	let mut rng = Rng::new(2027);
	for round in 0..400 {
		let (width, height) = (1 + rng.below(12), 1 + rng.below(12));
		let global = colors(&mut rng);
		let images: Vec<Image> = (0..1 + rng.below(5))
			.map(|_| {
				let (w, h) = (1 + rng.below(width + 2), 1 + rng.below(height + 2));
				let local = (rng.below(3) == 0).then(|| colors(&mut rng));
				let palette = local.as_ref().unwrap_or(&global).len();
				// Now and then an index past the table.
				let indices = (0..w * h).map(|_| rng.below(palette + 1) as u8).collect();
				let mut img = image(w as u16, h as u16, indices);
				(img.left, img.top) = (rng.below(width) as u16, rng.below(height) as u16);
				img.local = local;
				img.interlaced = rng.below(2) == 0;
				let transparent = (rng.below(2) == 0).then(|| rng.below(palette) as u8);
				img.control =
					(rng.below(4) != 0).then(|| (rng.below(8) as u8, rng.below(500) as u16, transparent));
				img
			})
			.collect();
		let file = write(width as u16, height as u16, Some(&global), &images);
		let gif = decode(&file, ROOMY).unwrap_or_else(|why| panic!("round {round}: {why}"));
		let frames: Vec<Vec<u32>> = gif.frames.iter().map(|f| f.pixels.clone()).collect();
		assert_eq!(
			frames,
			reference(width, height, &global, &images),
			"round {round}"
		);
		let delays: Vec<u16> = gif.frames.iter().map(|f| f.delay).collect();
		let expected: Vec<u16> = images
			.iter()
			.map(|i| i.control.map_or(0, |(_, d, _)| d))
			.collect();
		assert_eq!(delays, expected, "round {round}");
	}
}

#[test]
fn random_bytes_as_lzw_never_panic_or_pass_the_limit() {
	let mut rng = Rng::new(9);
	for _ in 0..3000 {
		let bytes: Vec<u8> = (0..rng.below(300)).map(|_| rng.below(256) as u8).collect();
		let (min, limit) = (2 + rng.below(7) as u8, rng.below(2000));
		if let Some(out) = super::lzw_tests::decode(&bytes, min, limit) {
			assert!(out.len() <= limit);
			assert!(
				out.iter().all(|&index| usize::from(index) < 1 << min),
				"only roots' bytes"
			);
		}
	}
}

#[test]
fn a_short_graphic_control_block_is_ignored_and_empty_extensions_skipped() {
	let mut file = write(
		1,
		1,
		Some(&[0xffff_0000, 0xff00_ff00]),
		&[image(1, 1, vec![1])],
	);
	let at = 13 + 3 * 2;
	// A graphic control of 3 bytes (too short to read), then an extension
	// whose first sub-block is already the terminator.
	file.splice(
		at..at,
		[0x21, 0xf9, 3, 0x01, 9, 9, 0, 0x21, 0x01, 0]
			.iter()
			.copied(),
	);
	let gif = decode(&file, ROOMY).unwrap();
	assert_eq!(
		gif.frames[0].pixels,
		[0xff00_ff00],
		"index 1 is not made transparent"
	);
	assert_eq!(gif.frames[0].delay, 0);
}

#[test]
fn control_applies_to_the_next_image_only() {
	let mut first = image(1, 1, vec![0]);
	first.control = Some((0, 7, Some(0)));
	let second = image(1, 1, vec![0]);
	let file = write(1, 1, Some(&[0xffff_0000, 0xff00_ff00]), &[first, second]);
	let gif = decode(&file, ROOMY).unwrap();
	let frames: Vec<(u32, u16)> = gif.frames.iter().map(|f| (f.pixels[0], f.delay)).collect();
	assert_eq!(
		frames,
		[(0, 7), (0xffff_0000, 0)],
		"the second has no transparency or delay"
	);
}

#[test]
fn unknown_blocks_end_the_file_with_the_frames_so_far() {
	let mut file = write(1, 1, Some(&[0xffff_0000]), &[image(1, 1, vec![0])]);
	let end = file.len() - 1;
	file[end] = 0x99;
	file.extend(b"garbage");
	assert_eq!(decode(&file, ROOMY).unwrap().frames.len(), 1);
}

#[test]
fn interlaced_rows_arrive_in_the_specifications_four_passes() {
	// Pass 1 every eighth row from 0, pass 2 every eighth from 4, pass 3
	// every fourth from 2, pass 4 every second from 1 (GIF89a, appendix E).
	assert_eq!(super::interlaced(1), [0]);
	assert_eq!(super::interlaced(5), [0, 4, 2, 1, 3]);
	let sixteen = [0, 8, 4, 12, 2, 6, 10, 14, 1, 3, 5, 7, 9, 11, 13, 15];
	assert_eq!(super::interlaced(16), sixteen);
	for h in 0..=40 {
		let mut rows = super::interlaced(h);
		rows.sort_unstable();
		assert_eq!(rows, (0..h).collect::<Vec<_>>(), "each of {h} rows once");
	}
}

#[test]
fn a_frame_cap_keeps_the_first_frames_of_any_animation() {
	let mut rng = Rng::new(404);
	for _ in 0..100 {
		let global = colors(&mut rng);
		let images: Vec<Image> = (0..1 + rng.below(6))
			.map(|_| {
				let mut img = image(
					2,
					2,
					(0..4).map(|_| rng.below(global.len()) as u8).collect(),
				);
				img.control = Some((rng.below(4) as u8, 1, None));
				img
			})
			.collect();
		let file = write(2, 2, Some(&global), &images);
		let all = reference(2, 2, &global, &images);
		for cap in 1..=7 {
			let limits = Limits {
				max_frames: cap,
				..ROOMY
			};
			let frames: Vec<Vec<u32>> = decode(&file, limits)
				.unwrap()
				.frames
				.into_iter()
				.map(|f| f.pixels)
				.collect();
			assert_eq!(frames, all[..cap.min(all.len())], "cap {cap}");
		}
	}
}

#[test]
fn the_background_color_and_aspect_ratio_change_nothing() {
	let file = write(
		2,
		1,
		Some(&[0xffff_0000, 0xff00_ff00]),
		&[image(1, 1, vec![1])],
	);
	let expected = decode(&file, ROOMY).unwrap();
	let mut other = file.clone();
	// Background index 1 and an aspect ratio: viewers leave unpainted
	// pixels transparent, not the background color.
	(other[11], other[12]) = (1, 49);
	assert_eq!(decode(&other, ROOMY), Ok(expected.clone()));
	assert_eq!(expected.frames[0].pixels, [0xff00_ff00, 0]);
}
