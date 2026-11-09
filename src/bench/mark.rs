//! The end of a benchmarked program's output, marked in-band. Some kernels
//! (FreeBSD's) drop what a PTY still holds once its program exits, so the
//! program's exit cannot end the reading: a wrapper prints [`MARK`] after
//! the program and waits, and the reader ends at the mark instead.

use std::{
	io::{self, Read},
	simd::{Simd, cmp::SimdPartialEq},
};

use crate::pty::reader::READ_SIZE;

/// Where `needle` (not empty) first starts in `haystack`. The reader that
/// times the PTY stages runs this over every byte, so it skips 32 bytes at
/// a time to the next possible first byte instead of comparing at each
/// offset, which took half the stage's time.
pub(super) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
	let last = haystack.len().checked_sub(needle.len())?;
	let first = Simd::<u8, 32>::splat(needle[0]);
	let mut at = 0;
	while at <= last {
		let ahead = &haystack[at..=last];
		let skip = match ahead.get(..32) {
			Some(chunk) => {
				let hits = Simd::<u8, 32>::from_slice(chunk)
					.simd_eq(first)
					.to_bitmask();
				if hits == 0 {
					at += 32;
					continue;
				}
				hits.trailing_zeros() as usize
			}
			None => ahead.iter().position(|&byte| byte == needle[0])?,
		};
		at += skip;
		if haystack[at..].starts_with(needle) {
			return Some(at);
		}
		at += 1;
	}
	None
}

/// Printed after the program's output. Printable ASCII, so output
/// processing passes it unchanged, and absent from every workload.
pub(crate) const MARK: &[u8] = b"<poopterm:bench:end:7f3c>";

/// `inner` up to [`MARK`], then the end, as if the file ended there. The
/// mark itself is never passed on, nor anything after it; without one, all
/// of `inner` is.
pub(crate) struct UntilMark<R> {
	inner: R,
	/// Read but not yet passed on: whatever may be the start of the mark.
	held: Vec<u8>,
	ended: bool,
}

impl<R: Read> UntilMark<R> {
	pub(crate) fn new(inner: R) -> Self {
		let (held, ended) = (Vec::new(), false);
		Self { inner, held, ended }
	}
}

impl<R: Read> Read for UntilMark<R> {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		loop {
			// Until the end, the last bytes held may begin the mark.
			let keep = if self.ended { 0 } else { MARK.len() - 1 };
			let ready = self.held.len().saturating_sub(keep).min(buf.len());
			if ready > 0 || self.ended {
				buf[..ready].copy_from_slice(&self.held[..ready]);
				self.held.drain(..ready);
				return Ok(ready);
			}
			let len = self.held.len();
			self.held.resize(len + READ_SIZE, 0);
			let read = self.inner.read(&mut self.held[len..]);
			self.held.truncate(len + read.as_ref().map_or(0, |&n| n));
			self.ended = read? == 0;
			// Earlier searches saw every mark that ends before the new bytes.
			let from = len.saturating_sub(MARK.len() - 1);
			if let Some(at) = find(&self.held[from..], MARK) {
				self.held.truncate(from + at);
				self.ended = true;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::VecDeque;

	/// A reader returning `chunks` one per read, then the end; it panics if
	/// read past `stop` reads, as a program's output past the mark never
	/// arrives.
	struct Chunks {
		chunks: VecDeque<Vec<u8>>,
		reads: usize,
		stop: usize,
	}

	fn chunks(chunks: &[&[u8]]) -> Chunks {
		let stop = usize::MAX;
		Chunks {
			chunks: chunks.iter().map(|chunk| chunk.to_vec()).collect(),
			reads: 0,
			stop,
		}
	}

	impl Read for Chunks {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			self.reads += 1;
			assert!(self.reads <= self.stop, "read past the mark");
			let Some(chunk) = self.chunks.pop_front() else {
				return Ok(0);
			};
			assert!(chunk.len() <= buf.len());
			buf[..chunk.len()].copy_from_slice(&chunk);
			Ok(chunk.len())
		}
	}

	/// Everything `reader` passes on, read `size` bytes at a time.
	fn all(mut reader: impl Read, size: usize) -> Vec<u8> {
		let (mut out, mut buf) = (Vec::new(), vec![0; size]);
		loop {
			match reader.read(&mut buf).unwrap() {
				0 => return out,
				n => out.extend_from_slice(&buf[..n]),
			}
		}
	}

	#[test]
	fn output_ends_at_the_mark() {
		let chunk = [&b"output"[..], MARK, b"echo"].concat();
		assert_eq!(all(UntilMark::new(chunks(&[&chunk])), 64), b"output");
	}

	#[test]
	fn nothing_is_read_past_the_mark() {
		let mut inner = chunks(&[b"a", MARK, b"b"]);
		inner.stop = 2;
		assert_eq!(all(UntilMark::new(inner), 64), b"a");
	}

	#[test]
	fn a_mark_split_anywhere_across_reads_is_found() {
		for at in 1..MARK.len() {
			let first = [&b"data"[..], &MARK[..at]].concat();
			let second = [&MARK[at..], b"after"].concat();
			let read = all(UntilMark::new(chunks(&[&first, &second])), 64);
			assert_eq!(read, b"data", "split at {at}");
		}
	}

	#[test]
	fn the_start_of_a_mark_that_goes_on_differently_is_passed_on() {
		let almost = &MARK[..MARK.len() - 1];
		let input = [almost, b"x", almost].concat();
		assert_eq!(
			all(UntilMark::new(chunks(&[almost, b"x", almost])), 64),
			input
		);
	}

