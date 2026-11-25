//! Zlib (RFC 1950) and DEFLATE (RFC 1951) decompression of untrusted input,
//! refusing output past a limit before allocating it.
use std::simd::u8x16;

use self::{
	bits::Bits,
	huffman::{BASE, END, INVALID, KIND, LITERAL, Table, extra, length},
};

mod adler;
mod bits;
pub(crate) mod gzip;
mod huffman;

/// Decompresses a zlib stream into at most `limit` bytes, checking its
/// header and checksum, with room for `expected` bytes allocated up front.
/// Input after the checksum is ignored.
pub(crate) fn zlib(data: &[u8], expected: usize, limit: usize) -> Option<Vec<u8>> {
	let (&[method, flags], data) = data.split_first_chunk()?;
	// Deflate with a window of 32 KiB at most, and no preset dictionary.
	let check = u16::from_be_bytes([method, flags]) % 31;
	if method & 0x0f != 8 || method >> 4 > 7 || check != 0 || flags & 0x20 != 0 {
		return None;
	}
	let mut inflater = Inflater::new(data, expected.min(limit), limit);
	inflater.run()?;
	// The fast loop may finish a match past the limit.
	if inflater.len > limit {
		return None;
	}
	let checksum = inflater.bits.bytes(4)?;
	let out = inflater.finish();
	(adler::adler32(&out).to_be_bytes() == checksum).then_some(out)
}

/// Bases and extra bit counts for `N` symbols from `first` on, whose extra
/// bits grow by one every `step` symbols after the first `2 * step`.
const fn bases<const N: usize>(first: u16, step: usize) -> [(u16, u8); N] {
	let (mut table, mut base, mut symbol) = ([(0, 0); N], first, 0);
	while symbol < N {
		let extra = if symbol < 2 * step {
			0
		} else {
			symbol / step - 1
		};
		table[symbol] = (base, extra as u8);
		base += 1 << extra;
		symbol += 1;
	}
	table
}

/// Lengths for symbols 257 to 285, and how many extra bits each has; the
/// last breaks the pattern.
const LENGTHS: [(u16, u8); 29] = {
	let mut lengths = bases(3, 4);
	lengths[28] = (258, 0);
	lengths
};
/// Distances for symbols 0 to 29, and how many extra bits each has.
const DISTANCES: [(u16, u8); 30] = bases(1, 2);
/// The order code length code lengths are sent in.
const ORDER: [usize; 19] = [
	16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn base((base, extra): (u16, u8)) -> u32 {
	BASE | u32::from(base) << 16 | u32::from(extra) << 8
}

fn literal_or_length(symbol: usize) -> u32 {
	match symbol {
		0..256 => LITERAL | (symbol as u32) << 16,
		256 => END,
		257..286 => base(LENGTHS[symbol - 257]),
		_ => INVALID,
	}
}

fn distance(symbol: usize) -> u32 {
	DISTANCES.get(symbol).map_or(INVALID, |&pair| base(pair))
}

/// For each distance under 16, the source byte of each lane of a copy.
const PERIODS: [[u8; 16]; 16] = {
	let mut periods = [[0; 16]; 16];
	let mut distance = 1;
	while distance < 16 {
		let mut lane = 0;
		while lane < 16 {
			periods[distance][lane] = (lane % distance) as u8;
			lane += 1;
		}
		distance += 1;
	}
	periods
};

/// Room past the output for a whole match copied 16 bytes at a time.
const FAST_ROOM: usize = 258 + 16;

struct Inflater<'a> {
	bits: Bits<'a>,
	/// Decompressed bytes up to `len`; the rest is room to write into,
	/// with `FAST_ROOM` bytes past the room reserved for.
	out: Vec<u8>,
	len: usize,
	limit: usize,
	literals: Table<1024>,
	distances: Table<256>,
	/// Whether the tables hold the fixed codes.
	fixed: bool,
}

impl<'a> Inflater<'a> {
	fn new(data: &'a [u8], expected: usize, limit: usize) -> Self {
		Self {
			bits: Bits::new(data),
			out: vec![0; expected + FAST_ROOM],
			len: 0,
			limit,
			literals: Table::new(),
			distances: Table::new(),
			fixed: false,
		}
	}

