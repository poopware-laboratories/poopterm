//! Reading a Mach-O dynamic library to load it ourselves (`jit::loader`):
//! its segments, the libraries it links, the pointers it needs rebased or
//! bound (chained fixups, the format linkers write for iOS 13.4 and
//! later), its initializers and its symbols. Everything here is plain
//! data over bytes from a downloaded file, so every read is bounds-checked
//! and anything unexpected is an error, never a panic.

/// What went wrong reading a library: a message for the user.
pub(crate) type Error = String;

const MAGIC_64: u32 = 0xfeed_facf;
const FAT_MAGIC: u32 = 0xcafe_babe;
const MH_DYLIB: u32 = 6;
const MH_BUNDLE: u32 = 8;
const LC_REQ_DYLD: u32 = 0x8000_0000;
const LC_SEGMENT_64: u32 = 0x19;
const LC_SYMTAB: u32 = 0x2;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_LOAD_WEAK_DYLIB: u32 = 0x18 | LC_REQ_DYLD;
const LC_REEXPORT_DYLIB: u32 = 0x1f | LC_REQ_DYLD;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x23 | LC_REQ_DYLD;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x22 | LC_REQ_DYLD;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x34 | LC_REQ_DYLD;
const S_MOD_INIT_FUNC_POINTERS: u32 = 0x9;
const S_THREAD_LOCAL_REGULAR: u32 = 0x11;
const S_THREAD_LOCAL_INIT_FUNCTION_POINTERS: u32 = 0x15;
const S_INIT_FUNC_OFFSETS: u32 = 0x16;
const N_EXT: u8 = 0x01;
const N_TYPE: u8 = 0x0e;
const N_SECT: u8 = 0x0e;
const DYLD_CHAINED_PTR_64: u16 = 2;
const DYLD_CHAINED_PTR_64_OFFSET: u16 = 6;
const DYLD_CHAINED_PTR_START_NONE: u16 = 0xffff;
const DYLD_CHAINED_IMPORT: u32 = 1;
const DYLD_CHAINED_IMPORT_ADDEND: u32 = 2;
const DYLD_CHAINED_IMPORT_ADDEND64: u32 = 3;

/// The CPU this program runs on, as Mach-O names it.
#[cfg(target_arch = "aarch64")]
pub(crate) const HOST_CPU: u32 = 0x0100_000c;
#[cfg(target_arch = "x86_64")]
pub(crate) const HOST_CPU: u32 = 0x0100_0007;
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub(crate) const HOST_CPU: u32 = 0;

/// Most load commands, segments, fixups and imports one library may have:
/// far above any real one, and a bound on what a hostile file costs.
const MAX_COMMANDS: u32 = 4096;
const MAX_SPAN: u64 = 1 << 30;
const MAX_FIXUPS: usize = 1 << 24;
const MAX_IMPORTS: u32 = 1 << 20;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
	Some(u16::from_le_bytes(
		bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
	))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
	Some(u32::from_le_bytes(
		bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
	))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
	Some(u64::from_le_bytes(
		bytes.get(at..at.checked_add(8)?)?.try_into().ok()?,
	))
}

/// A NUL-terminated string at `at`, as UTF-8.
fn c_str(bytes: &[u8], at: usize) -> Option<&str> {
	let rest = bytes.get(at..)?;
	let end = rest.iter().position(|&byte| byte == 0)?;
	std::str::from_utf8(&rest[..end]).ok()
}

/// A fixed 16-byte name (segments, sections).
fn name16(bytes: &[u8], at: usize) -> Option<String> {
	let raw = bytes.get(at..at.checked_add(16)?)?;
	let end = raw.iter().position(|&byte| byte == 0).unwrap_or(16);
	Some(String::from_utf8_lossy(&raw[..end]).into_owned())
}

fn usize_of(value: u64) -> Option<usize> {
	usize::try_from(value).ok()
}

