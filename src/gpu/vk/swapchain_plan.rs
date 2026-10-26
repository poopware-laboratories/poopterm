//! What swapchain to make for a surface: its format, present mode, size,
//! image count and alpha, chosen from what the surface supports as plain
//! data, so the choice is tested without a driver.
use super::image::Format;

/// `VK_FORMAT_UNDEFINED`, which a surface lists alone to take any format.
const UNDEFINED: i32 = 0;
/// `VK_COLOR_SPACE_SRGB_NONLINEAR_KHR`: values as software writes them.
pub(crate) const SRGB_NONLINEAR: i32 = 0;
/// `VkPresentModeKHR`.
pub(crate) const MAILBOX: i32 = 1;
pub(crate) const FIFO: i32 = 2;
/// `VkCompositeAlphaFlagBitsKHR`.
pub(crate) const OPAQUE: u32 = 1;
pub(crate) const PRE_MULTIPLIED: u32 = 2;
pub(crate) const POST_MULTIPLIED: u32 = 4;
pub(crate) const INHERIT: u32 = 8;
/// `VK_IMAGE_USAGE_TRANSFER_DST_BIT` and `_COLOR_ATTACHMENT_BIT`: frames
/// are drawn into the images, or copied in when software drew them.
pub(crate) const USAGE: u32 = 0x2 | 0x10;

/// What a surface supports, from `VkSurfaceCapabilitiesKHR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Capabilities {
	pub min_images: u32,
	/// Zero for no limit.
	pub max_images: u32,
	/// `u32::MAX` twice when the swapchain sets the size, as on Wayland.
	pub current_extent: [u32; 2],
	pub min_extent: [u32; 2],
	pub max_extent: [u32; 2],
	pub current_transform: u32,
	pub alpha: u32,
	pub usage: u32,
}

/// `VkSurfaceFormatKHR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceFormat {
	pub format: i32,
	pub color_space: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
	pub format: Format,
	pub mode: i32,
	pub extent: [u32; 2],
	pub images: u32,
	pub transform: u32,
	pub alpha: u32,
}

/// Why no swapchain suits the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unfit {
	/// The window has no area, as when minimized: draw nothing for now.
	Hidden,
	/// No 8-bit UNORM format in sRGB nonlinear values, which the shaders
	/// blend in as software does.
	Format,
	/// Images cannot be drawn to or copied into.
	Usage,
	/// No composite alpha at all, which a driver must not report.
	Alpha,
}

/// The swapchain for a surface supporting `capabilities`, `formats` and
/// `modes`, in a window of `window` pixels.
pub(crate) fn plan(
	capabilities: &Capabilities,
	formats: &[SurfaceFormat],
	modes: &[i32],
	window: [u32; 2],
) -> Result<Plan, Unfit> {
	let extent = if capabilities.current_extent == [u32::MAX; 2] {
		let (min, max) = (capabilities.min_extent, capabilities.max_extent);
		// Not `clamp`, which panics when a broken driver reports min > max.
		[0, 1].map(|axis| window[axis].max(min[axis]).min(max[axis]))
	} else {
		capabilities.current_extent
	};
	if extent.contains(&0) {
		return Err(Unfit::Hidden);
	}
	if capabilities.usage & USAGE != USAGE {
		return Err(Unfit::Usage);
	}
	let alpha = [OPAQUE, INHERIT, PRE_MULTIPLIED, POST_MULTIPLIED]
		.into_iter()
		.find(|&alpha| capabilities.alpha & alpha != 0)
		.ok_or(Unfit::Alpha)?;
	let format = if let [only] = formats
		&& only.format == UNDEFINED
	{
		Format::Bgra
	} else {
		[Format::Bgra, Format::Rgba]
			.into_iter()
			.find(|&format| {
				let wanted = SurfaceFormat {
					format: format as i32,
					color_space: SRGB_NONLINEAR,
				};
				formats.contains(&wanted)
			})
			.ok_or(Unfit::Format)?
	};
	let images = capabilities.min_images.saturating_add(1);
	let max = capabilities.max_images;
	Ok(Plan {
		format,
		mode: if modes.contains(&MAILBOX) {
			MAILBOX
		} else {
			FIFO
		},
		extent,
		images: if max == 0 { images } else { images.min(max) },
		transform: capabilities.current_transform,
		alpha,
	})
}
