//! Tests of the quad pipeline: our MSL compiles on the real device with
//! both entry points, and source that does not compile is refused with
//! the compiler's own words.
use super::{Pipeline, SOURCE, function, library};
use crate::gpu::mtl::device::{Error, tests::device};

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
