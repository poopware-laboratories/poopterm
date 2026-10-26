//! Tests of calling Vulkan: which result codes are errors and how they
//! read, and the two-call enumeration as drivers may answer it.
use super::call::{Error, INCOMPLETE, check, enumerate, fixed_string};
use std::ffi::c_char;

#[test]
fn success_and_status_codes_pass_and_negative_codes_fail() {
	// VK_SUCCESS, VK_NOT_READY, VK_TIMEOUT, VK_INCOMPLETE, VK_SUBOPTIMAL_KHR.
	for code in [0, 1, 2, 5, 1_000_001_003] {
		assert_eq!(check(code), Ok(code));
	}
	for code in [-1, -4, -13, -1_000_001_004, i32::MIN] {
		assert_eq!(check(code), Err(Error::Failed(code)));
	}
}

#[test]
fn errors_say_what_failed() {
	let cases = [
		(Error::Failed(-4), "VkResult -4"),
		(Error::Failed(-1_000_001_004), "VkResult -1000001004"),
		(Error::Failed(5), "VkResult 5"),
		(
			Error::Missing(c"vkCreateInstance"),
			"Vulkan lacks vkCreateInstance",
		),
		(
			Error::TooOld(1 << 22 | 2 << 12 | 198),
			"Vulkan 1.2 is older than 1.3",
		),
		(Error::TooOld(1 << 22), "Vulkan 1.0 is older than 1.3"),
		(Error::Copy, "a Vulkan copy outside what it copies between"),
		(Error::Spirv, "a shader is not SPIR-V for Vulkan 1.3"),
		(Error::Draw, "a Vulkan draw of quads it cannot draw"),
		(Error::Busy, "a Vulkan quad set still in use"),
		(
			Error::Foreign,
			"a Vulkan device and surface of different instances",
		),
		(
			Error::Unshown,
			"the Vulkan device cannot show on the window's surface",
		),
	];
	for (error, text) in cases {
		assert!(error.to_string().contains(text), "{error:?}: {error}");
	}
	let none = Error::NoLoader("libvulkan.so.1: cannot open shared object file".into());
	assert_eq!(
		none.to_string(),
		"no Vulkan loader: libvulkan.so.1: cannot open shared object file"
	);
}

/// A driver's enumeration of `list`, which never changes.
fn driver(list: &[u32]) -> impl FnMut(&mut u32, *mut u32) -> i32 + '_ {
	move |count: &mut u32, items: *mut u32| {
		if items.is_null() {
			*count = list.len() as u32;
			return 0;
		}
		let written = list.len().min(*count as usize);
		for (i, &item) in list[..written].iter().enumerate() {
			// SAFETY: `enumerate` passes room for `count` items.
			unsafe { items.add(i).write(item) };
		}
		*count = written as u32;
		if written < list.len() { INCOMPLETE } else { 0 }
	}
}

#[test]
fn enumeration_asks_for_the_count_then_the_items() {
	assert_eq!(enumerate(0, driver(&[7, 8, 9])), Ok(vec![7, 8, 9]));
	assert_eq!(enumerate(0, driver(&[1])), Ok(vec![1]));
}

#[test]
fn enumeration_asks_again_while_the_list_grows_between_calls() {
	// The count is 1, but two items exist by the time they are asked for.
	let mut first = true;
	let growing = move |count: &mut u32, items: *mut u32| {
		if items.is_null() {
			*count = if first { 1 } else { 2 };
			first = false;
			return 0;
		}
		let have = if *count >= 2 {
			[4, 5].as_slice()
		} else {
			[4].as_slice()
		};
		for (i, &item) in have.iter().enumerate() {
			// SAFETY: room for `count` items, and `have` is no longer.
			unsafe { items.add(i).write(item) };
		}
		*count = have.len() as u32;
		if *count < 2 { INCOMPLETE } else { 0 }
	};
	assert_eq!(enumerate(0, growing), Ok(vec![4, 5]));
}

#[test]
fn enumeration_keeps_only_the_items_written_when_the_list_shrinks() {
	let shrinking = |count: &mut u32, items: *mut u32| {
		if items.is_null() {
			*count = 3;
		} else {
			// SAFETY: room for three items.
			unsafe { items.write(6) };
			*count = 1;
		}
		0
	};
	assert_eq!(enumerate(0, shrinking), Ok(vec![6]));
}

#[test]
fn enumeration_fails_with_the_driver_and_gives_up_on_endless_growth() {
	let failing = |_: &mut u32, _: *mut u32| -1;
	assert_eq!(enumerate(0, failing), Err(Error::Failed(-1)));
	let mut calls = 0;
	let failing_late = |count: &mut u32, items: *mut u32| {
		calls += 1;
		*count = 2;
		if items.is_null() { 0 } else { -2 }
	};
	assert_eq!(enumerate(0, failing_late), Err(Error::Failed(-2)));
	assert_eq!(calls, 2);
	// Each answer has one more item than the last count asked for.
	let mut calls = 0;
	let endless = |count: &mut u32, items: *mut u32| {
		calls += 1;
		if items.is_null() {
			*count = calls;
			return 0;
		}
		INCOMPLETE
	};
	assert_eq!(enumerate(0u32, endless), Err(Error::Failed(INCOMPLETE)));
	assert_eq!(calls, 16, "eight tries of two calls");
}

#[test]
fn an_empty_list_takes_one_call() {
	let mut calls = 0;
	let empty = |count: &mut u32, _: *mut u32| {
		calls += 1;
		*count = 0;
		0
	};
	assert_eq!(enumerate(0, empty), Ok(vec![]));
	assert_eq!(calls, 1);
}

/// `text` in a fixed-size field of `N` C characters, padded with `fill`.
fn field<const N: usize>(text: &[u8], fill: u8) -> [c_char; N] {
	let mut field = [fill as c_char; N];
	for (out, &byte) in field.iter_mut().zip(text) {
		*out = byte as c_char;
	}
	field
}

#[test]
fn fixed_strings_end_at_the_first_nul() {
	assert_eq!(
		fixed_string(&field::<256>(b"llvmpipe (LLVM 23.1.1)\0junk", 0)),
		"llvmpipe (LLVM 23.1.1)"
	);
	assert_eq!(
		fixed_string(&field::<256>(b"VK_KHR_swapchain", 0)),
		"VK_KHR_swapchain"
	);
	assert_eq!(fixed_string(&field::<8>(b"\0", b'x')), "");
	assert_eq!(fixed_string(&field::<8>(b"", 0)), "");
}

#[test]
fn fixed_strings_without_a_nul_stop_at_the_field_and_bad_bytes_are_replaced() {
	// A broken driver may fill the whole field; nothing past it is read.
	assert_eq!(fixed_string(&field::<4>(b"abcdef", b'z')), "abcd");
	assert_eq!(fixed_string(&field::<3>(b"", b'q')), "qqq");
	assert_eq!(fixed_string(&field::<6>(b"a\xffb\0", 0)), "a\u{fffd}b");
	assert_eq!(fixed_string(&field::<6>("é\0".as_bytes(), 0)), "é");
	assert_eq!(fixed_string(&[]), "");
}
