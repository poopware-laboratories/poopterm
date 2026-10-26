//! What the X server's connection setup says about putting our pixels in a
//! window, and how many rows of them fit in one `PutImage` request.

/// The bytes before the vendor string, in a successful setup.
const HEADER: usize = 40;
/// The bytes of a `PutImage` request before its pixels.
const PUT_IMAGE_HEADER: usize = 24;

/// Whether the server takes `ZPixmap` images of `depth` as our pixels
/// are: 32 bits each, in this machine's byte order. `setup` is the setup
/// the server sent, whose length field bounds what is read.
pub(crate) fn takes_our_pixels(setup: &[u8], depth: u8) -> bool {
	let field = |at: usize| setup.get(at..at + 2).map(|bytes| [bytes[0], bytes[1]]);
	let (Some(length), Some(vendor)) = (field(6), field(24)) else {
		return false;
	};
	let ours = if cfg!(target_endian = "little") { 0 } else { 1 };
	if setup[0] != 1 || setup.get(30) != Some(&ours) {
		return false;
	}
	let end = (8 + 4 * usize::from(u16::from_ne_bytes(length))).min(setup.len());
	let start = HEADER + usize::from(u16::from_ne_bytes(vendor)).div_ceil(4) * 4;
	let formats = usize::from(setup[29]);
	let listed = setup
		.get(start..end)
		.unwrap_or_default()
		.chunks_exact(8)
		.take(formats);
	listed
		.into_iter()
		.any(|format| format[0] == depth && format[1] == 32)
}

/// The rows `[start, end)` of each `PutImage` request for a frame `width`
/// pixels wide and `height` high, each as many whole rows as a request of
/// at most `max_bytes` holds; `None` when not even one row fits.
pub(crate) fn put_rows(
	width: usize,
	height: usize,
	max_bytes: usize,
) -> Option<Vec<(usize, usize)>> {
	if width == 0 || height == 0 {
		return Some(Vec::new());
	}
	let per = max_bytes.saturating_sub(PUT_IMAGE_HEADER) / (width * 4);
	let starts = (0..height).step_by(per.max(1));
	(per > 0).then(|| {
		starts
			.map(|start| (start, (start + per).min(height)))
			.collect()
	})
}
