//! Vulkan version numbers, and which one the renderer asks for.

/// `VK_MAKE_API_VERSION(0, major, minor, 0)`.
pub(crate) const fn api_version(major: u32, minor: u32) -> u32 {
	major << 22 | minor << 12
}

/// The major and minor version of a packed version number.
pub(crate) const fn major_minor(version: u32) -> (u32, u32) {
	(version >> 22 & 0x7f, version >> 12 & 0x3ff)
}

/// The version to ask an instance for, given the loader's: 1.4 where the
/// loader has it, so its features can be opted into one by one, else 1.3;
/// `None` below 1.3, or for a variant other than Vulkan itself.
pub(crate) fn requested(loader: u32) -> Option<u32> {
	if loader >> 29 != 0 {
		return None;
	}
	match major_minor(loader) {
		version if version >= (1, 4) => Some(api_version(1, 4)),
		version if version >= (1, 3) => Some(api_version(1, 3)),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn versions_pack_as_the_specification_defines_them() {
		// VK_API_VERSION_1_0, VK_API_VERSION_1_3 and VK_API_VERSION_1_4.
		assert_eq!(api_version(1, 0), 0x40_0000);
		assert_eq!(api_version(1, 3), 0x40_3000);
		assert_eq!(api_version(1, 4), 0x40_4000);
		// 1.4.363, as Mesa reports it, and the largest fields.
		assert_eq!(major_minor(0x40_4000 | 363), (1, 4));
		assert_eq!(major_minor(0x7f << 22 | 0x3ff << 12 | 0xfff), (0x7f, 0x3ff));
		assert_eq!(major_minor(7 << 29 | api_version(1, 2)), (1, 2));
	}

	#[test]
	fn instances_ask_for_1_4_where_the_loader_has_it_else_1_3() {
		let cases = [
			(api_version(1, 0), None),
			(api_version(1, 2) | 198, None),
			(api_version(1, 3), Some(api_version(1, 3))),
			(api_version(1, 3) | 280, Some(api_version(1, 3))),
			(api_version(1, 4), Some(api_version(1, 4))),
			(api_version(1, 4) | 363, Some(api_version(1, 4))),
			(api_version(1, 5), Some(api_version(1, 4))),
			(api_version(2, 0), Some(api_version(1, 4))),
			// Vulkan SC and other variants are not Vulkan.
			(1 << 29 | api_version(1, 3), None),
		];
		for (loader, expected) in cases {
			assert_eq!(requested(loader), expected, "{:?}", major_minor(loader));
		}
	}
}