	fn finish(mut self) -> Vec<u8> {
		self.out.truncate(self.len);
		self.out
	}

	fn run(&mut self) -> Option<()> {
		loop {
			self.bits.refill().then_some(())?;
			let header = self.bits.take(3);
			match header >> 1 {
				0 => self.stored()?,
				1 => {
					if !self.fixed {
						self.fixed_tables();
					}
					self.codes()?;
				}
				2 => {
					self.dynamic_tables()?;
					self.codes()?;
				}
				_ => return None,
			}
			if header & 1 == 1 {
				return Some(());
			}
		}
	}

	fn stored(&mut self) -> Option<()> {
		let header = self.bits.bytes(4)?;
		let length = u16::from_le_bytes([header[0], header[1]]);
		if length != !u16::from_le_bytes([header[2], header[3]]) {
			return None;
		}
		let bytes = self.bits.bytes(length.into())?;
		self.reserve(bytes.len())?;
		self.out[self.len..self.len + bytes.len()].copy_from_slice(bytes);
		self.len += bytes.len();
		Some(())
	}

	fn fixed_tables(&mut self) {
		let mut lengths = [8u8; 288];
		lengths[144..256].fill(9);
		lengths[256..280].fill(7);
		// Complete codes cannot be refused.
		self.literals.build(&lengths, literal_or_length).unwrap();
		self.distances.build(&[5; 32], distance).unwrap();
		self.fixed = true;
	}

	fn dynamic_tables(&mut self) -> Option<()> {
		self.fixed = false;
		self.bits.refill().then_some(())?;
		let literals = self.bits.take(5) + 257;
		let distances = self.bits.take(5) + 1;
		// The fields reach 288 and 32, but RFC 1951 only defines 286 and 30.
		(literals <= 286 && distances <= 30).then_some(())?;
		let mut code_lengths = [0u8; 19];
		for &symbol in &ORDER[..self.bits.take(4) + 4] {
			self.bits.refill().then_some(())?;
			code_lengths[symbol] = self.bits.take(3) as u8;
		}
		let mut codes = Table::<128>::new();
		codes.build(&code_lengths, |symbol| LITERAL | (symbol as u32) << 16)?;
		// Literal and distance code lengths form one run-length coded list.
		let mut lengths = [0u8; 320];
		let (mut i, total) = (0, literals + distances);
		while i < total {
			self.bits.refill().then_some(())?;
			let entry = codes.entry(self.bits.peek());
			if entry & KIND == INVALID {
				return None;
			}
			self.bits.consume(length(entry));
			let (value, repeat) = match entry >> 16 {
				symbol @ 0..16 => (symbol as u8, 1),
				16 => (*lengths[..i].last()?, 3 + self.bits.take(2)),
				17 => (0, 3 + self.bits.take(3)),
				_ => (0, 11 + self.bits.take(7)),
			};
			lengths
				.get_mut(i..i + repeat)
				.filter(|_| i + repeat <= total)?
				.fill(value);
			i += repeat;
		}
		self
			.literals
			.build(&lengths[..literals], literal_or_length)?;
		self.distances.build(&lengths[literals..total], distance)
	}

	/// Decodes a block's symbols up to its end.
	fn codes(&mut self) -> Option<()> {
		loop {
			if self.len + FAST_ROOM <= self.out.len() && self.fast()? {
				return Some(());
			}
			self.bits.refill().then_some(())?;
			let entry = self.literals.entry(self.bits.peek());
			self.bits.consume(length(entry));
			match entry & KIND {
				LITERAL => {
					self.reserve(1)?;
					self.out[self.len] = (entry >> 16) as u8;
					self.len += 1;
				}
				BASE => {
					let count = (entry >> 16) as usize + self.bits.take(extra(entry));
					let distance = distance_after(&mut self.bits, &self.distances)?;
					let from = self.len.checked_sub(distance)?;
					self.reserve(count)?;
					for i in 0..count {
						self.out[self.len + i] = self.out[from + i];
					}
					self.len += count;
				}
				END => return Some(()),
				_ => return None,
			}
		}
	}

