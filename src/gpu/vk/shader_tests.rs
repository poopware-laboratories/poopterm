//! Tests of the SPIR-V the renderer embeds: reading its words in either
//! byte order, refusing what is not SPIR-V for Vulkan 1.3, the entry
//! points and bindings the checked-in shaders declare, and their modules
//! on the fake driver.
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	driver_tests::{finish, with},
	shader::{QUAD_FRAGMENT, QUAD_VERTEX, ShaderModule, words},
};
use std::collections::HashMap;

const MAGIC: u32 = 0x0723_0203;
/// SPIR-V 1.6, the newest Vulkan 1.3 takes.
const V1_6: u32 = 0x0001_0600;

/// A header of `version` and an empty module after it, as bytes in the
/// order `to_bytes` gives.
fn module(version: u32, to_bytes: fn(u32) -> [u8; 4]) -> Vec<u8> {
	[MAGIC, version, 0x000d_0000, 1, 0]
		.into_iter()
		.flat_map(to_bytes)
		.collect()
}

#[test]
fn words_are_read_in_the_order_the_magic_number_shows() {
	let header = [MAGIC, V1_6, 0x000d_0000, 1, 0];
	assert_eq!(words(&module(V1_6, u32::to_le_bytes)), Ok(header.to_vec()));
	// A module written on a machine of the other byte order.
	assert_eq!(words(&module(V1_6, u32::to_be_bytes)), Ok(header.to_vec()));
	assert_eq!(
		words(&module(0x0001_0000, u32::to_le_bytes)).unwrap()[1],
		0x0001_0000
	);
}

#[test]
fn what_is_not_spirv_for_vulkan_1_3_is_refused() {
	let mut misaligned = module(V1_6, u32::to_le_bytes);
	misaligned.push(0);
	let mut wrong_magic = module(V1_6, u32::to_le_bytes);
	wrong_magic[0] ^= 1;
	let refused = [
		Vec::new(),
		misaligned,
		module(V1_6, u32::to_le_bytes)[..16].to_vec(),
		wrong_magic,
		module(0x0001_0700, u32::to_le_bytes),
		module(0x0002_0000, u32::to_le_bytes),
		module(0x0000_0600, u32::to_le_bytes),
	];
	for bytes in refused {
		assert_eq!(words(&bytes), Err(Error::Spirv), "{bytes:?}");
	}
}

/// The instructions of `module` after its header: each opcode and its
/// operands.
fn instructions(module: &[u32]) -> Vec<(u32, &[u32])> {
	let mut rest = &module[5..];
	let mut found = Vec::new();
	while let Some(&first) = rest.first() {
		let count = (first >> 16) as usize;
		assert!(count > 0 && count <= rest.len(), "a truncated instruction");
		found.push((first & 0xffff, &rest[1..count]));
		rest = &rest[count..];
	}
	found
}

/// The literal string that starts `operands`.
fn string(operands: &[u32]) -> String {
	let bytes: Vec<u8> = operands
		.iter()
		.flat_map(|word| word.to_le_bytes())
		.collect();
	let end = bytes.iter().position(|&byte| byte == 0).unwrap();
	String::from_utf8(bytes[..end].to_vec()).unwrap()
}

/// `OpEntryPoint` and `OpDecorate`, and the decorations `DescriptorSet` and
/// `Binding`.
const ENTRY_POINT: u32 = 15;
/// `OpCapability`, and the capability `Shader`.
const CAPABILITY: u32 = 17;
const SHADER: u32 = 1;
const DECORATE: u32 = 71;
const DESCRIPTOR_SET: u32 = 34;
const BINDING: u32 = 33;

/// The execution model and name of each entry point, and the set and
/// binding of each resource.
type Interface = (Vec<(u32, String)>, Vec<(u32, u32)>);

