//! GIF files written here from the specification, and well-known ones,
//! decoded into frames: color tables, transparency, the three disposals,
//! offsets, interlacing, limits, and damaged or unusual files.
use super::{Gif, Limits, decode, lzw_tests::encode};
use crate::bench::corpus::Rng;

const ROOMY: Limits = Limits {
	max_side: 1 << 14,
	max_pixels: 1 << 26,
	max_frames: 100,
};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const WHITE: u32 = 0xffff_ffff;
const CLEAR: u32 = 0;

/// An image to place on the screen.
#[derive(Clone)]
pub(super) struct Image {
	pub left: u16,
	pub top: u16,
	pub width: u16,
	pub height: u16,
	/// Row by row, top to bottom; written interlaced if asked.
	pub indices: Vec<u8>,
	pub local: Option<Vec<u32>>,
	pub interlaced: bool,
	/// Graphic control: disposal (0–3), delay, transparent index.
	pub control: Option<(u8, u16, Option<u8>)>,
}

pub(super) fn image(width: u16, height: u16, indices: Vec<u8>) -> Image {
	Image {
		left: 0,
		top: 0,
		width,
		height,
		indices,
		local: None,
		interlaced: false,
		control: None,
	}
}

/// A color table of the entries given, padded to a power of two, and the
/// size field that says so.
fn table(colors: &[u32]) -> (Vec<u8>, u8) {
	let size = (1..=8).find(|&bits| colors.len() <= 1 << bits).unwrap();
	let mut bytes: Vec<u8> = colors
		.iter()
		.flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8])
		.collect();
	bytes.resize(3 << size, 0);
	(bytes, size as u8 - 1)
}

/// The rows of an `h`-row image in the order interlacing sends them.
fn interlace_order(h: usize) -> Vec<usize> {
	let mut order = Vec::new();
	for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
		order.extend((start..h).step_by(step));
	}
	order
}

/// A GIF89a of `width` by `height` with an optional global table.
pub(super) fn write(width: u16, height: u16, global: Option<&[u32]>, images: &[Image]) -> Vec<u8> {
	let mut out = b"GIF89a".to_vec();
	out.extend(width.to_le_bytes());
	out.extend(height.to_le_bytes());
	let global = global.map(table);
	out.push(global.as_ref().map_or(0, |(_, size)| 0x80 | size));
	out.extend([0, 0]);
	if let Some((bytes, _)) = &global {
		out.extend(bytes);
	}
	for img in images {
		if let Some((disposal, delay, transparent)) = img.control {
			out.extend([
				0x21,
				0xf9,
				4,
				disposal << 2 | u8::from(transparent.is_some()),
			]);
			out.extend(delay.to_le_bytes());
			out.extend([transparent.unwrap_or(0), 0]);
		}
		out.push(0x2c);
		for value in [img.left, img.top, img.width, img.height] {
			out.extend(value.to_le_bytes());
		}
		let local = img.local.as_deref().map(table);
		let flags =
			local.as_ref().map_or(0, |(_, size)| 0x80 | size) | if img.interlaced { 0x40 } else { 0 };
		out.push(flags);
		if let Some((bytes, _)) = &local {
			out.extend(bytes);
		}
		let indices: Vec<u8> = match img.interlaced {
			false => img.indices.clone(),
			true => {
				let w = usize::from(img.width);
				let rows = interlace_order(usize::from(img.height));
				rows
					.iter()
					.flat_map(|&y| img.indices[y * w..(y + 1) * w].to_vec())
					.collect()
			}
		};
		let min = 2.max(8 - indices.iter().max().copied().unwrap_or(0).leading_zeros() as u8);
		out.push(min);
		for block in encode(&indices, min, true).chunks(255) {
			out.push(block.len() as u8);
			out.extend(block);
		}
		out.push(0);
	}
	out.push(0x3b);
	out
}

fn frames(gif: &Gif) -> Vec<Vec<u32>> {
	gif
		.frames
		.iter()
		.map(|frame| frame.pixels.clone())
		.collect()
}

#[test]
fn the_smallest_transparent_and_white_gifs_decode() {
	// Two files found on countless web pages.
	let transparent = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
	let white = "R0lGODlhAQABAIAAAP///wAAACwAAAAAAQABAAACAkQBADs=";
	for (file, pixel) in [(transparent, CLEAR), (white, WHITE)] {
		let bytes = crate::graphics::base64::decode(file.as_bytes()).unwrap();
		let gif = decode(&bytes, ROOMY).unwrap();
		assert_eq!(
			(gif.width, gif.height, frames(&gif)),
			(1, 1, vec![vec![pixel]]),
			"{file}"
		);
	}
}

