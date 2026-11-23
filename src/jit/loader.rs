//! Loading a command library ourselves, where dyld would refuse it for
//! being unsigned: its segments go into an `Arena` (memory a debugger made
//! executable on iOS), its imports are looked up in the libraries it links
//! (the system's, already in memory), its pointers are fixed up for where
//! it landed, and its initializers run. A loaded library stays: its data
//! is put back as it was loaded before each run, so every run starts with
//! fresh globals, as reopening a library with dyld gives.
use std::{
	ffi::{CString, c_void},
	path::Path,
};

use super::macho::{self, Fixup, Initializers, Library};
use crate::sys::{
	dl,
	jit::{Arena, Image},
};

/// Most bytes of a library file we read.
const MAX_FILE: u64 = 512 << 20;

/// A library in memory, ready to run.
pub(crate) struct Loaded {
	image: Image,
	/// Its `poopterm_entry`.
	entry: usize,
	/// Its writable segments as loaded (offset, bytes), put back before
	/// each run.
	data: Vec<(usize, Vec<u8>)>,
	/// Its initializers' addresses, run before each run.
	initializers: Vec<usize>,
}

impl std::fmt::Debug for Loaded {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Loaded")
			.field("address", &self.image.address())
			.finish()
	}
}

/// What resolving an import gives: its address, or none.
pub(crate) type Resolve<'a> = dyn FnMut(&macho::Import) -> Option<usize> + 'a;

/// Reads `path` and loads it into `arena`.
pub(crate) fn load(path: &Path, arena: &mut Arena) -> Result<Loaded, String> {
	let size = std::fs::metadata(path)
		.map_err(|error| error.to_string())?
		.len();
	if size > MAX_FILE {
		return Err("too big to load".to_owned());
	}
	let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
	let library = macho::parse(&bytes)?;
	let mut linked = Vec::new();
	for dylib in &library.dylibs {
		let name = CString::new(dylib.as_str()).map_err(|_| "a library name with a NUL byte")?;
		// A library it links that is not on this system (one of its own,
		// beside it) cannot be loaded this way yet.
		linked.push(dl::Library::open(&name).map_err(|error| format!("needs {dylib}: {error}"))?);
	}
	let mut resolve = |import: &macho::Import| -> Option<usize> {
		let name = CString::new(import.name.strip_prefix('_').unwrap_or(&import.name)).ok()?;
		let found = match usize::try_from(import.ordinal) {
			Ok(index) if index >= 1 => linked.get(index - 1)?.symbol(&name),
			_ => dl::any_symbol(&name),
		};
		found.map(|symbol| symbol.as_ptr() as usize)
	};
	let placed = place(&bytes, &library, arena, &mut resolve);
	// The libraries it links stay open for the app's life, as the code
	// loaded stays: never closed.
	linked.into_iter().for_each(std::mem::forget);
	placed
}

/// Lays out, fixes up and seals a parsed library, resolving imports with
/// `resolve` (imports the library defines itself are found in it).
pub(crate) fn place(
	bytes: &[u8],
	library: &Library,
	arena: &mut Arena,
	resolve: &mut Resolve,
) -> Result<Loaded, String> {
	let mut image = macho::layout(bytes, library)?;
	let (imports, fixups) = macho::fixups(bytes, library, &image)?;
	let span = usize::try_from(library.span).map_err(|_| "too big to load")?;
	let room = arena.reserve(span).map_err(|error| error.to_string())?;
	let address = room.address();
	let mut resolved = Vec::with_capacity(imports.len());
	for import in &imports {
		let own = (import.ordinal == 0)
			.then(|| macho::symbol(bytes, library, &import.name))
			.flatten()
			.map(|offset| address + offset as usize);
		let found = match own {
			Some(address) => Some(address),
			None => resolve(import),
		};
		let base = match (found, import.weak) {
			(Some(address), _) => address as u64,
			(None, true) => 0,
			(None, false) => return Err(format!("needs {}, which this system lacks", import.name)),
		};
		resolved.push(if base == 0 {
			0
		} else {
			base.wrapping_add(import.addend as u64)
		});
	}
	macho::apply(&mut image, &fixups, address as u64, &resolved)?;
	let code: Vec<(usize, usize)> = library
		.segments
		.iter()
		.filter(|segment| segment.executable())
		.map(|segment| (segment.offset as usize, segment.size as usize))
		.collect();
	let data: Vec<(usize, Vec<u8>)> = library
		.segments
		.iter()
		.filter(|segment| !segment.executable() && segment.protection & 2 != 0)
		.map(|segment| {
			let (at, size) = (segment.offset as usize, segment.size as usize);
			(at, image[at..at + size].to_vec())
		})
		.collect();
	let initializers = initializers(&image, library, address, &fixups)?;
	let entry = macho::symbol(bytes, library, "_poopterm_entry")
		.ok_or("not a command library (no poopterm_entry)")?;
	room
		.seal(&image, &code)
		.map_err(|error| error.to_string())?;
	Ok(Loaded {
		entry: address + entry as usize,
		image: room,
		data,
		initializers,
	})
}

/// The addresses of the library's initializers, from the fixed-up image.
fn initializers(
	image: &[u8],
	library: &Library,
	address: usize,
	_fixups: &[Fixup],
) -> Result<Vec<usize>, String> {
	let mut found = Vec::new();
	let read = |at: usize, len: usize| {
		image
			.get(at..at + len)
			.ok_or("initializers out of the image")
	};
	for initializers in &library.initializers {
		match *initializers {
			Initializers::Pointers { offset, count } => {
				for index in 0..count as usize {
					let at = offset as usize + 8 * index;
					let pointer = u64::from_le_bytes(read(at, 8)?.try_into().map_err(|_| "initializers")?);
					found.push(pointer as usize);
				}
			}
			Initializers::Offsets { offset, count } => {
				for index in 0..count as usize {
					let at = offset as usize + 4 * index;
					let delta = u32::from_le_bytes(read(at, 4)?.try_into().map_err(|_| "initializers")?);
					found.push(address + delta as usize);
				}
			}
		}
	}
	let inside = |pointer: &usize| (address..address + library.span as usize).contains(pointer);
	if !found.iter().all(inside) {
		return Err("an initializer outside the library".to_owned());
	}
	Ok(found)
}

impl Loaded {
	/// Where it runs.
	#[cfg(test)]
	pub(crate) fn address(&self) -> usize {
		self.image.address()
	}

	/// Its `poopterm_entry`, after putting its data back as loaded and
	/// running its initializers: ready for a fresh run.
	pub(crate) fn fresh_entry(&self) -> *mut c_void {
		for (at, bytes) in &self.data {
			// SAFETY: the writable segment at this offset of our image, the
			// same length as when it was copied.
			unsafe {
				std::ptr::copy_nonoverlapping(
					bytes.as_ptr(),
					(self.image.address() + at) as *mut u8,
					bytes.len(),
				);
			}
		}
		for &initializer in &self.initializers {
			// SAFETY: an initializer of the library, inside its code (checked
			// when loaded), of the type C constructors have.
			let run: extern "C" fn() = unsafe { std::mem::transmute(initializer) };
			run();
		}
		self.entry as *mut c_void
	}
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
