//! The Vulkan loader's entry point, which finds every other function, and
//! the instance it creates.
use super::{
	call::{Error, check},
	instance::Instance,
	library::Library,
	types::*,
	version::{api_version, requested},
};
use std::{
	ffi::CStr,
	mem::{size_of, transmute_copy},
	ptr,
};

/// The loader's name on Linux and FreeBSD.
pub(crate) const LOADER: &CStr = c"libvulkan.so.1";

pub(crate) struct Entry {
	pub(super) get_instance_proc_addr: GetInstanceProcAddr,
	/// Kept open while the functions it gave are in use.
	pub(super) _library: Option<Library>,
}

impl Entry {
	/// The system's loader.
	pub(crate) fn load() -> Result<Self, Error> {
		Self::from_library(Library::open(LOADER)?)
	}

	pub(crate) fn from_library(library: Library) -> Result<Self, Error> {
		const NAME: &CStr = c"vkGetInstanceProcAddr";
		let symbol = library.symbol(NAME).ok_or(Error::Missing(NAME))?;
		// SAFETY: a Vulkan loader exports vkGetInstanceProcAddr with this type,
		// and a function pointer is the size of a data pointer here.
		let get_instance_proc_addr = unsafe { transmute_copy(&symbol) };
		Ok(Self {
			get_instance_proc_addr,
			_library: Some(library),
		})
	}

	/// The function `name` of `instance`, which is live, or the global
	/// function `name` when `instance` is null.
	pub(crate) fn function(
		&self,
		instance: RawInstance,
		name: &'static CStr,
	) -> Result<VoidFunction, Error> {
		// SAFETY: the loader's own function, given a live or null instance and
		// a C string.
		unsafe { (self.get_instance_proc_addr)(instance, name.as_ptr()) }.ok_or(Error::Missing(name))
	}

	/// The Vulkan version the loader offers; a 1.0 loader lacks the call.
	pub(crate) fn version(&self) -> Result<u32, Error> {
		let Ok(function) = self.function(ptr::null_mut(), c"vkEnumerateInstanceVersion") else {
			return Ok(api_version(1, 0));
		};
		// SAFETY: the function the loader returned for this name has this type.
		let enumerate: EnumerateInstanceVersion = unsafe { cast(function) };
		let mut version = 0;
		// SAFETY: `version` is writable.
		check(unsafe { enumerate(&mut version) })?;
		Ok(version)
	}

	/// An instance named `application`, asking for the newest version the
	/// renderer uses.
	pub(crate) fn create_instance(self, application: &CStr) -> Result<Instance, Error> {
		let loader = self.version()?;
		let api = requested(loader).ok_or(Error::TooOld(loader))?;
		// SAFETY: the function the loader returned for this name has this type.
		let create: CreateInstance =
			unsafe { cast(self.function(ptr::null_mut(), c"vkCreateInstance")?) };
		let application = ApplicationInfo {
			s_type: STRUCTURE_TYPE_APPLICATION_INFO,
			next: ptr::null(),
			application_name: application.as_ptr(),
			application_version: 0,
			engine_name: application.as_ptr(),
			engine_version: 0,
			api_version: api,
		};
		let info = InstanceCreateInfo {
			s_type: STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			application_info: &application,
			enabled_layer_count: 0,
			enabled_layer_names: ptr::null(),
			enabled_extension_count: 0,
			enabled_extension_names: ptr::null(),
		};
		let mut handle = ptr::null_mut();
		// SAFETY: `info` and the strings it points to outlive the call, and
		// `handle` is writable.
		check(unsafe { create(&info, ptr::null(), &mut handle) })?;
		Instance::new(self, handle, api)
	}
}

/// `function` as the type `F` the specification gives it.
///
/// # Safety
///
/// `F` must be the type of the function the loader returned.
pub(crate) unsafe fn cast<F: Copy>(function: VoidFunction) -> F {
	const { assert!(size_of::<F>() == size_of::<VoidFunction>()) };
	// SAFETY: both are function pointers of one size, and the caller vouches
	// for the type.
	unsafe { transmute_copy(&function) }
}