/// A segment, at its offset in the image (from the first segment's
/// address), with the bytes of the file it starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
	pub name: String,
	pub offset: u64,
	pub size: u64,
	pub file_offset: u64,
	pub file_size: u64,
	/// `VM_PROT_*`: 1 read, 2 write, 4 execute.
	pub protection: u32,
}

impl Segment {
	pub(crate) fn executable(&self) -> bool {
		self.protection & 4 != 0
	}
}

/// Where a library's initializers are: pointers to them (after fixups), or
/// 32-bit offsets from the image's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Initializers {
	Pointers { offset: u64, count: u64 },
	Offsets { offset: u64, count: u64 },
}

/// A parsed library: what loading it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Library {
	/// The address the first segment was linked at; fixups name addresses
	/// relative to it.
	pub base: u64,
	/// Bytes from the first segment's start to the last one's end.
	pub span: u64,
	pub segments: Vec<Segment>,
	/// The libraries it links, in order: bind ordinal 1 is the first.
	pub dylibs: Vec<String>,
	pub initializers: Vec<Initializers>,
	/// `LC_DYLD_CHAINED_FIXUPS`: where its data is in the file.
	chained: Option<(usize, usize)>,
	/// `LC_SYMTAB`: symbols, their count, strings and their size.
	symtab: Option<(usize, usize, usize, usize)>,
}

/// Reads the header and load commands of `bytes`.
pub(crate) fn parse(bytes: &[u8]) -> Result<Library, Error> {
	let bad = || "not a valid Mach-O library".to_owned();
	let magic = u32_at(bytes, 0).ok_or_else(bad)?;
	if magic == FAT_MAGIC.swap_bytes() || magic == FAT_MAGIC {
		return Err("a universal binary: build it for this device's CPU alone".to_owned());
	}
	if magic != MAGIC_64 {
		return Err(bad());
	}
	if u32_at(bytes, 4) != Some(HOST_CPU) {
		return Err("built for another CPU".to_owned());
	}
	let kind = u32_at(bytes, 12).ok_or_else(bad)?;
	if kind != MH_DYLIB && kind != MH_BUNDLE {
		return Err("not a dynamic library".to_owned());
	}
	let count = u32_at(bytes, 16).ok_or_else(bad)?;
	if count > MAX_COMMANDS {
		return Err(bad());
	}
	let mut library = Library {
		base: 0,
		span: 0,
		segments: Vec::new(),
		dylibs: Vec::new(),
		initializers: Vec::new(),
		chained: None,
		symtab: None,
	};
	let mut raw_segments = Vec::new();
	let mut at = 32usize;
	for _ in 0..count {
		let command = u32_at(bytes, at).ok_or_else(bad)?;
		let size = usize_of(u64::from(u32_at(bytes, at + 4).ok_or_else(bad)?)).ok_or_else(bad)?;
		if size < 8 || bytes.len() < at.saturating_add(size) {
			return Err(bad());
		}
		match command {
			LC_SEGMENT_64 => raw_segments.push(segment(bytes, at, &mut library)?),
			LC_SYMTAB => {
				let field = |n: usize| u32_at(bytes, at + 8 + 4 * n).and_then(|v| usize_of(v.into()));
				library.symtab = Some((
					field(0).ok_or_else(bad)?,
					field(1).ok_or_else(bad)?,
					field(2).ok_or_else(bad)?,
					field(3).ok_or_else(bad)?,
				));
			}
			LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB | LC_LOAD_UPWARD_DYLIB => {
				let name = u32_at(bytes, at + 8)
					.and_then(|v| usize_of(v.into()))
					.ok_or_else(bad)?;
				let text = bytes.get(at..at + size).ok_or_else(bad)?;
				library
					.dylibs
					.push(c_str(text, name).ok_or_else(bad)?.to_owned());
			}
			LC_DYLD_CHAINED_FIXUPS => {
				let offset = u32_at(bytes, at + 8)
					.and_then(|v| usize_of(v.into()))
					.ok_or_else(bad)?;
				let length = u32_at(bytes, at + 12)
					.and_then(|v| usize_of(v.into()))
					.ok_or_else(bad)?;
				if bytes.len() < offset.saturating_add(length) {
					return Err(bad());
				}
				library.chained = Some((offset, length));
			}
			LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
				return Err("uses the old dyld info format: link it for iOS 13.4 or later".to_owned());
			}
			_ => {}
		}
		at += size;
	}
	if raw_segments.is_empty() {
		return Err(bad());
	}
	let base = raw_segments
		.iter()
		.map(|segment| segment.offset)
		.min()
		.unwrap_or(0);
	let end = raw_segments
		.iter()
		.map(|segment| segment.offset.saturating_add(segment.size))
		.max()
		.unwrap_or(0);
	if end - base > MAX_SPAN {
		return Err("too big to load".to_owned());
	}
	library.base = base;
	library.span = end - base;
	for segment in &mut raw_segments {
		segment.offset -= base;
	}
	for initializers in &mut library.initializers {
		match initializers {
			Initializers::Pointers { offset, .. } | Initializers::Offsets { offset, .. } => {
				*offset = offset.checked_sub(base).ok_or_else(bad)?;
			}
		}
	}
	library.segments = raw_segments;
	Ok(library)
}

