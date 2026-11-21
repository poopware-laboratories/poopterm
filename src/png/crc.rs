//! CRC-32 as PNG chunks carry it (ISO 3309, polynomial 0xEDB88320,
//! reflected), eight bytes a step from tables built at compile time
//! ("slicing by eight").
const TABLES: [[u32; 256]; 8] = {
	let mut tables = [[0; 256]; 8];
	let mut n = 0;
	while n < 256 {
		let mut c = n as u32;
		let mut k = 0;
		while k < 8 {
			c = if c & 1 != 0 {
				0xedb8_8320 ^ (c >> 1)
			} else {
				c >> 1
			};
			k += 1;
		}
		tables[0][n] = c;
		n += 1;
	}
	// Table k is the CRC of a byte followed by k zero bytes.
	let mut k = 1;
	while k < 8 {
		let mut n = 0;
		while n < 256 {
			let previous = tables[k - 1][n];
			tables[k][n] = tables[0][(previous & 0xff) as usize] ^ (previous >> 8);
			n += 1;
		}
		k += 1;
	}
	tables
};

/// The CRC of `parts` one after another.
pub(crate) fn crc32(parts: &[&[u8]]) -> u32 {
	let byte = |c: u32, b: u8| TABLES[0][((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
	let mut c = !0u32;
	for part in parts {
		let (eights, rest) = part.as_chunks::<8>();
		for eight in eights {
			let low = c ^ u32::from_le_bytes([eight[0], eight[1], eight[2], eight[3]]);
			let t = |k: usize, value: u32| TABLES[k][(value & 0xff) as usize];
			c = t(7, low)
				^ t(6, low >> 8)
				^ t(5, low >> 16)
				^ t(4, low >> 24)
				^ t(3, u32::from(eight[4]))
				^ t(2, u32::from(eight[5]))
				^ t(1, u32::from(eight[6]))
				^ t(0, u32::from(eight[7]));
		}
		c = rest.iter().fold(c, |c, &b| byte(c, b));
	}
	!c
}
