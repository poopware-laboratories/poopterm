//! The layouts of the surface create infos against the specification's C
//! declarations on 64-bit targets: size, alignment and every field's
//! offset.
use super::surface_types::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn wayland_surface_create_info_is_laid_out_as_vk_wayland_surface_create_info_khr() {
	let size = (
		size_of::<WaylandSurfaceCreateInfo>(),
		align_of::<WaylandSurfaceCreateInfo>(),
	);
	assert_eq!(size, (40, 8));
	let offsets = [
		offset_of!(WaylandSurfaceCreateInfo, s_type),
		offset_of!(WaylandSurfaceCreateInfo, next),
		offset_of!(WaylandSurfaceCreateInfo, flags),
		offset_of!(WaylandSurfaceCreateInfo, display),
		offset_of!(WaylandSurfaceCreateInfo, surface),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32]);
}

#[test]
fn xlib_surface_create_info_is_laid_out_as_vk_xlib_surface_create_info_khr() {
	let size = (
		size_of::<XlibSurfaceCreateInfo>(),
		align_of::<XlibSurfaceCreateInfo>(),
	);
	assert_eq!(size, (40, 8));
	let offsets = [
		offset_of!(XlibSurfaceCreateInfo, s_type),
		offset_of!(XlibSurfaceCreateInfo, next),
		offset_of!(XlibSurfaceCreateInfo, flags),
		offset_of!(XlibSurfaceCreateInfo, display),
		offset_of!(XlibSurfaceCreateInfo, window),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32]);
}

#[test]
fn headless_surface_create_info_is_laid_out_as_vk_headless_surface_create_info_ext() {
	let size = (
		size_of::<HeadlessSurfaceCreateInfo>(),
		align_of::<HeadlessSurfaceCreateInfo>(),
	);
	assert_eq!(size, (24, 8));
	let offsets = [
		offset_of!(HeadlessSurfaceCreateInfo, s_type),
		offset_of!(HeadlessSurfaceCreateInfo, next),
		offset_of!(HeadlessSurfaceCreateInfo, flags),
	];
	assert_eq!(offsets, [0, 8, 16]);
}
