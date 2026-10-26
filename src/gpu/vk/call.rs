//! Calling Vulkan: result codes as errors, the two-call pattern that lists
//! objects (count first, then the items), and the fixed-size strings it
//! writes.
use super::{choose::Unsuited, swapchain_plan::Unfit};
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
	/// The device lacks this feature, which the renderer needs.
	Lacks(&'static str),
	/// No memory type suits the resource and what it is for.
	NoMemory,
	/// An image's width, height and layers, beyond what the device allows
	/// or zero.
	ImageSize([u32; 3]),
	/// A copy outside its buffer or image, of nothing, between devices, or
	/// with a buffer or image not made for it.
	Copy,
	/// A shader that is not SPIR-V Vulkan 1.3 takes.
	Spirv,
	/// A draw of quads past its buffer's end, between devices, to the
	/// atlas itself, or with a buffer or image not made for it.
	Draw,
	/// A quad set already drawn with in the recording, or by commands the
	/// GPU has yet to finish.
	Busy,
	/// No swapchain suits the window's surface, or not for now.
	Unfit(Unfit),
	/// A swapchain on a device and surface of different instances.
	Foreign,
	/// A swapchain on a surface the device's queue cannot show on.
	Unshown,
	/// No device draws in the window: each one's name, and why.
	NoDevice(Vec<(String, Unsuited)>),
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
			Self::Lacks(feature) => write!(f, "the Vulkan device lacks {feature}"),
			Self::NoMemory => write!(f, "no Vulkan memory type suits the resource"),
			Self::ImageSize([width, height, layers]) => write!(
				f,
				"the Vulkan device cannot make a {width}×{height} image of {layers} layers"
			),
			Self::Copy => write!(f, "a Vulkan copy outside what it copies between"),
			Self::Spirv => write!(f, "a shader is not SPIR-V for Vulkan 1.3"),
			Self::Draw => write!(f, "a Vulkan draw of quads it cannot draw"),
			Self::Busy => write!(f, "a Vulkan quad set still in use"),
			Self::Unfit(Unfit::Hidden) => write!(f, "the window has no area to draw in"),
			Self::Unfit(Unfit::Format) => {
				write!(f, "the Vulkan surface offers no 8-bit UNORM sRGB format")
			}
			Self::Unfit(Unfit::Usage) => {
				write!(
					f,
					"Vulkan swapchain images cannot be drawn to or copied into"
				)
			}
			Self::Unfit(Unfit::Alpha) => write!(f, "the Vulkan surface reports no composite alpha"),
			Self::Foreign => write!(f, "a Vulkan device and surface of different instances"),
			Self::Unshown => write!(f, "the Vulkan device cannot show on the window's surface"),
			Self::NoDevice(devices) if devices.is_empty() => write!(f, "no Vulkan device"),
			Self::NoDevice(devices) => {
				write!(f, "no Vulkan device suits")?;
				for (index, (name, why)) in devices.iter().enumerate() {
					write!(f, "{}{name} ", if index == 0 { ": " } else { "; " })?;
					match why {
						&Unsuited::TooOld(version) => {
							let (major, minor) = super::version::major_minor(version);
							write!(f, "is Vulkan {major}.{minor}, older than 1.3")
						}
						Unsuited::Cpu => write!(f, "draws on the CPU, slower than software"),
						Unsuited::NoSwapchain => write!(f, "cannot show in windows"),
						Unsuited::NoQueue => write!(f, "has no queue that draws and shows in the window"),
					}?;
				}
				Ok(())
			}
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