	/// Decodes symbols while the output has room for a whole match past
	/// its end, with the reader and length in locals. Returns whether the
	/// block ended.
	#[inline(always)]
	fn fast(&mut self) -> Option<bool> {
		let Self {
			bits,
			out,
			len,
			literals,
			distances,
			..
		} = self;
		let (mut reader, mut at) = (*bits, *len);
		let ended = loop {
			if at + FAST_ROOM > out.len() {
				break Some(false);
			}
			if !reader.refill() {
				break None;
			}
			// Literals with codes the primary table covers skip resolving;
			// longer ones are decoded with lengths, below.
			let mut entry = literals.first(reader.peek());
			// Three literals of at most 15 bits fit in one refill, and the
			// room for a match is room for them.
			let mut literal = 0;
			while entry & KIND == LITERAL && literal < 3 {
				reader.consume(length(entry));
				out[at] = (entry >> 16) as u8;
				at += 1;
				literal += 1;
				entry = literals.first(reader.peek());
			}
			if literal > 0 {
				continue;
			}
			let entry = literals.resolve(entry, reader.peek());
			reader.consume(length(entry));
			match entry & KIND {
				LITERAL => {
					out[at] = (entry >> 16) as u8;
					at += 1;
				}
				BASE => {
					let count = (entry >> 16) as usize + reader.take(extra(entry));
					let Some(distance) = distance_after(&mut reader, distances) else {
						break None;
					};
					if !copy(out, at, distance, count) {
						break None;
					}
					at += count;
				}
				END => break Some(true),
				_ => break None,
			}
		};
		(*bits, *len) = (reader, at);
		ended
	}

	/// Makes room for `n` more bytes, within the limit.
	#[inline(always)]
	fn reserve(&mut self, n: usize) -> Option<()> {
		if self.len + n + FAST_ROOM > self.out.len() {
			return self.grow(n);
		}
		Some(())
	}

	#[cold]
	fn grow(&mut self, n: usize) -> Option<()> {
		let need = self.len.checked_add(n).filter(|&need| need <= self.limit)?;
		let room = (self.out.len() - FAST_ROOM) * 2;
		let mut out = vec![0; room.clamp(need, self.limit) + FAST_ROOM];
		out[..self.len].copy_from_slice(&self.out[..self.len]);
		self.out = out;
		Some(())
	}
}

/// Decodes the distance code that follows a length.
#[inline(always)]
fn distance_after(bits: &mut Bits, distances: &Table<256>) -> Option<usize> {
	let entry = distances.entry(bits.peek());
	if entry & KIND != BASE {
		return None;
	}
	bits.consume(length(entry));
	Some((entry >> 16) as usize + bits.take(extra(entry)))
}

/// Repeats the `count` bytes from `distance` back of `to`, writing up to 16
/// bytes past them; `out` must have room for that. False if `distance`
/// reaches before the output.
#[inline(always)]
fn copy(out: &mut [u8], to: usize, distance: usize, count: usize) -> bool {
	let Some(from) = to.checked_sub(distance) else {
		return false;
	};
	if distance >= 16 {
		// Each 16 bytes are read before the copy reaches them.
		for i in (0..count).step_by(16) {
			u8x16::from_slice(&out[from + i..]).copy_to_slice(&mut out[to + i..]);
		}
	} else if distance == 1 {
		let byte = u8x16::splat(out[from]);
		for i in (0..count).step_by(16) {
			byte.copy_to_slice(&mut out[to + i..]);
		}
	} else {
		// The copy repeats the `distance` bytes before it: spread them over
		// a vector, and store it at offsets that are multiples of the period.
		let lanes = u8x16::from_array(PERIODS[distance]);
		let pattern = u8x16::from_slice(&out[from..]).swizzle_dyn(lanes);
		for i in (0..count).step_by(16 - 16 % distance) {
			pattern.copy_to_slice(&mut out[to + i..]);
		}
	}
	true
}

#[cfg(test)]
mod adler_tests;
#[cfg(test)]
mod bits_tests;
#[cfg(test)]
mod blocks_tests;
#[cfg(test)]
mod gzip_tests;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod huffman_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod throughput_bench;
