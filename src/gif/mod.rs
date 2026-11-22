//! Our own GIF decoder (GIF89a, and GIF87a), for iTerm2 inline images: the
//! logical screen, color tables, graphic control extensions (delay,
//! transparency, disposal), interlaced and plain images, composed into
//! whole `0xAARRGGBB` frames as a viewer shows them.
#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod gif_bench;
mod lzw;
#[cfg(test)]
mod lzw_tests;
#[cfg(test)]
mod tests;

/// A frame as shown, and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
	pub pixels: Vec<u32>,
	/// Hundredths of a second.
	pub delay: u16,
}

/// A decoded GIF: its screen size and frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Gif {
	pub width: usize,
	pub height: usize,
	pub frames: Vec<Frame>,
}

/// Why a GIF could not be decoded.
pub(crate) type Error = &'static str;

/// Limits on what a file may make us allocate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
	pub max_side: usize,
	pub max_pixels: usize,
	/// Most frames kept; later ones are dropped.
	pub max_frames: usize,
}

/// Pixels an image may have whatever the screen's size.
const MAX_IMAGE: usize = 1 << 20;

/// Disposal methods: how the area of a frame is left before the next is
/// drawn. Others (0, 1 and the reserved 4 to 7) keep it.
const CLEAR: u8 = 2;
const RESTORE: u8 = 3;

/// The graphic control extension's settings for the next image.
#[derive(Clone, Copy)]
struct Control {
	disposal: u8,
	delay: u16,
	transparent: Option<u8>,
}

const NO_CONTROL: Control = Control {
	disposal: 0,
	delay: 0,
	transparent: None,
};

/// Decodes `data` within `limits`. A file ending early keeps the frames
/// it finished; one with none fails.
pub(crate) fn decode(data: &[u8], limits: Limits) -> Result<Gif, Error> {
	let mut input = Reader(data);
	let signature = input.take(6).ok_or("not a GIF")?;
	if signature != b"GIF87a" && signature != b"GIF89a" {
		return Err("not a GIF");
	}
	let screen = input.take(7).ok_or("truncated")?;
	let (width, height) = (word(&screen[0..2]), word(&screen[2..4]));
	if width == 0 || height == 0 {
		return Err("empty screen");
	}
	if width > limits.max_side || height > limits.max_side || width * height > limits.max_pixels {
		return Err("image too large");
	}
	let global = if screen[4] & 0x80 != 0 {
		Some(input.palette(screen[4] & 7).ok_or("truncated")?)
	} else {
		None
	};
	let mut canvas = vec![0u32; width * height];
	let mut frames = Vec::new();
	let mut control = NO_CONTROL;
	while frames.len() < limits.max_frames {
		let Some(introducer) = input.byte() else {
			break;
		};
		match introducer {
			0x3b => break,
			0x21 => {
				let label = input.byte().ok_or("truncated")?;
				let first = input.sub_block().ok_or("truncated")?;
				if label == 0xf9 && first.len() >= 4 {
					control = Control {
						disposal: first[0] >> 2 & 7,
						delay: u16::from_le_bytes([first[1], first[2]]),
						transparent: (first[0] & 1 != 0).then_some(first[3]),
					};
				}
				if !first.is_empty() {
					input.skip_blocks().ok_or("truncated")?;
				}
			}
			0x2c => {
				let Some(frame) = image(&mut input, &global, control, (width, height), &mut canvas) else {
					break;
				};
				frames.push(frame);
				control = NO_CONTROL;
			}
			_ => break,
		}
	}
	if frames.is_empty() {
		return Err("no image");
	}
	Ok(Gif {
		width,
		height,
		frames,
	})
}

/// Decodes one image onto `canvas` and returns the frame it shows, then
/// disposes of it as `control` says; `None` when it is cut short.
fn image(
	input: &mut Reader,
	global: &Option<Vec<u32>>,
	control: Control,
	(width, height): (usize, usize),
	canvas: &mut [u32],
) -> Option<Frame> {
	let descriptor = input.take(9)?;
	let (left, top) = (word(&descriptor[0..2]), word(&descriptor[2..4]));
	let (w, h) = (word(&descriptor[4..6]), word(&descriptor[6..8]));
	// An image far larger than the screen is a way to make decoding run
	// on: one of up to four screens, or a million pixels, is drawn.
	if w * h > (4 * width * height).max(MAX_IMAGE) {
		return None;
	}
	let flags = descriptor[8];
	let local = if flags & 0x80 != 0 {
		Some(input.palette(flags & 7)?)
	} else {
		None
	};
	let colors = local.as_ref().or(global.as_ref())?;
	let min = input.byte()?;
	let mut data = Vec::new();
	loop {
		let block = input.sub_block()?;
		if block.is_empty() {
			break;
		}
		data.extend_from_slice(block);
	}
	let before = (control.disposal == RESTORE).then(|| canvas.to_vec());
	let rows: Vec<usize> = if flags & 0x40 != 0 {
		interlaced(h)
	} else {
		(0..h).collect()
	};
	// Indices go straight onto the screen; only those on it draw.
	let mut i = 0;
	let draw = |index: u8| {
		let (x, y) = (left + i % w, top + rows[i / w]);
		i += 1;
		if x < width && y < height && control.transparent != Some(index) {
			// Indices past the table are black, as viewers show them.
			canvas[y * width + x] = colors
				.get(usize::from(index))
				.copied()
				.unwrap_or(0xff00_0000);
		}
	};
	lzw::decode_into(&data, min, w * h, draw)?;
	let frame = Frame {
		pixels: canvas.to_vec(),
		delay: control.delay,
	};
	match (control.disposal, before) {
		(RESTORE, Some(before)) => canvas.copy_from_slice(&before),
		(CLEAR, _) => {
			for y in top..(top + h).min(height) {
				for x in left..(left + w).min(width) {
					canvas[y * width + x] = 0;
				}
			}
		}
		_ => {}
	}
	Some(frame)
}

/// The order an interlaced image's `h` rows arrive in: every eighth from
/// 0, every eighth from 4, every fourth from 2, every second from 1.
fn interlaced(h: usize) -> Vec<usize> {
	let passes = [(0, 8), (4, 8), (2, 4), (1, 2)];
	passes
		.iter()
		.flat_map(|&(start, step)| (start..h).step_by(step))
		.collect()
}

fn word(bytes: &[u8]) -> usize {
	usize::from(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// The bytes not read yet.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
	fn take(&mut self, n: usize) -> Option<&'a [u8]> {
		let (taken, rest) = self.0.split_at_checked(n)?;
		self.0 = rest;
		Some(taken)
	}

	fn byte(&mut self) -> Option<u8> {
		self.take(1).map(|b| b[0])
	}

	/// A color table of `2^(size + 1)` RGB entries, as opaque pixels.
	fn palette(&mut self, size: u8) -> Option<Vec<u32>> {
		let bytes = self.take(3 << (size + 1))?;
		let entries = bytes.as_chunks::<3>().0.iter();
		Some(
			entries
				.map(|&[r, g, b]| u32::from_be_bytes([0xff, r, g, b]))
				.collect(),
		)
	}

	/// One data sub-block: its length byte, then that many bytes.
	fn sub_block(&mut self) -> Option<&'a [u8]> {
		let length = self.byte()?;
		self.take(usize::from(length))
	}

	/// Sub-blocks up to and with the empty one that ends them.
	fn skip_blocks(&mut self) -> Option<()> {
		while !self.sub_block()?.is_empty() {}
		Some(())
	}
}