#[test]
fn indices_take_their_colors_from_the_global_or_a_local_table() {
	let indices = vec![0, 1, 2, 1];
	let global = write(
		2,
		2,
		Some(&[RED, GREEN, BLUE]),
		&[image(2, 2, indices.clone())],
	);
	assert_eq!(
		frames(&decode(&global, ROOMY).unwrap()),
		[vec![RED, GREEN, BLUE, GREEN]]
	);
	let mut local = image(2, 2, indices);
	local.local = Some(vec![BLUE, WHITE, RED]);
	let file = write(2, 2, Some(&[RED, GREEN, BLUE]), &[local]);
	assert_eq!(
		frames(&decode(&file, ROOMY).unwrap()),
		[vec![BLUE, WHITE, RED, WHITE]],
		"local wins"
	);
	// An image without any table cannot be drawn.
	assert_eq!(
		decode(&write(1, 1, None, &[image(1, 1, vec![0])]), ROOMY),
		Err("no image")
	);
}

#[test]
fn indices_past_the_table_are_black() {
	// A table of two padded to four entries holds black at 2 and 3; an
	// index past even the padding draws black too.
	let file = write(3, 1, Some(&[RED, GREEN]), &[image(3, 1, vec![0, 3, 1])]);
	assert_eq!(
		frames(&decode(&file, ROOMY).unwrap()),
		[vec![RED, 0xff00_0000, GREEN]]
	);
}

#[test]
fn the_transparent_index_leaves_what_was_there() {
	let mut first = image(2, 1, vec![0, 1]);
	first.control = Some((1, 10, None));
	let mut second = image(2, 1, vec![2, 2]);
	second.control = Some((1, 20, Some(2)));
	let mut third = image(2, 1, vec![2, 0]);
	third.control = Some((1, 30, Some(2)));
	let file = write(2, 1, Some(&[RED, GREEN, BLUE]), &[first, second, third]);
	let gif = decode(&file, ROOMY).unwrap();
	assert_eq!(
		frames(&gif),
		[vec![RED, GREEN], vec![RED, GREEN], vec![RED, RED]]
	);
	let delays: Vec<u16> = gif.frames.iter().map(|f| f.delay).collect();
	assert_eq!(delays, [10, 20, 30]);
	// Transparency on the first frame shows nothing: the screen starts clear.
	let mut only = image(2, 1, vec![0, 1]);
	only.control = Some((0, 0, Some(0)));
	let file = write(2, 1, Some(&[RED, GREEN]), &[only]);
	assert_eq!(frames(&decode(&file, ROOMY).unwrap()), [vec![CLEAR, GREEN]]);
}

/// A 3×1 screen: a red background frame, then a blue pixel at x = 1 with
/// `disposal`, then a frame that draws nothing (all transparent).
fn after_disposal(disposal: u8) -> Vec<Vec<u32>> {
	let mut background = image(3, 1, vec![0, 0, 0]);
	background.control = Some((1, 0, None));
	let mut dot = image(1, 1, vec![1]);
	dot.left = 1;
	dot.control = Some((disposal, 0, None));
	let mut nothing = image(3, 1, vec![2, 2, 2]);
	nothing.control = Some((1, 0, Some(2)));
	frames(
		&decode(
			&write(3, 1, Some(&[RED, BLUE, GREEN]), &[background, dot, nothing]),
			ROOMY,
		)
		.unwrap(),
	)
}

#[test]
fn each_disposal_leaves_the_screen_as_the_specification_says() {
	let shown = [vec![RED; 3], vec![RED, BLUE, RED]];
	// 0 (unspecified) and 1 (do not dispose) keep the dot.
	for disposal in [0, 1] {
		assert_eq!(
			after_disposal(disposal),
			[&shown[..], &[vec![RED, BLUE, RED]]].concat(),
			"{disposal}"
		);
	}
	// 2 clears its area to the background, transparent as viewers show it.
	assert_eq!(
		after_disposal(2),
		[&shown[..], &[vec![RED, CLEAR, RED]]].concat()
	);
	// 3 restores what was there before it.
	assert_eq!(
		after_disposal(3),
		[&shown[..], &[vec![RED, RED, RED]]].concat()
	);
	// Values 4 to 7 are reserved and treated as keeping.
	assert_eq!(
		after_disposal(5),
		[&shown[..], &[vec![RED, BLUE, RED]]].concat()
	);
}

