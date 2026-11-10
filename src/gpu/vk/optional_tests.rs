//! Tests of the optional extensions: found by name among a device's,
//! turned off by `POOPTERM_VULKAN_DISABLE`, and enabled only when found.
use super::optional::{INCREMENTAL_PRESENT, Optional};

fn names(list: &[&str]) -> Vec<String> {
	list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn an_extension_the_device_offers_is_used() {
	let offered = names(&["VK_KHR_swapchain", "VK_KHR_incremental_present"]);
	let optional = Optional::found(&offered, None);
	assert!(optional.incremental_present);
	assert_eq!(optional.extensions(), [INCREMENTAL_PRESENT]);
}

#[test]
fn an_extension_the_device_lacks_is_not_asked_for() {
	let optional = Optional::found(&names(&["VK_KHR_swapchain"]), None);
	assert_eq!(optional, Optional::default());
	assert!(optional.extensions().is_empty());
}

#[test]
fn names_must_match_whole() {
	// A longer name sharing the prefix is another extension.
	let offered = names(&["VK_KHR_incremental_present2", "VK_KHR_incremental"]);
	assert!(!Optional::found(&offered, None).incremental_present);
}

#[test]
fn the_variable_turns_offered_extensions_off_by_short_name() {
	let offered = names(&["VK_KHR_incremental_present"]);
	for disable in [
		"incremental_present",
		"present_wait,incremental_present",
		" incremental_present , other",
		"all",
	] {
		let optional = Optional::found(&offered, Some(disable));
		assert!(!optional.incremental_present, "{disable:?}");
		assert!(optional.extensions().is_empty(), "{disable:?}");
	}
}

#[test]
fn other_words_in_the_variable_leave_extensions_on() {
	let offered = names(&["VK_KHR_incremental_present"]);
	for disable in [
		"",
		",",
		"incremental",
		"INCREMENTAL_PRESENT",
		"present_wait",
	] {
		let optional = Optional::found(&offered, Some(disable));
		assert!(optional.incremental_present, "{disable:?}");
	}
}

#[test]
fn the_names_are_the_specifications() {
	assert_eq!(INCREMENTAL_PRESENT, c"VK_KHR_incremental_present");
}

#[test]
fn the_enabled_ones_are_listed_by_short_name() {
	let on = Optional {
		incremental_present: true,
	};
	assert_eq!(on.names(), ["incremental_present"]);
	assert!(Optional::default().names().is_empty());
}