/// The execution model and name of each entry point of `module`, and the
/// set and binding of each resource it declares.
fn interface(module: &[u32]) -> Interface {
	let mut entries = Vec::new();
	let mut sets = Vec::new();
	let mut bindings = Vec::new();
	for (opcode, operands) in instructions(module) {
		match (opcode, operands) {
			(ENTRY_POINT, [model, _, name @ ..]) => entries.push((*model, string(name))),
			(DECORATE, &[id, DESCRIPTOR_SET, set]) => sets.push((id, set)),
			(DECORATE, &[id, BINDING, binding]) => bindings.push((id, binding)),
			_ => {}
		}
	}
	let mut resources: Vec<_> = (sets.iter())
		.map(|&(id, set)| {
			let binding = bindings.iter().find(|&&(bound, _)| bound == id);
			(set, binding.expect("a binding for each set").1)
		})
		.collect();
	resources.sort();
	(entries, resources)
}

#[test]
fn the_shaders_are_spirv_1_6_with_the_entry_points_and_bindings_the_pipeline_expects() {
	let vertex = words(QUAD_VERTEX).unwrap();
	let fragment = words(QUAD_FRAGMENT).unwrap();
	assert_eq!(vertex[1], V1_6);
	assert_eq!(fragment[1], V1_6);
	// Execution models 0, vertex, and 4, fragment. The vertex shader reads
	// the quads; the fragment shader reads them and samples the atlas.
	assert_eq!(interface(&vertex), (vec![(0, "main".into())], vec![(0, 0)]));
	assert_eq!(
		interface(&fragment),
		(vec![(4, "main".into())], vec![(0, 0), (0, 1)])
	);
}

#[test]
fn the_shaders_need_no_capability_beyond_what_every_device_has() {
	// `OpCapability Shader` alone: the device turns on no shader features,
	// and `discard` would need `DemoteToHelperInvocation` under Vulkan 1.3.
	for shader in [QUAD_VERTEX, QUAD_FRAGMENT] {
		let module = words(shader).unwrap();
		let capabilities: Vec<_> = (instructions(&module).into_iter())
			.filter(|&(opcode, _)| opcode == CAPABILITY)
			.map(|(_, operands)| operands.to_vec())
			.collect();
		assert_eq!(capabilities, [[SHADER]]);
	}
}

#[test]
fn the_interface_reader_finds_what_a_module_declares() {
	// An entry point "main" of the fragment model, and a resource with its
	// binding declared before its set.
	let name = u32::from_le_bytes(*b"main");
	let module = [
		MAGIC,
		V1_6,
		0,
		9,
		0,
		5 << 16 | ENTRY_POINT,
		4,
		1,
		name,
		0,
		4 << 16 | DECORATE,
		7,
		BINDING,
		3,
		4 << 16 | DECORATE,
		7,
		DESCRIPTOR_SET,
		2,
	];
	assert_eq!(interface(&module), (vec![(4, "main".into())], vec![(2, 3)]));
}

#[test]
fn a_module_is_made_from_the_words_and_destroyed_with_its_owner() {
	let device = device(&INTEL, |_| {});
	let code = words(QUAD_VERTEX).unwrap();
	let module = ShaderModule::new(&device, &code).unwrap();
	assert_eq!(module.handle(), 0x60_0001);
	drop(module);
	with(|driver| driver.fails.push((c"vkCreateShaderModule", -2)));
	let failed = ShaderModule::new(&device, &code);
	assert_eq!(failed.err(), Some(Error::Failed(-2)));
	drop(device);
	let driver = finish();
	assert_eq!(driver.pipeline_objects.modules, [code]);
	assert_eq!(driver.released, ["module 0x600001"]);
}

/// The types, variables and decorations of a module, by id.
#[derive(Default)]
struct Reflection {
	/// Each type's opcode and operands after its result id.
	types: HashMap<u32, (u32, Vec<u32>)>,
	/// Each global variable: id, storage class and the type it points to.
	variables: Vec<(u32, u32, u32)>,
	/// The decorations of each id, and of each member of each structure.
	decorations: HashMap<u32, Vec<Vec<u32>>>,
	member_decorations: HashMap<(u32, u32), Vec<Vec<u32>>>,
}

