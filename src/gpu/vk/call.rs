//! Calling Vulkan: result codes as errors, the two-call pattern that lists
//! objects (count first, then the items), and the fixed-size strings it
//! writes.
use std::{
	ffi::{CStr, c_char},
	fmt,
};

/// `VK_INCOMPLETE`: the list had more items than there was room for.
pub(crate) const INCOMPLETE: i32 = 5;

/// How many times a list that keeps growing is asked for before giving up.
const TRIES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
	/// The loader library could not be opened: the system's message.
	NoLoader(String),
	/// The loader or driver lacks this function.
	Missing(&'static CStr),
	/// The loader's Vulkan version, older than the 1.3 the renderer needs.
	TooOld(u32),
	/// A call returned this `VkResult`.
	Failed(i32),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::NoLoader(message) => write!(f, "no Vulkan loader: {message}"),
			Self::Missing(name) => write!(f, "Vulkan lacks {}", name.to_string_lossy()),
			&Self::TooOld(version) => {
				let (major, minor) = super::version::major_minor(version);
				write!(f, "Vulkan {major}.{minor} is older than 1.3")
			}
			Self::Failed(code) => write!(f, "VkResult {code}"),
		}
	}
}

/// `code` when it reports success or a status, else the error.
pub(crate) fn check(code: i32) -> Result<i32, Error> {
	if code < 0 {
		Err(Error::Failed(code))
	} else {
		Ok(code)
	}
}

/// The items a Vulkan list function gives: `call(count, null)` sets the
/// count, then `call(count, items)` writes up to `count` items and sets how
/// many it wrote, answering `VK_INCOMPLETE` when more appeared meanwhile,
/// which asks again.
pub(crate) fn enumerate<T: Copy>(
	zero: T,
	mut call: impl FnMut(&mut u32, *mut T) -> i32,
) -> Result<Vec<T>, Error> {
	for _ in 0..TRIES {
		let mut count = 0;
		check(call(&mut count, std::ptr::null_mut()))?;
		if count == 0 {
			return Ok(Vec::new());
		}
		let mut items = vec![zero; count as usize];
		if check(call(&mut count, items.as_mut_ptr()))? != INCOMPLETE {
			items.truncate(count as usize);
			return Ok(items);
		}
	}
	Err(Error::Failed(INCOMPLETE))
}

/// The text of a fixed-size C string field, up to its first NUL or its
/// end, with bytes that are not UTF-8 replaced.
pub(crate) fn fixed_string(field: &[c_char]) -> String {
	let bytes = field.iter().map(|&c| c as u8).take_while(|&byte| byte != 0);
	String::from_utf8_lossy(&bytes.collect::<Vec<u8>>()).into_owned()
}