#[test]
fn interlaced_images_decode_to_the_same_pixels_as_plain_ones() {
	let mut rng = Rng::new(21);
	for height in 1..=20u16 {
		let width = 1 + rng.below(9) as u16;
		let indices: Vec<u8> = (0..usize::from(width * height))
			.map(|_| rng.below(4) as u8)
			.collect();
		let plain = write(
			width,
			height,
			Some(&[RED, GREEN, BLUE, WHITE]),
			&[image(width, height, indices.clone())],
		);
		let mut interlaced = image(width, height, indices);
		interlaced.interlaced = true;
		let interlaced = write(
			width,
			height,
			Some(&[RED, GREEN, BLUE, WHITE]),
			&[interlaced],
		);
		assert_ne!(plain, interlaced, "{width}×{height}");
		assert_eq!(
			decode(&interlaced, ROOMY),
			decode(&plain, ROOMY),
			"{width}×{height}"
		);
	}
}

#[test]
fn images_off_or_past_the_screen_draw_only_what_lies_on_it() {
	let mut wide = image(4, 2, vec![1; 8]);
	(wide.left, wide.top) = (1, 1);
	let file = write(3, 2, Some(&[RED, GREEN]), &[wide]);
	let gif = decode(&file, ROOMY).unwrap();
	assert_eq!(
		frames(&gif),
		[vec![CLEAR, CLEAR, CLEAR, CLEAR, GREEN, GREEN]]
	);
	let mut away = image(2, 2, vec![1; 4]);
	(away.left, away.top) = (500, 500);
	let file = write(2, 2, Some(&[RED, GREEN]), &[away]);
	assert_eq!(frames(&decode(&file, ROOMY).unwrap()), [vec![CLEAR; 4]]);
}

#[test]
fn screens_past_the_limits_are_refused_and_frames_past_them_dropped() {
	let file = write(
		40,
		30,
		Some(&[RED, GREEN]),
		&[image(1, 1, vec![1]), image(1, 1, vec![0])],
	);
	let limits = |max_side, max_pixels, max_frames| Limits {
		max_side,
		max_pixels,
		max_frames,
	};
	assert_eq!(
		decode(&file, limits(40, 1200, 2)).unwrap().frames.len(),
		2,
		"at the limits"
	);
	assert_eq!(decode(&file, limits(39, 1200, 2)), Err("image too large"));
	assert_eq!(decode(&file, limits(40, 1199, 2)), Err("image too large"));
	assert_eq!(decode(&file, limits(40, 1200, 1)).unwrap().frames.len(), 1);
	// An image claiming 65535 × 65535 pixels on a 2 × 2 screen is refused
	// before any of its data is decoded; one of a million pixels is not.
	let huge = image(65535, 65535, vec![1; 4]);
	let file = write(2, 2, Some(&[RED, GREEN]), &[huge]);
	assert_eq!(decode(&file, ROOMY), Err("no image"));
	let big = image(1024, 1024, vec![1; 4]);
	let file = write(2, 2, Some(&[RED, GREEN]), &[big]);
	assert_eq!(
		frames(&decode(&file, ROOMY).unwrap()),
		[vec![GREEN, GREEN, CLEAR, CLEAR]]
	);
	// 17 × 61681 is one pixel more than a million.
	let over = image(17, 61681, vec![1; 4]);
	assert_eq!(
		decode(&write(2, 2, Some(&[RED, GREEN]), &[over]), ROOMY),
		Err("no image")
	);
}

#[test]
fn a_file_cut_short_keeps_its_finished_frames() {
	let file = write(
		2,
		1,
		Some(&[RED, GREEN]),
		&[image(2, 1, vec![0, 1]), image(2, 1, vec![1, 0])],
	);
	let whole = decode(&file, ROOMY).unwrap();
	assert_eq!(whole.frames.len(), 2);
	for cut in 0..file.len() {
		match decode(&file[..cut], ROOMY) {
			Ok(gif) => assert!(whole.frames.starts_with(&gif.frames[..1]), "cut at {cut}"),
			Err(why) => assert!(
				["not a GIF", "truncated", "no image"].contains(&why),
				"cut at {cut}: {why}"
			),
		}
	}
	// Without its trailer, a file still shows all it holds.
	assert_eq!(decode(&file[..file.len() - 1], ROOMY), Ok(whole));
}