/// `OpTypeInt`, `OpTypeFloat`, `OpTypeVector`, `OpTypeImage`,
/// `OpTypeSampledImage`, `OpTypeRuntimeArray`, `OpTypeStruct`,
/// `OpTypePointer`, `OpVariable` and `OpMemberDecorate`.
const TYPE_INT: u32 = 21;
const TYPE_FLOAT: u32 = 22;
const TYPE_VECTOR: u32 = 23;
const TYPE_IMAGE: u32 = 25;
const TYPE_SAMPLED_IMAGE: u32 = 27;
const TYPE_RUNTIME_ARRAY: u32 = 29;
const TYPE_STRUCT: u32 = 30;
const TYPE_POINTER: u32 = 32;
const VARIABLE: u32 = 59;
const MEMBER_DECORATE: u32 = 72;
/// The decorations `Flat`, `BuiltIn`, `Location`, `ArrayStride` and
/// `Offset`.
const FLAT: u32 = 14;
const BUILT_IN: u32 = 11;
const LOCATION: u32 = 30;
const ARRAY_STRIDE: u32 = 6;
const OFFSET: u32 = 35;
/// The storage class of variables inside functions.
const FUNCTION: u32 = 7;

impl Reflection {
	fn new(module: &[u32]) -> Self {
		let mut found = Self::default();
		for (opcode, operands) in instructions(module) {
			match (opcode, operands) {
				(TYPE_INT..=TYPE_POINTER, [id, rest @ ..]) => {
					found.types.insert(*id, (opcode, rest.to_vec()));
				}
				(VARIABLE, &[pointer, id, class, ..]) if class != FUNCTION => {
					let pointee = found.types[&pointer].1[1];
					found.variables.push((id, class, pointee));
				}
				(DECORATE, [id, decoration @ ..]) => {
					found
						.decorations
						.entry(*id)
						.or_default()
						.push(decoration.to_vec());
				}
				(MEMBER_DECORATE, [id, member, decoration @ ..]) => {
					let decorations = found.member_decorations.entry((*id, *member));
					decorations.or_default().push(decoration.to_vec());
				}
				_ => {}
			}
		}
		found
	}

	/// The first operand of decoration `kind` of `id`, if it has one.
	fn decoration(&self, id: u32, kind: u32) -> Option<u32> {
		let all = self.decorations.get(&id)?;
		all
			.iter()
			.find(|decoration| decoration[0] == kind)
			.map(|decoration| decoration.get(1).copied().unwrap_or(0))
	}

	fn member_offset(&self, id: u32, member: u32) -> Option<u32> {
		let all = self.member_decorations.get(&(id, member))?;
		all
			.iter()
			.find(|decoration| decoration[0] == OFFSET)
			.map(|decoration| decoration[1])
	}

	fn is_built_in(&self, id: u32) -> bool {
		self.decoration(id, BUILT_IN).is_some()
			|| (self.member_decorations.iter())
				.any(|(&(of, _), all)| of == id && all.iter().any(|decoration| decoration[0] == BUILT_IN))
	}

	/// Type `id`, written out: `i32x4` for an `ivec4`, `{a@0, b@16}` for a
	/// structure with its members' offsets, `[t; stride]` for a runtime
	/// array.
	fn type_name(&self, id: u32) -> String {
		match &self.types[&id] {
			(TYPE_INT, operands) => format!("{}{}", ["u", "i"][operands[1] as usize], operands[0]),
			(TYPE_FLOAT, operands) => format!("f{}", operands[0]),
			(TYPE_VECTOR, operands) => format!("{}x{}", self.type_name(operands[0]), operands[1]),
			(TYPE_IMAGE, operands) => {
				let [sampled, dim, depth, arrayed, multisampled, usage, ..] = operands[..] else {
					panic!("a short image type")
				};
				let shape =
					format!("dim {dim} depth {depth} arrayed {arrayed} ms {multisampled} usage {usage}");
				format!("image of {} ({shape})", self.type_name(sampled))
			}
			(TYPE_SAMPLED_IMAGE, operands) => format!("sampled {}", self.type_name(operands[0])),
			(TYPE_RUNTIME_ARRAY, operands) => {
				let stride = self.decoration(id, ARRAY_STRIDE).expect("a stride");
				format!("[{}; {stride}]", self.type_name(operands[0]))
			}
			(TYPE_STRUCT, members) => {
				let members: Vec<_> = (members.iter().zip(0..))
					.map(|(&member, index)| match self.member_offset(id, index) {
						Some(offset) => format!("{}@{offset}", self.type_name(member)),
						None => self.type_name(member),
					})
					.collect();
				format!("{{{}}}", members.join(", "))
			}
			(opcode, _) => format!("type {opcode}"),
		}
	}

