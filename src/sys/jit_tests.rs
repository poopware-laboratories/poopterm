//! Executable memory on this machine: code written into an arena runs.
//! A test process is not hardened, so the kernel lets it make its own
//! pages executable, as a debugger lets the app on iOS.
use super::*;

/// `mov x0, #42; ret`, and a function returning what x0 points to.
#[cfg(target_arch = "aarch64")]
const FORTY_TWO: [u32; 2] = [0xd280_0540, 0xd65f_03c0];
#[cfg(target_arch = "aarch64")]
const LOAD: [u32; 2] = [0xf940_0000, 0xd65f_03c0]; // ldr x0, [x0]; ret

#[cfg(target_arch = "aarch64")]
fn code(words: &[u32]) -> Vec<u8> {
	words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

/// Calls the function at `address` with `argument`.
#[cfg(target_arch = "aarch64")]
fn call(address: usize, argument: usize) -> usize {
	// SAFETY: the tests put a function of this type at `address`, made
	// executable by the arena.
	let function: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(address) };
	function(argument)
}

#[test]
fn the_code_signing_flags_and_version_are_read() {
	assert_ne!(code_signing_flags(), 0, "every Apple process has some");
	assert!(!debugged() || std::env::var_os("POOPTERM_UNDER_DEBUGGER").is_some());
	let version = os_version().unwrap();
	let major: u32 = version.split('.').next().unwrap().parse().unwrap();
	assert!(major >= 11, "{version}");
}

#[test]
fn pages_round_up_to_sixteen_kilobytes() {
	assert_eq!(pages(0), 0);
	assert_eq!(pages(1), 16384);
	assert_eq!(pages(16384), 16384);
	assert_eq!(pages(16385), 32768);
	assert_eq!(
		pages(usize::MAX),
		usize::MAX - usize::MAX % 16384,
		"saturates"
	);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn a_plain_arena_runs_code_and_keeps_data_writable() {
	let mut arena = Arena::plain(1 << 20);
	let image = arena.reserve(20000).unwrap();
	let mut bytes = code(&FORTY_TWO);
	bytes.resize(16384 + 8, 0);
	bytes[16384..].copy_from_slice(&7u64.to_le_bytes());
	image.seal(&bytes, &[(0, 16384)]).unwrap();
	assert_eq!(call(image.address(), 0), 42);
	let data = image.address() + 16384;
	// SAFETY: the data page of the image just sealed, writable.
	unsafe { *(data as *mut u64) = 9 };
	assert_eq!(arena.usage(), (32768, 1 << 20));
	assert!(!arena.is_prepared());
}

#[cfg(target_arch = "aarch64")]
#[test]
fn a_prepared_region_is_written_through_its_mirror() {
	// What the debugger does: an executable region nothing else uses.
	let len = 4 * 16384;
	// SAFETY: a new private anonymous mapping; checked below.
	let region = unsafe {
		libc::mmap(
			std::ptr::null_mut(),
			len,
			libc::PROT_READ | libc::PROT_WRITE,
			libc::MAP_PRIVATE | libc::MAP_ANON,
			-1,
			0,
		)
	};
	assert_ne!(region, libc::MAP_FAILED);
	protect(region as usize, len, libc::PROT_READ | libc::PROT_EXEC).unwrap();
	// SAFETY: the region is ours, executable and unused.
	let mut arena = unsafe { Arena::prepared(region as usize, len) }.unwrap();
	assert!(arena.is_prepared());
	let first = arena.reserve(2 * 16384).unwrap();
	let mut bytes = code(&FORTY_TWO);
	bytes.resize(16384 + 8, 0);
	bytes[16384..].copy_from_slice(&1234u64.to_le_bytes());
	first.seal(&bytes, &[(0, 16384)]).unwrap();
	assert_eq!(
		first.address(),
		region as usize,
		"the first piece of the region"
	);
	assert_eq!(call(first.address(), 0), 42);
	let data = first.address() + 16384;
	// SAFETY: the data page was replaced by a writable one.
	unsafe { *(data as *mut u64) += 1 };
	let second = arena.reserve(16384).unwrap();
	second.seal(&code(&LOAD), &[(0, 16384)]).unwrap();
	assert_eq!(second.address(), first.address() + 2 * 16384);
	assert_eq!(call(second.address(), data), 1235, "code reads the data");
	assert!(arena.reserve(2 * 16384).is_err(), "full");
	assert_eq!(arena.usage(), (3 * 16384, len));
}

#[test]
fn an_image_bigger_than_its_room_is_refused() {
	let mut arena = Arena::plain(1 << 20);
	let image = arena.reserve(16384).unwrap();
	assert!(image.seal(&vec![0; 16385], &[]).is_err());
	assert!(image.seal(&[], &[(16000, 1000)]).is_err());
	assert!(arena.reserve(0).is_err(), "nothing to load");
	assert!(
		Arena::plain(16384).reserve(16385).is_err(),
		"over the arena's size"
	);
}
