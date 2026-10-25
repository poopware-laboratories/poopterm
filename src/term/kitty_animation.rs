//! Applies kitty animation commands: frame data (`a=f`), composition
//! (`a=c`), playback control (`a=a`) and frame deletion (`d=f`), and moves
//! running animations along the clock.
use std::{sync::Arc, time::Instant};

use super::Term;
use crate::graphics::{
	Bitmap, Rect,
	kitty::{self, Command, Error, Playback, compose},
};

fn no_frame() -> Error {
	Error::new("ENOENT", "frame not found")
}

/// A frame number key (`r`, `c`): 1-based, 0 when absent.
fn frame_number(command: &Command, key: u8) -> usize {
	command.uint(key) as usize
}

fn gap(command: &Command) -> i32 {
	command.int(b'z').clamp(i32::MIN.into(), i32::MAX.into()) as i32
}

impl Term {
	/// The image a command names by number (`I`) or id (`i`).
	pub(super) fn kitty_lookup(&self, command: &Command) -> Option<u32> {
		let number = command.uint(b'I');
		let image = if number != 0 {
			self.kitty_images.by_number(number)
		} else {
			self.kitty_images.get(command.uint(b'i'))
		};
		image.map(|image| image.id)
	}

	/// Like [`Term::kitty_lookup`], but an error when missing; a number is
	/// answered with the image's id.
	pub(super) fn kitty_target(&self, command: &mut Command) -> Result<u32, Error> {
		let id = self
			.kitty_lookup(command)
			.ok_or_else(|| Error::new("ENOENT", "image not found"))?;
		if command.uint(b'I') != 0 {
			command.set(b'i', i64::from(id));
		}
		Ok(id)
	}

	/// `a=f`: composes frame data onto a new frame or an existing one (`r`).
	pub(super) fn kitty_frame(
		&mut self,
		command: &mut Command,
		data: Result<Vec<u8>, Error>,
	) -> Result<(), Error> {
		let data = kitty::load(command, data?)?;
		let id = self.kitty_target(command)?;
		let animation = &self.kitty_images.get(id).unwrap().animation;
		let root = animation.frame(0).unwrap();
		let (width, height) = (root.width, root.height);
		// Raw data covers the whole image unless the client says otherwise.
		for (key, size) in [(b's', width), (b'v', height)] {
			if !command.has(key) {
				command.set(key, size as i64);
			}
		}
		let data = kitty::decode(command, &data)?;
		let at = (command.uint(b'x') as usize, command.uint(b'y') as usize);
		if at.0 + data.width > width || at.1 + data.height > height {
			return Err(Error::new("EINVAL", "frame data outside the image"));
		}
		let edit = frame_number(command, b'r');
		let base = if edit != 0 {
			edit
		} else {
			frame_number(command, b'c')
		};
		let mut canvas = if base != 0 {
			Bitmap::clone(animation.frame(base - 1).ok_or_else(no_frame)?)
		} else {
			// The background color is RGBA.
			let color = command.uint(b'Y').rotate_right(8);
			Bitmap::new(width, height, vec![color; width * height])
		};
		let whole = Rect {
			x: 0,
			y: 0,
			width: data.width,
			height: data.height,
		};
		compose(&mut canvas, at, &data, whole, command.uint(b'X') == 1);
		let (canvas, gap, now) = (Arc::new(canvas), gap(command), Instant::now());
		self
			.kitty_images
			.animation_mut(id, |animation| {
				if edit == 0 {
					return animation.push(canvas, gap, now);
				}
				animation.replace(edit - 1, canvas);
				animation.set_gap(edit - 1, gap);
				Ok(())
			})
			.unwrap()?;
		let shown = self.kitty_images_in_use();
		let evicted = self.kitty_images.evict(id, |id| shown.contains(&id));
		self.remove_kitty_placements(|placement| evicted.contains(&placement.image));
		if !self.kitty_images.fits() && edit == 0 {
			self
				.kitty_images
				.animation_mut(id, |animation| animation.pop());
			return Err(Error::new("ENOSPC", "no room for the frame"));
		}
		self.show_kitty_frame(id);
		Ok(())
	}

