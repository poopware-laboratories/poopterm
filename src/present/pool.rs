//! The window buffers frames are copied into: a few, so the window draws
//! the next frame while the compositor still shows the last, each reused
//! once the compositor releases it, with its age saying which frame it
//! still holds. The window system makes and destroys the buffers this
//! names.

/// How many buffers of the window's size there may be at once.
pub(crate) const MAX_BUFFERS: usize = 3;

/// Which buffer to copy the next frame into.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pick {
	/// A buffer made before, holding the frame shown `age` frames ago:
	/// 1 for the last one, 0 when unknown.
	Reuse { id: u64, age: u8 },
	/// Make a buffer of the pool's size, with this id.
	Make { id: u64 },
	/// The compositor holds every buffer, or the window has no area.
	Wait,
}

struct Buffer {
	id: u64,
	/// Shown and not yet released.
	held: bool,
	/// The frame it last showed, counting from 1; 0 for none.
	shown: u64,
	/// Of an old size: destroyed once released, never reused.
	stale: bool,
}

pub(crate) struct Pool {
	buffers: Vec<Buffer>,
	size: [u32; 2],
	/// Frames shown so far.
	frames: u64,
	made: u64,
}

impl Pool {
	pub(crate) fn new(size: [u32; 2]) -> Self {
		Self {
			buffers: Vec::new(),
			size,
			frames: 0,
			made: 0,
		}
	}

	/// The size buffers are made at.
	pub(crate) fn size(&self) -> [u32; 2] {
		self.size
	}

	/// How many buffers are made and not destroyed, stale ones included.
	pub(crate) fn live(&self) -> usize {
		self.buffers.len()
	}

	/// The buffer to copy the next frame into: the free one holding the
	/// newest frame, else a new one while there may be more.
	pub(crate) fn next(&mut self) -> Pick {
		if self.size.contains(&0) {
			return Pick::Wait;
		}
		let current = self.buffers.iter().filter(|buffer| !buffer.stale);
		let free = current.clone().filter(|buffer| !buffer.held);
		if let Some(buffer) = free.max_by_key(|buffer| buffer.shown) {
			let age = match buffer.shown {
				0 => 0,
				shown => u8::try_from(self.frames - shown + 1).unwrap_or(0),
			};
			return Pick::Reuse { id: buffer.id, age };
		}
		if current.count() == MAX_BUFFERS {
			return Pick::Wait;
		}
		self.made += 1;
		self.buffers.push(Buffer {
			id: self.made,
			held: false,
			shown: 0,
			stale: false,
		});
		Pick::Make { id: self.made }
	}

	/// Buffer `id` could not be made after all: it is gone.
	pub(crate) fn forget(&mut self, id: u64) {
		self.buffers.retain(|buffer| buffer.id != id);
	}

	/// The frame copied into buffer `id` was handed to the compositor.
	pub(crate) fn presented(&mut self, id: u64) {
		self.frames += 1;
		if let Some(buffer) = self.buffers.iter_mut().find(|buffer| buffer.id == id) {
			buffer.held = true;
			buffer.shown = self.frames;
		}
	}

	/// The compositor let go of buffer `id`: whether to destroy it now, as
	/// it is of an old size.
	pub(crate) fn released(&mut self, id: u64) -> bool {
		let Some(index) = self.buffers.iter().position(|buffer| buffer.id == id) else {
			return false;
		};
		let buffer = &mut self.buffers[index];
		if buffer.stale {
			self.buffers.remove(index);
			return true;
		}
		buffer.held = false;
		false
	}

	/// Buffers are made at `size` from now on: the free ones of the old
	/// size, to destroy now; held ones go once released.
	pub(crate) fn resize(&mut self, size: [u32; 2]) -> Vec<u64> {
		if size == self.size {
			return Vec::new();
		}
		self.size = size;
		let mut destroyed = Vec::new();
		self.buffers.retain_mut(|buffer| {
			if buffer.held {
				buffer.stale = true;
			} else {
				destroyed.push(buffer.id);
			}
			buffer.held
		});
		destroyed
	}
}