/// One `LC_SEGMENT_64` at `at`, its address still absolute; its sections
/// add initializers to `library`.
fn segment(bytes: &[u8], at: usize, library: &mut Library) -> Result<Segment, Error> {
	let bad = || "a damaged segment".to_owned();
	let segment = Segment {
		name: name16(bytes, at + 8).ok_or_else(bad)?,
		offset: u64_at(bytes, at + 24).ok_or_else(bad)?,
		size: u64_at(bytes, at + 32).ok_or_else(bad)?,
		file_offset: u64_at(bytes, at + 40).ok_or_else(bad)?,
		file_size: u64_at(bytes, at + 48).ok_or_else(bad)?,
		protection: u32_at(bytes, at + 60).ok_or_else(bad)?,
	};
	if segment.file_size > segment.size
		|| (bytes.len() as u64) < segment.file_offset.saturating_add(segment.file_size)
	{
		return Err(bad());
	}
	let sections = u32_at(bytes, at + 64).ok_or_else(bad)?;
	for index in 0..sections.min(MAX_COMMANDS) {
		let section = at + 72 + 80 * usize_of(index.into()).ok_or_else(bad)?;
		let address = u64_at(bytes, section + 32).ok_or_else(bad)?;
		let size = u64_at(bytes, section + 40).ok_or_else(bad)?;
		let kind = u32_at(bytes, section + 64).ok_or_else(bad)? & 0xff;
		match kind {
			S_MOD_INIT_FUNC_POINTERS => library.initializers.push(Initializers::Pointers {
				offset: address,
				count: size / 8,
			}),
			S_INIT_FUNC_OFFSETS => library.initializers.push(Initializers::Offsets {
				offset: address,
				count: size / 4,
			}),
			S_THREAD_LOCAL_REGULAR..=S_THREAD_LOCAL_INIT_FUNCTION_POINTERS => {
				return Err("uses thread-local variables, which only dyld can set up".to_owned());
			}
			_ => {}
		}
	}
	Ok(segment)
}

/// The image as it lies in memory: each segment's file bytes at its
/// offset, the rest zeroes.
pub(crate) fn layout(bytes: &[u8], library: &Library) -> Result<Vec<u8>, Error> {
	let span = usize_of(library.span).ok_or("too big to load")?;
	let mut image = vec![0u8; span];
	for segment in &library.segments {
		let (Some(start), Some(from), Some(length)) = (
			usize_of(segment.offset),
			usize_of(segment.file_offset),
			usize_of(segment.file_size),
		) else {
			return Err("a damaged segment".to_owned());
		};
		let source = bytes.get(from..from + length).ok_or("a damaged segment")?;
		image
			.get_mut(start..start + length)
			.ok_or("a damaged segment")?
			.copy_from_slice(source);
	}
	Ok(image)
}