	/// Every global variable but the built-ins, as its storage class, its
	/// location or set and binding, and its type, sorted.
	fn globals(&self) -> Vec<String> {
		let mut found: Vec<_> = (self.variables.iter())
			.filter(|&&(id, _, pointee)| !self.is_built_in(id) && !self.is_built_in(pointee))
			.map(|&(id, class, pointee)| {
				let class = match class {
					0 => "uniform constant",
					1 => "input",
					3 => "output",
					9 => "push constant",
					12 => "storage buffer",
					_ => "other",
				};
				let place = match (
					self.decoration(id, LOCATION),
					self.decoration(id, DESCRIPTOR_SET),
				) {
					(Some(location), _) => format!(" location {location}"),
					(None, Some(set)) => format!(" {set}.{}", self.decoration(id, BINDING).unwrap()),
					(None, None) => String::new(),
				};
				let flat = if self.decoration(id, FLAT).is_some() {
					" flat"
				} else {
					""
				};
				format!("{class}{place}{flat}: {}", self.type_name(pointee))
			})
			.collect();
		found.sort();
		found
	}
}

/// The quads both shaders read: each rectangle, clip and source a 16-byte
/// vector, 48 bytes a quad, read-only. The renderer writes the buffer in
/// this layout.
const QUADS: &str = "storage buffer 0.0: {[{i32x4@0, i32x4@16, u32x4@32}; 48]@0}";

#[test]
fn the_shaders_declare_what_the_pipeline_layout_and_the_renderer_provide() {
	let vertex = Reflection::new(&words(QUAD_VERTEX).unwrap()).globals();
	let fragment = Reflection::new(&words(QUAD_FRAGMENT).unwrap()).globals();
	// The push constant block is the 8 bytes the layout's range covers: the
	// target's size, two floats.
	assert_eq!(
		vertex,
		[
			"output location 0 flat: u32",
			"push constant: {f32x2@0}",
			QUADS,
		]
	);
	// Binding 1 is the atlas: a combined image sampler over a sampled 2D
	// array of float texels, not multisampled, as the set layout declares.
	let atlas = "sampled image of f32 (dim 1 depth 0 arrayed 1 ms 0 usage 1)";
	assert_eq!(
		fragment,
		[
			"input location 0 flat: u32".to_string(),
			"output location 0: f32x4".into(),
			QUADS.into(),
			format!("uniform constant 0.1: {atlas}"),
		]
	);
	// What the vertex shader hands on is what the fragment shader reads.
	assert_eq!(vertex[0].replacen("output", "input", 1), fragment[0]);
}

#[test]
fn the_reflection_skips_built_ins_and_names_what_it_does_not_know() {
	// A built-in input, a structure of built-in members, and an input of an
	// array of fixed length, which `type_name` has no name for.
	let module = [
		MAGIC,
		V1_6,
		0,
		20,
		0,
		3 << 16 | TYPE_FLOAT,
		1,
		32,
		4 << 16 | TYPE_POINTER,
		2,
		1,
		1,
		4 << 16 | VARIABLE,
		2,
		3,
		1,
		4 << 16 | DECORATE,
		3,
		BUILT_IN,
		15,
		3 << 16 | TYPE_STRUCT,
		4,
		1,
		4 << 16 | TYPE_POINTER,
		5,
		3,
		4,
		4 << 16 | VARIABLE,
		5,
		6,
		3,
		5 << 16 | MEMBER_DECORATE,
		4,
		0,
		BUILT_IN,
		0,
		4 << 16 | 28,
		7,
		1,
		1,
		4 << 16 | TYPE_POINTER,
		8,
		1,
		7,
		4 << 16 | VARIABLE,
		8,
		9,
		1,
	];
	assert_eq!(Reflection::new(&module).globals(), ["input: type 28"]);
}
