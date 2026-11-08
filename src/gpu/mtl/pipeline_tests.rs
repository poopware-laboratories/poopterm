//! Tests of the quad pipeline: our MSL compiles on the real device with
//! both entry points, and source that does not compile is refused with
//! the compiler's own words.
use super::{Pipeline, SOURCE, function, library};
use crate::{
	gpu::mtl::device::{Error, tests::device},
	objc::Raw,
};

#[test]
fn the_quad_shaders_compile_and_make_a_pipeline() {
	let Some(device) = device() else { return };
	let library = library(&device, SOURCE).unwrap();
	assert!(function(&library, c"quad_vertex").is_ok());
	assert!(function(&library, c"quad_fragment").is_ok());
	let pipeline = Pipeline::new(&device).unwrap();
	assert!(pipeline.state.responds(c"device"));
}

#[test]
fn a_function_the_library_lacks_is_refused() {
	let Some(device) = device() else { return };
	let library = library(&device, SOURCE).unwrap();
	assert_eq!(
		function(&library, c"quad_compute").err(),
		Some(Error::Refused("shader function"))
	);
}

#[test]
fn source_that_does_not_compile_is_refused_with_what_the_compiler_said() {
	let Some(device) = device() else { return };
	let broken = "#include <metal_stdlib>\nvertex float4 broken() { return undeclared; }\n";
	match library(&device, broken) {
		Err(Error::Shader(said)) => assert!(said.contains("undeclared"), "{said}"),
		Err(other) => panic!("{other}"),
		Ok(_) => panic!("compiled"),
	}
}

#[test]
fn the_pipeline_blends_over_a_bgra_target_as_vulkan_s_does() {
	use crate::gpu::mtl::fake_tests::{Made, fake, made};
	// The fake's functions are the real device's.
	if device().is_none() {
		return;
	}
	let device = fake(0);
	assert!(Pipeline::new(&device).is_ok());
	assert_eq!(
		made(),
		[
			Made::Library(SOURCE.encode_utf16().count()),
			Made::Function("quad_vertex".into()),
			Made::Function("quad_fragment".into()),
			// Color: source × alpha + target × (1 − alpha); alpha: source +
			// target × (1 − alpha).
			Made::Pipeline {
				format: 80,
				blends: true,
				factors: [4, 5, 1, 5],
			},
		]
	);
}

#[test]
fn source_metal_refuses_to_compile_is_refused_with_its_error_and_goes_no_further() {
	use crate::gpu::mtl::fake_tests::{LIBRARY, Made, fake, made};
	let device = fake(LIBRARY);
	assert_eq!(
		Pipeline::new(&device).err(),
		Some(Error::Shader("fake: use of undeclared identifier".into()))
	);
	assert_eq!(made(), [Made::Library(SOURCE.encode_utf16().count())]);
}

#[test]
fn a_function_or_pipeline_metal_refuses_is_refused_with_what_it_said() {
	use crate::gpu::mtl::fake_tests::{FUNCTION, PIPELINE, fake, made};
	let refusing = fake(FUNCTION);
	assert_eq!(
		Pipeline::new(&refusing).err(),
		Some(Error::Refused("shader function"))
	);
	// The vertex function was refused: the fragment one never asked for.
	assert_eq!(made().len(), 2);
	if device().is_none() {
		return;
	}
	let refusing = fake(PIPELINE);
	assert_eq!(
		Pipeline::new(&refusing).err(),
		Some(Error::Shader("fake: no such attachment".into()))
	);
}

#[test]
fn errors_metal_sets_are_released_once_described() {
	use crate::{
		gpu::mtl::fake_tests::{LIBRARY, fake},
		objc::probe_tests::Weak,
	};
	let device = fake(LIBRARY);
	let mut error: Raw = std::ptr::null_mut();
	let watched;
	{
		let _pool = crate::objc::Pool::new();
		let source = crate::objc::string("x").unwrap();
		// SAFETY: the fake's `newLibraryWithSource:options:error:` takes a
		// string, nil and room for an error, which it sets autoreleased.
		let made: Raw = unsafe {
			device.device.send(
				c"newLibraryWithSource:options:error:",
				(source.raw(), crate::objc::NIL, &mut error as *mut Raw),
			)
		};
		assert!(made.is_null() && !error.is_null());
		// SAFETY: a live error.
		let mut weak = unsafe { Weak::of(error) };
		assert!(weak.alive());
		watched = weak;
	}
	// Released with the pool, as the one `library` makes releases it.
	let mut watched = watched;
	assert!(!watched.alive());
	assert!(matches!(library(&device, "x"), Err(Error::Shader(_))));
}