/// A symbol a library takes from another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Import {
	pub name: String,
	/// 1 and up: the library of `dylibs` it comes from; 0 itself, -1 the
	/// main program, -2 any library (flat), -3 a weak definition.
	pub ordinal: i32,
	/// A missing weak import is null instead of an error.
	pub weak: bool,
	pub addend: i64,
}

/// A pointer in the image to set before it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fixup {
	/// To the image's base plus `target`, with `high` as the top byte.
	Rebase { at: u64, target: u64, high: u8 },
	/// To an imported symbol's address plus `addend`.
	Bind { at: u64, import: usize, addend: i64 },
}

/// Walks the chains of pointers in `image` (laid out by `layout`) and the
/// table of imports they bind to.
pub(crate) fn fixups(
	bytes: &[u8],
	library: &Library,
	image: &[u8],
) -> Result<(Vec<Import>, Vec<Fixup>), Error> {
	let Some((start, length)) = library.chained else {
		return Ok((Vec::new(), Vec::new()));
	};
	let data = bytes.get(start..start + length).ok_or("damaged fixups")?;
	let bad = || "damaged fixups".to_owned();
	let field = |n: usize| u32_at(data, 4 * n).ok_or_else(bad);
	let (starts, imports_at, symbols_at, import_count, format) =
		(field(1)?, field(2)?, field(3)?, field(4)?, field(5)?);
	if field(6)? != 0 {
		return Err("compressed symbol names are not supported".to_owned());
	}
	if import_count > MAX_IMPORTS {
		return Err(bad());
	}
	let imports = imports_of(data, imports_at, symbols_at, import_count, format)?;
	let starts = usize_of(starts.into()).ok_or_else(bad)?;
	let segment_count = u32_at(data, starts).ok_or_else(bad)?;
	let mut fixups = Vec::new();
	for index in 0..segment_count.min(MAX_COMMANDS) {
		let info = u32_at(data, starts + 4 + 4 * index as usize).ok_or_else(bad)?;
		if info == 0 {
			continue;
		}
		let info = starts + info as usize;
		let page_size = u64::from(u16_at(data, info + 4).ok_or_else(bad)?);
		let format = u16_at(data, info + 6).ok_or_else(bad)?;
		if format != DYLD_CHAINED_PTR_64 && format != DYLD_CHAINED_PTR_64_OFFSET {
			return Err(format!(
				"pointer format {format} is not supported (arm64e?)"
			));
		}
		let segment_offset = u64_at(data, info + 8).ok_or_else(bad)?;
		let pages = u16_at(data, info + 20).ok_or_else(bad)?;
		for page in 0..pages {
			let first = u16_at(data, info + 22 + 2 * usize::from(page)).ok_or_else(bad)?;
			if first == DYLD_CHAINED_PTR_START_NONE {
				continue;
			}
			let mut at = segment_offset + u64::from(page) * page_size + u64::from(first);
			loop {
				let raw = u64_at(image, usize_of(at).ok_or_else(bad)?).ok_or_else(bad)?;
				fixups.push(decode(raw, at, format, library.base, imports.len())?);
				if fixups.len() > MAX_FIXUPS {
					return Err(bad());
				}
				let next = (raw >> 51) & 0xfff;
				if next == 0 {
					break;
				}
				at += next * 4;
			}
		}
	}
	Ok((imports, fixups))
}

/// One pointer of a chain.
fn decode(raw: u64, at: u64, format: u16, base: u64, imports: usize) -> Result<Fixup, Error> {
	if raw >> 63 != 0 {
		let import = (raw & 0xff_ffff) as usize;
		if import >= imports {
			return Err("a fixup binds to a missing import".to_owned());
		}
		let addend = ((raw >> 24) & 0xff) as i64;
		return Ok(Fixup::Bind { at, import, addend });
	}
	let target = raw & 0xf_ffff_ffff;
	let target = match format {
		DYLD_CHAINED_PTR_64 => target
			.checked_sub(base)
			.ok_or("a rebase out of the image")?,
		_ => target,
	};
	let high = ((raw >> 36) & 0xff) as u8;
	Ok(Fixup::Rebase { at, target, high })
}