#[test]
fn other_extensions_and_signatures() {
	let mut file = write(1, 1, Some(&[RED, GREEN]), &[image(1, 1, vec![1])]);
	// A comment and a NETSCAPE looping block before the image.
	let extensions = [
		&[0x21, 0xfe, 5][..],
		b"hello",
		&[0, 0x21, 0xff, 11],
		b"NETSCAPE2.0",
		&[3, 1, 0, 0, 0],
	]
	.concat();
	let at = 13 + 3 * 2;
	file.splice(at..at, extensions);
	assert_eq!(frames(&decode(&file, ROOMY).unwrap()), [vec![GREEN]]);
	let mut old = file.clone();
	old[3..6].copy_from_slice(b"87a");
	assert!(decode(&old, ROOMY).is_ok(), "GIF87a");
	for bad in [&b"GIF90a"[..], b"PNG", b""] {
		assert_eq!(decode(bad, ROOMY), Err("not a GIF"));
	}
	let mut empty = write(1, 1, Some(&[RED]), &[]);
	assert_eq!(decode(&empty, ROOMY), Err("no image"));
	empty[6..8].copy_from_slice(&[0, 0]);
	assert_eq!(decode(&empty, ROOMY), Err("empty screen"));
}

#[test]
fn damaged_files_never_panic() {
	let mut rng = Rng::new(22);
	let mut dot = image(5, 4, (0..20).map(|i| (i % 3) as u8).collect());
	dot.control = Some((2, 5, Some(1)));
	let file = write(6, 5, Some(&[RED, GREEN, BLUE]), &[dot.clone(), dot]);
	for _ in 0..3000 {
		let mut damaged = file.clone();
		for _ in 0..1 + rng.below(4) {
			let at = rng.below(damaged.len());
			damaged[at] = rng.below(256) as u8;
		}
		if let Ok(gif) = decode(&damaged, ROOMY) {
			assert!(
				gif
					.frames
					.iter()
					.all(|f| f.pixels.len() == gif.width * gif.height)
			);
		}
	}
}

#[test]
fn every_color_table_size_reaches_its_last_entry() {
	// Size fields 0 to 7 hold 2 to 256 entries; index n - 1 is the last.
	for bits in 1..=8u32 {
		let entries = 1usize << bits;
		let colors: Vec<u32> = (0..entries as u32)
			.map(|i| 0xff00_0000 | (i * 0x01_0101))
			.collect();
		let last = (entries - 1) as u8;
		let file = write(2, 1, Some(&colors), &[image(2, 1, vec![0, last])]);
		assert_eq!(
			frames(&decode(&file, ROOMY).unwrap()),
			[vec![colors[0], colors[entries - 1]]],
			"{entries}"
		);
		// A table cut short is a truncated file.
		let at = 13 + 3 * entries - 1;
		assert_eq!(decode(&file[..at], ROOMY), Err("truncated"), "{entries}");
	}
}

#[test]
fn extensions_of_many_sub_blocks_are_skipped_whole() {
	let file = write(1, 1, Some(&[RED, GREEN]), &[image(1, 1, vec![1])]);
	let at = 13 + 3 * 2;
	// An application extension of ten sub-blocks of every length up to 255.
	let mut extension = vec![0x21, 0xff];
	for length in [255, 1, 0x2c, 0x3b, 7, 255, 2, 100, 3, 9] {
		extension.push(length);
		extension.extend(std::iter::repeat_n(0x2c, usize::from(length)));
	}
	extension.push(0);
	let mut long = file.clone();
	long.splice(at..at, extension.iter().copied());
	assert_eq!(
		decode(&long, ROOMY),
		decode(&file, ROOMY),
		"block bytes that look like introducers"
	);
	// Cut inside it, the file ends before any image.
	assert_eq!(decode(&long[..at + 300], ROOMY), Err("truncated"));
}

#[test]
fn a_transparent_index_past_the_table_wins_over_black() {
	// Two colors padded to four; index 3 names no color, and is transparent.
	let mut background = image(3, 1, vec![0, 0, 0]);
	background.control = Some((1, 0, None));
	let mut over = image(3, 1, vec![3, 1, 2]);
	over.control = Some((1, 0, Some(3)));
	let file = write(3, 1, Some(&[RED, GREEN]), &[background, over]);
	let shown = frames(&decode(&file, ROOMY).unwrap());
	assert_eq!(shown.len(), 2);
	assert_eq!(shown[0], [RED; 3]);
	// Index 3 keeps the red below; index 2, padding, draws black.
	assert_eq!(shown[1], [RED, GREEN, 0xff00_0000]);
}