	#[test]
	fn without_a_mark_everything_is_passed_on() {
		assert_eq!(
			all(UntilMark::new(chunks(&[b"one", b"two"])), 64),
			b"onetwo"
		);
		assert_eq!(all(UntilMark::new(chunks(&[])), 64), b"");
	}

	#[test]
	fn small_reads_get_every_byte_before_the_mark() {
		let data: Vec<u8> = (0..=255).collect();
		let input = [&data[..], MARK].concat();
		assert_eq!(all(UntilMark::new(chunks(&[&input])), 1), data);
		assert_eq!(all(UntilMark::new(chunks(&[&input])), 7), data);
	}

	#[test]
	fn a_mark_first_is_an_empty_output() {
		let mut reader = UntilMark::new(chunks(&[MARK]));
		assert_eq!(reader.read(&mut [0; 8]).unwrap(), 0);
		// And it stays ended.
		assert_eq!(reader.read(&mut [0; 8]).unwrap(), 0);
	}

	#[test]
	fn read_errors_reach_the_caller() {
		struct Broken;
		impl Read for Broken {
			fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
				Err(io::Error::other("gone"))
			}
		}
		let error = UntilMark::new(Broken).read(&mut [0; 8]).unwrap_err();
		assert_eq!(error.to_string(), "gone");
	}

	#[test]
	fn the_mark_passes_output_processing_unchanged() {
		assert!(
			MARK.iter().all(|byte| byte.is_ascii_graphic()),
			"no \\n, \\t or controls"
		);
	}

	#[test]
	fn an_interrupted_read_loses_nothing_held() {
		struct Interrupting(Vec<io::Result<Vec<u8>>>);
		impl Read for Interrupting {
			fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
				let chunk = self.0.remove(0)?;
				buf[..chunk.len()].copy_from_slice(&chunk);
				Ok(chunk.len())
			}
		}
		let interrupted = Err(io::ErrorKind::Interrupted.into());
		let inner = Interrupting(vec![Ok(b"abc".to_vec()), interrupted, Ok(MARK.to_vec())]);
		let mut reader = UntilMark::new(inner);
		let error = reader.read(&mut [0; 64]).unwrap_err();
		assert_eq!(error.kind(), io::ErrorKind::Interrupted);
		let mut buf = [0; 64];
		assert_eq!(reader.read(&mut buf).unwrap(), 3);
		assert_eq!(&buf[..3], b"abc");
	}

	#[test]
	fn a_mark_ending_a_full_read_is_found() {
		let mut full = vec![b'x'; READ_SIZE - MARK.len()];
		full.extend_from_slice(MARK);
		let read = all(UntilMark::new(chunks(&[&full, b"after"])), READ_SIZE);
		assert_eq!(read.len(), READ_SIZE - MARK.len());
	}

	/// The obvious search, to check `find` against.
	fn naive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
		haystack
			.windows(needle.len())
			.position(|window| window == needle)
	}

	#[test]
	fn find_agrees_with_the_obvious_search_across_vector_boundaries() {
		let mut rng = super::super::corpus::Rng::new(274);
		// Mostly the mark's own bytes, so near misses are everywhere.
		let alphabet = b"<poopterm:bench:end:7f3c>x";
		for len in 0..200 {
			for _ in 0..20 {
				let mut haystack: Vec<u8> = (0..len)
					.map(|_| alphabet[rng.below(alphabet.len())])
					.collect();
				if len >= MARK.len() && rng.below(2) == 0 {
					let at = rng.below(len - MARK.len() + 1);
					haystack[at..at + MARK.len()].copy_from_slice(MARK);
				}
				assert_eq!(
					find(&haystack, MARK),
					naive(&haystack, MARK),
					"{haystack:?}"
				);
				assert_eq!(find(&haystack, b"<p"), naive(&haystack, b"<p"));
			}
		}
	}

	#[test]
	fn find_reports_the_first_of_several_matches_at_every_offset() {
		for at in 0..70 {
			let mut haystack = vec![b'.'; 140];
			haystack[at..at + MARK.len()].copy_from_slice(MARK);
			haystack[at + 40..at + 40 + MARK.len()].copy_from_slice(MARK);
			assert_eq!(find(&haystack, MARK), Some(at), "at {at}");
		}
	}

	#[test]
	fn find_handles_haystacks_shorter_than_or_ending_in_the_needle() {
		assert_eq!(find(b"", MARK), None);
		assert_eq!(find(&MARK[1..], MARK), None);
		assert_eq!(find(MARK, MARK), Some(0));
		let tail = [&[b'<'; 33][..], MARK].concat();
		assert_eq!(find(&tail, MARK), Some(33));
		assert_eq!(find(&tail[..tail.len() - 1], MARK), None);
	}

	#[test]
	fn near_misses_across_many_reads_pass_through_until_the_mark() {
		// Each read ends in most of the mark and the next one breaks it off,
		// so every search starts inside what the last one saw.
		let almost = &MARK[..MARK.len() - 1];
		let reads: Vec<Vec<u8>> = (0..50)
			.map(|n| [&[b'a' + n % 26][..], almost].concat())
			.collect();
		let mut inner: Vec<&[u8]> = reads.iter().map(Vec::as_slice).collect();
		inner.push(MARK);
		let expected = reads.concat();
		assert_eq!(all(UntilMark::new(chunks(&inner)), 64), expected);
	}

	#[test]
	fn no_workload_holds_the_mark() {
		let size = super::super::corpus::Size { cols: 80, rows: 24 };
		for workload in super::super::corpus::WORKLOADS {
			let data = workload.build(256 << 10, size);
			let found = data.windows(MARK.len()).any(|bytes| bytes == MARK);
			assert!(!found, "{}", workload.name);
		}
	}
}