/// The import table, in any of its three formats.
fn imports_of(
	data: &[u8],
	at: u32,
	symbols: u32,
	count: u32,
	format: u32,
) -> Result<Vec<Import>, Error> {
	let bad = || "damaged imports".to_owned();
	let (at, symbols) = (at as usize, symbols as usize);
	let mut imports = Vec::with_capacity(count as usize);
	for index in 0..count as usize {
		let (ordinal, weak, name, addend) = match format {
			DYLD_CHAINED_IMPORT | DYLD_CHAINED_IMPORT_ADDEND => {
				let stride = if format == DYLD_CHAINED_IMPORT { 4 } else { 8 };
				let raw = u32_at(data, at + stride * index).ok_or_else(bad)?;
				let addend = match format {
					DYLD_CHAINED_IMPORT => 0,
					_ => i64::from(u32_at(data, at + stride * index + 4).ok_or_else(bad)? as i32),
				};
				(
					i32::from((raw & 0xff) as u8 as i8),
					raw & 0x100 != 0,
					raw >> 9,
					addend,
				)
			}
			DYLD_CHAINED_IMPORT_ADDEND64 => {
				let raw = u64_at(data, at + 16 * index).ok_or_else(bad)?;
				let addend = u64_at(data, at + 16 * index + 8).ok_or_else(bad)? as i64;
				(
					i32::from((raw & 0xffff) as u16 as i16),
					raw & 0x1_0000 != 0,
					(raw >> 32) as u32,
					addend,
				)
			}
			_ => return Err(format!("import format {format} is not supported")),
		};
		let name = c_str(data, symbols + name as usize).ok_or_else(bad)?;
		imports.push(Import {
			name: name.to_owned(),
			ordinal,
			weak,
			addend,
		});
	}
	Ok(imports)
}

/// Where the library defines the exported symbol `name` (its C name with
/// the leading underscore), as an offset in the image.
pub(crate) fn symbol(bytes: &[u8], library: &Library, name: &str) -> Option<u64> {
	let (symbols, count, strings, size) = library.symtab?;
	let names = bytes.get(strings..strings.checked_add(size)?)?;
	(0..count.min(1 << 22)).find_map(|index| {
		let entry = symbols.checked_add(index.checked_mul(16)?)?;
		let kind = *bytes.get(entry + 4)?;
		if kind & N_EXT == 0 || kind & N_TYPE != N_SECT {
			return None;
		}
		let text = c_str(names, usize_of(u32_at(bytes, entry)?.into())?)?;
		(text == name).then(|| u64_at(bytes, entry + 8)?.checked_sub(library.base))?
	})
}

/// Sets every fixup in `image`, loaded at `address`: rebases point into
/// it, binds at `resolved[import]` (0 for a missing weak one).
pub(crate) fn apply(
	image: &mut [u8],
	fixups: &[Fixup],
	address: u64,
	resolved: &[u64],
) -> Result<(), Error> {
	for fixup in fixups {
		let (at, value) = match *fixup {
			Fixup::Rebase { at, target, high } => {
				(at, address.wrapping_add(target) | u64::from(high) << 56)
			}
			Fixup::Bind { at, import, addend } => {
				let symbol = *resolved
					.get(import)
					.ok_or("a fixup binds to a missing import")?;
				(at, symbol.wrapping_add(addend as u64))
			}
		};
		let at = usize_of(at).ok_or("a fixup out of the image")?;
		image
			.get_mut(at..at + 8)
			.ok_or("a fixup out of the image")?
			.copy_from_slice(&value.to_le_bytes());
	}
	Ok(())
}

#[cfg(test)]
#[path = "macho_tests.rs"]
pub(crate) mod tests;
