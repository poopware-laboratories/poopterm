//! GIF's variable-length LZW (GIF89a, appendix F): codes of `min + 1` to
//! 12 bits, least significant bit first, with clear and end codes, and a
//! table that stops growing when full.

/// Most codes a table holds: 12-bit codes.
const TABLE: usize = 4096;

/// Decodes `data` with the minimum code size `min` (2 to 8), handing each
/// of at most `limit` indices to `put` in order, without keeping them;
/// what decodes past `limit` is dropped. `None` for a code that is not yet
/// in the table; a stream ending early keeps what it gave.
pub(crate) fn decode_into(
	data: &[u8],
	min: u8,
	limit: usize,
	mut put: impl FnMut(u8),
) -> Option<()> {
	if !(2..=8).contains(&min) {
		return None;
	}
	let (clear, end) = (1usize << min, (1usize << min) + 1);
	// Each entry: the entry before it (its prefix), its last byte, its length.
	let mut prefix = vec![0u16; TABLE];
	let mut last = vec![0u8; TABLE];
	let mut length = vec![0u16; TABLE];
	for code in 0..clear {
		(last[code], length[code]) = (code as u8, 1);
	}
	// The string of one code, written back to front: no longer than the table.
	let mut string = [0u8; TABLE];
	let mut given = 0;
	let (mut size, mut next, mut previous) = (min + 1, end + 1, None::<usize>);
	let (mut bits, mut held) = (0u32, 0u8);
	let mut bytes = data.iter();
	loop {
		while held < size {
			let Some(&byte) = bytes.next() else {
				return Some(());
			};
			bits |= u32::from(byte) << held;
			held += 8;
		}
		let code = (bits & ((1 << size) - 1)) as usize;
		(bits, held) = (bits >> size, held - size);
		if code == clear {
			(size, next, previous) = (min + 1, end + 1, None);
			continue;
		}
		if code == end {
			return Some(());
		}
		match previous {
			// The first code after a clear is a root.
			None if code < clear => {}
			None => return None,
			Some(previous) => {
				// A code just being defined is the previous one plus its own
				// first byte.
				let known = code < next;
				if !known && code != next {
					return None;
				}
				let source = if known { code } else { previous };
				let first = first_byte(&prefix, &length, source);
				if next < TABLE {
					(prefix[next], last[next]) = (previous as u16, last[first]);
					length[next] = length[previous] + 1;
					next += 1;
					if next == 1 << size && size < 12 {
						size += 1;
					}
				}
			}
		}
		let count = usize::from(length[code]);
		let mut at = code;
		for slot in string[..count].iter_mut().rev() {
			*slot = last[at];
			at = usize::from(prefix[at]);
		}
		let room = limit - given;
		string[..count.min(room)]
			.iter()
			.for_each(|&index| put(index));
		given += count.min(room);
		if given == limit {
			return Some(());
		}
		previous = Some(code);
	}
}

/// The root a code's string starts with.
fn first_byte(prefix: &[u16], length: &[u16], mut code: usize) -> usize {
	while length[code] > 1 {
		code = usize::from(prefix[code]);
	}
	code
}
