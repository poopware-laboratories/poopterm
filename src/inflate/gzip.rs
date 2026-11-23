//! Gzip (RFC 1952) members, as `tar.gz` packages arrive: the header and its
//! optional fields, the DEFLATE data, and the CRC-32 and size that end it,
//! all checked, with output refused past a limit.
use super::Inflater;

/// The CRC-32 (IEEE 802.3, reflected) table, one entry per byte.
const TABLE: [u32; 256] = {
	let mut table = [0; 256];
	let mut byte = 0;
	while byte < 256 {
		let mut crc = byte as u32;
		let mut bit = 0;
		while bit < 8 {
			crc = if crc & 1 == 1 {
				0xedb8_8320 ^ (crc >> 1)
			} else {
				crc >> 1
			};
			bit += 1;
		}
		table[byte] = crc;
		byte += 1;
	}
	table
};

/// The CRC-32 of `data`, as gzip and zip store it.
pub(crate) fn crc32(data: &[u8]) -> u32 {
	!data.iter().fold(!0u32, |crc, &byte| {
		TABLE[usize::from(crc as u8 ^ byte)] ^ (crc >> 8)
	})
}

/// Header flags: a header CRC, an extra field, a name, a comment, and the
/// reserved bits that must be clear.
const FHCRC: u8 = 2;
const FEXTRA: u8 = 4;
const FNAME: u8 = 8;
const FCOMMENT: u8 = 16;
const RESERVED: u8 = 0xe0;

/// Decompresses the gzip member at the start of `data` into at most
/// `limit` bytes, checking its header, CRC-32 and size. Input after the
/// member is ignored.
pub(crate) fn gzip(data: &[u8], limit: usize) -> Option<Vec<u8>> {
	let body = header_end(data)?;
	let mut inflater = Inflater::new(&data[body..], 0, limit);
	inflater.run()?;
	if inflater.len > limit {
		return None;
	}
	let crc = inflater.bits.bytes(4)?;
	let size = inflater.bits.bytes(4)?;
	let out = inflater.finish();
	let checked = crc == crc32(&out).to_le_bytes() && size == (out.len() as u32).to_le_bytes();
	checked.then_some(out)
}

/// Where the DEFLATE data starts after the header at the start of `data`.
fn header_end(data: &[u8]) -> Option<usize> {
	let fixed: &[u8; 10] = data.first_chunk()?;
	if fixed[..3] != [0x1f, 0x8b, 8] || fixed[3] & RESERVED != 0 {
		return None;
	}
	let flags = fixed[3];
	let mut at = 10;
	if flags & FEXTRA != 0 {
		let length = data.get(at..at + 2)?;
		at += 2 + usize::from(u16::from_le_bytes([length[0], length[1]]));
	}
	for field in [FNAME, FCOMMENT] {
		if flags & field != 0 {
			at += data.get(at..)?.iter().position(|&byte| byte == 0)? + 1;
		}
	}
	if flags & FHCRC != 0 {
		let stored = data.get(at..at + 2)?;
		if *stored != (crc32(&data[..at]) as u16).to_le_bytes() {
			return None;
		}
		at += 2;
	}
	(at <= data.len()).then_some(at)
}