	/// `a=c`: composes a rectangle of frame `r` onto frame `c`.
	pub(super) fn kitty_compose(&mut self, command: &mut Command) -> Result<(), Error> {
		let id = self.kitty_target(command)?;
		let animation = &self.kitty_images.get(id).unwrap().animation;
		let (from, to) = (frame_number(command, b'r'), frame_number(command, b'c'));
		let frame = |number: usize| animation.frame(number.wrapping_sub(1)).ok_or_else(no_frame);
		let (source, target) = (frame(from)?, frame(to)?);
		let (width, height) = (source.width, source.height);
		let size = |key: u8, whole: usize| match command.uint(key) as usize {
			0 => whole,
			size => size,
		};
		let rect = Rect {
			x: command.uint(b'X') as usize,
			y: command.uint(b'Y') as usize,
			width: size(b'w', width),
			height: size(b'h', height),
		};
		let at = (command.uint(b'x') as usize, command.uint(b'y') as usize);
		let inside = |x: usize, y: usize| x + rect.width <= width && y + rect.height <= height;
		if !inside(rect.x, rect.y) || !inside(at.0, at.1) {
			return Err(Error::new("EINVAL", "rectangle outside the image"));
		}
		let overlap = rect.x < at.0 + rect.width
			&& at.0 < rect.x + rect.width
			&& rect.y < at.1 + rect.height
			&& at.1 < rect.y + rect.height;
		if from == to && overlap {
			return Err(Error::new("EINVAL", "overlapping rectangles in one frame"));
		}
		let mut canvas = Bitmap::clone(target);
		compose(&mut canvas, at, source, rect, command.uint(b'C') == 1);
		let canvas = Arc::new(canvas);
		self
			.kitty_images
			.animation_mut(id, |animation| animation.replace(to - 1, canvas));
		self.show_kitty_frame(id);
		Ok(())
	}

	/// `a=a`: loop count (`v`), a frame's gap (`r`, `z`), the shown frame
	/// (`c`) and playback (`s`).
	pub(super) fn kitty_animate(&mut self, command: &mut Command) -> Result<(), Error> {
		let id = self.kitty_target(command)?;
		let (gap_frame, current) = (frame_number(command, b'r'), frame_number(command, b'c'));
		let (gap, loops, now) = (gap(command), command.uint(b'v'), Instant::now());
		let playback = match command.uint(b's') {
			1 => Some(Playback::Stopped),
			2 => Some(Playback::Loading),
			3 => Some(Playback::Looping),
			_ => None,
		};
		let result = self
			.kitty_images
			.animation_mut(id, |animation| {
				if gap_frame > animation.len() || current > animation.len() {
					return Err(no_frame());
				}
				animation.set_loops(loops);
				if gap_frame != 0 {
					animation.set_gap(gap_frame - 1, gap);
				}
				if current != 0 {
					animation.set_current(current - 1, now);
				}
				if let Some(playback) = playback {
					animation.set_playback(playback, now);
				}
				Ok(())
			})
			.unwrap();
		self.show_kitty_frame(id);
		result
	}

	/// `a=d,d=f`: drops an image's frames but the first.
	pub(super) fn kitty_delete_frames(&mut self, command: &Command) {
		if let Some(id) = self.kitty_lookup(command) {
			self
				.kitty_images
				.animation_mut(id, |animation| animation.truncate());
			self.show_kitty_frame(id);
		}
	}

	/// Points the image's placements at the frame it shows now.
	fn show_kitty_frame(&mut self, id: u32) {
		let Some(image) = self.kitty_images.get(id) else {
			return;
		};
		let bitmap = image.bitmap().clone();
		for placement in self.images.iter_mut().chain(&mut self.primary_images) {
			if placement.kitty.is_some_and(|key| key.image == id)
				&& !Arc::ptr_eq(&placement.bitmap, &bitmap)
			{
				placement.bitmap = bitmap.clone();
			}
		}
	}

	fn kitty_image_shown(&self, id: u32) -> bool {
		self.kitty_placements().any(|key| key.image == id)
			|| self.kitty_images.virtual_ids().any(|image| image == id)
	}

	/// Moves running animations to `now`. Returns whether a frame that may
	/// be on screen changed.
	pub(crate) fn advance_animations(&mut self, now: Instant) -> bool {
		if self.reduce_motion {
			return false;
		}
		let changed = self.kitty_images.advance(now);
		for &id in &changed {
			self.show_kitty_frame(id);
		}
		changed.into_iter().any(|id| self.kitty_image_shown(id))
	}

	/// When a shown animation changes frame next.
	pub(crate) fn animation_deadline(&self) -> Option<Instant> {
		(!self.reduce_motion).then(|| self.kitty_images.deadline(|id| self.kitty_image_shown(id)))?
	}
}
