//! Tests of the GPU's facts as the Diagnostics page lists them: the
//! design's five rows, and how each reads when the GPU leaves it unknown.
use super::{
	atlas::Usage,
	facts::{GpuFacts, Swapchain, mode_name, version},
};

/// The design's example: a Radeon on Mesa, three FIFO images on Wayland.
fn radeon() -> GpuFacts {
	GpuFacts {
		device: "AMD Radeon RX 7800 XT (RADV NAVI32)".into(),
		api: "Vulkan",
		version: Some(1 << 22 | 4 << 12 | 321),
		driver: Some("Mesa 26.2.1".into()),
		swapchain: Some(Swapchain {
			format: "B8G8R8A8_UNORM",
			mode: 2,
			images: 3,
		}),
		atlas: Some(Usage {
			side: 2048,
			pages: 1,
			filled: 2048 * 2048 * 38 / 100 + 7,
		}),
		presenter: "Vulkan on Wayland".into(),
	}
}

#[test]
fn the_designs_five_rows_read_as_it_shows_them() {
	assert_eq!(
		radeon().rows(),
		[
			("Device", "AMD Radeon RX 7800 XT (RADV NAVI32)".to_owned()),
			("Driver", "Mesa 26.2.1 \u{b7} Vulkan 1.4.321".to_owned()),
			(
				"Swapchain",
				"B8G8R8A8_UNORM \u{b7} FIFO \u{b7} 3 images".to_owned()
			),
			("Glyph atlas", "2048 \u{d7} 2048 \u{b7} 38% used".to_owned()),
			("Presenter", "Vulkan on Wayland".to_owned()),
		]
	);
}

/// The value of `label`'s row, if listed.
fn row(facts: &GpuFacts, label: &str) -> Option<String> {
	let rows = facts.rows();
	rows
		.into_iter()
		.find(|(name, _)| *name == label)
		.map(|(_, value)| value)
}

#[test]
fn versions_read_major_minor_patch() {
	assert_eq!(version(1 << 22 | 3 << 12), "1.3.0");
	assert_eq!(version(1 << 22 | 4 << 12 | 4095), "1.4.4095");
	// The variant bits are not part of the version.
	assert_eq!(version(7 << 29 | 1 << 22 | 2 << 12 | 5), "1.2.5");
}

#[test]
fn present_modes_go_by_their_specification_names() {
	let names: Vec<String> = (0..4).map(mode_name).collect();
	assert_eq!(names, ["IMMEDIATE", "MAILBOX", "FIFO", "FIFO_RELAXED"]);
	// Modes from extensions, unnamed here, keep their number.
	assert_eq!(mode_name(1_000_111_000), "mode 1000111000");
	assert_eq!(mode_name(-1), "mode -1");
}

#[test]
fn a_driver_without_info_shows_the_version_alone_and_vice_versa() {
	let facts = GpuFacts {
		driver: None,
		..radeon()
	};
	assert_eq!(row(&facts, "Driver").unwrap(), "Vulkan 1.4.321");
	let facts = GpuFacts {
		version: None,
		..radeon()
	};
	assert_eq!(row(&facts, "Driver").unwrap(), "Mesa 26.2.1");
	let facts = GpuFacts {
		version: None,
		driver: None,
		..radeon()
	};
	assert_eq!(row(&facts, "Driver"), None);
}

#[test]
fn an_empty_driver_string_counts_as_none() {
	let facts = GpuFacts {
		driver: Some("  ".into()),
		..radeon()
	};
	assert_eq!(row(&facts, "Driver").unwrap(), "Vulkan 1.4.321");
}

#[test]
fn one_image_is_singular() {
	let facts = GpuFacts {
		swapchain: Some(Swapchain {
			format: "R8G8B8A8_UNORM",
			mode: 1,
			images: 1,
		}),
		..radeon()
	};
	assert_eq!(
		row(&facts, "Swapchain").unwrap(),
		"R8G8B8A8_UNORM \u{b7} MAILBOX \u{b7} 1 image"
	);
}

#[test]
fn an_atlas_of_several_pages_says_how_many() {
	let facts = GpuFacts {
		atlas: Some(Usage {
			side: 1024,
			pages: 4,
			filled: 0,
		}),
		..radeon()
	};
	assert_eq!(
		row(&facts, "Glyph atlas").unwrap(),
		"1024 \u{d7} 1024 \u{d7} 4 pages \u{b7} 0% used"
	);
}

#[test]
fn rows_the_gpu_does_not_know_are_left_out_in_order() {
	// Before the first frame there is no swapchain; Metal has no driver
	// facts here.
	let facts = GpuFacts {
		device: "Apple M2".into(),
		api: "Metal",
		version: None,
		driver: None,
		swapchain: None,
		atlas: None,
		presenter: "Metal on macOS".into(),
	};
	let labels: Vec<&str> = facts.rows().iter().map(|(label, _)| *label).collect();
	assert_eq!(labels, ["Device", "Presenter"]);
}

#[test]
fn the_metal_version_names_metal() {
	let facts = GpuFacts {
		api: "Metal",
		driver: None,
		version: Some(3 << 22),
		..radeon()
	};
	assert_eq!(row(&facts, "Driver").unwrap(), "Metal 3.0.0");
}
