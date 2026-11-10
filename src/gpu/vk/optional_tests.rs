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
	assert!(optional.incremental_present.is_some());
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
	assert!(
		Optional::found(&offered, None)
			.incremental_present
			.is_none()
	);
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
		assert!(optional.incremental_present.is_none(), "{disable:?}");
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
		assert!(optional.incremental_present.is_some(), "{disable:?}");
	}
}

#[test]
fn the_names_are_the_specifications() {
	assert_eq!(INCREMENTAL_PRESENT, c"VK_KHR_incremental_present");
}

#[test]
fn the_enabled_ones_are_listed_by_short_name() {
	let on = Optional {
		incremental_present: Some(INCREMENTAL_PRESENT),
		..Optional::default()
	};
	assert_eq!(on.names(), ["incremental_present"]);
	assert!(Optional::default().names().is_empty());
}

#[test]
fn found_agrees_with_a_model_for_random_offers_and_variables() {
	let offers = [
		"VK_KHR_swapchain",
		"VK_KHR_incremental_present",
		"VK_KHR_incremental_present2",
		"VK_KHR_incremental",
		"vk_khr_incremental_present",
		"VK_KHR_present_mode_fifo_latest_ready",
		"VK_EXT_present_mode_fifo_latest_ready",
		"VK_EXT_present_mode_fifo",
	];
	let words = [
		"incremental_present",
		"all",
		"ALL",
		"present_wait",
		"",
		" ",
		"incremental",
		"fifo_latest_ready",
	];
	let mut state = 0x510e_527f_ade6_82d1_u64;
	let mut below = |n: usize| {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		(state % n as u64) as usize
	};
	for case in 0..3000 {
		let offered: Vec<String> = (0..below(5)).map(|_| offers[below(8)].into()).collect();
		let disable = (below(3) > 0).then(|| {
			let chosen: Vec<_> = (0..below(4)).map(|_| words[below(8)]).collect();
			chosen.join(if below(2) == 0 { "," } else { " , " })
		});
		let off = |short: &str| {
			disable.as_deref().is_some_and(|value| {
				value
					.split(',')
					.any(|word| [short, "all"].contains(&word.trim()))
			})
		};
		let has = |name: &str| offered.iter().any(|offer| offer == name);
		let on = !off("incremental_present") && has("VK_KHR_incremental_present");
		// The KHR when offered, else the EXT.
		let [khr, ext] = super::optional::FIFO_LATEST_READY;
		let latest = [khr, ext]
			.into_iter()
			.find(|name| has(name.to_str().unwrap()));
		let latest = latest.filter(|_| !off("fifo_latest_ready"));
		let optional = Optional::found(&offered, disable.as_deref());
		let context = format!("case {case}: {offered:?} {disable:?}");
		assert_eq!(optional.incremental_present.is_some(), on, "{context}");
		assert_eq!(optional.fifo_latest_ready, latest, "{context}");
		let (extensions, names) = (optional.extensions(), optional.names());
		let count = usize::from(on) + usize::from(latest.is_some());
		assert_eq!(extensions.len(), count, "{context}");
		assert_eq!(names.len(), count, "{context}");
		assert_eq!(
			optional.features().len(),
			usize::from(latest.is_some()),
			"{context}"
		);
	}
}

#[test]
fn an_extension_with_two_names_is_used_by_the_preferred_one_offered() {
	use super::optional::FIFO_LATEST_READY;
	let [khr, ext] = FIFO_LATEST_READY;
	let found = |list: &[&str]| Optional::found(&names(list), None).fifo_latest_ready;
	assert_eq!(found(&["VK_EXT_present_mode_fifo_latest_ready"]), Some(ext));
	assert_eq!(found(&["VK_KHR_present_mode_fifo_latest_ready"]), Some(khr));
	// Both offered: the KHR, whatever the order.
	let both = [
		"VK_EXT_present_mode_fifo_latest_ready",
		"VK_KHR_present_mode_fifo_latest_ready",
	];
	assert_eq!(found(&both), Some(khr));
	assert_eq!(found(&["VK_KHR_present_mode_fifo"]), None);
	// The device is made with the name it offered.
	let optional = Optional::found(&names(&both[..1]), None);
	assert_eq!(optional.extensions(), [ext]);
}

#[test]
fn the_second_names_are_the_specifications() {
	use super::optional::{FIFO_LATEST_READY, FIFO_LATEST_READY_FEATURE};
	assert_eq!(
		FIFO_LATEST_READY,
		[
			c"VK_KHR_present_mode_fifo_latest_ready",
			c"VK_EXT_present_mode_fifo_latest_ready"
		]
	);
	// `VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PRESENT_MODE_FIFO_LATEST_READY_FEATURES_KHR`.
	assert_eq!(FIFO_LATEST_READY_FEATURE, 1_000_361_000);
}

#[test]
fn the_variable_turns_either_name_off_by_one_short_name() {
	let offered = names(&[
		"VK_EXT_present_mode_fifo_latest_ready",
		"VK_KHR_incremental_present",
	]);
	let optional = Optional::found(&offered, Some("fifo_latest_ready"));
	assert_eq!(optional.fifo_latest_ready, None);
	assert!(optional.incremental_present.is_some());
	assert_eq!(optional.names(), ["incremental_present"]);
}

#[test]
fn only_extensions_on_ask_for_their_features() {
	let offered = names(&[
		"VK_KHR_incremental_present",
		"VK_KHR_present_mode_fifo_latest_ready",
	]);
	let both = Optional::found(&offered, None);
	assert_eq!(both.features(), [1_000_361_000]);
	assert_eq!(both.names(), ["incremental_present", "fifo_latest_ready"]);
	// Incremental present has no feature to turn on.
	let one = Optional::found(&offered[..1], None);
	assert_eq!(one.features(), Vec::<i32>::new());
	assert_eq!(Optional::default().features(), Vec::<i32>::new());
}

#[test]
fn an_extension_whose_feature_the_device_lacks_is_turned_off() {
	let offered = names(&[
		"VK_KHR_incremental_present",
		"VK_KHR_present_mode_fifo_latest_ready",
	]);
	let mut optional = Optional::found(&offered, None);
	let mut asked = Vec::new();
	optional.keep_supported(|feature| {
		asked.push(feature);
		false
	});
	// Asked about the one feature; what has none stays on.
	assert_eq!(asked, [1_000_361_000]);
	assert_eq!(optional.fifo_latest_ready, None);
	assert_eq!(optional.incremental_present, Some(INCREMENTAL_PRESENT));
	let mut kept = Optional::found(&offered, None);
	kept.keep_supported(|_| true);
	assert_eq!(kept, Optional::found(&offered, None));
	// One already off is not asked about: the device need not know it.
	let mut off = Optional::found(&offered[..1], None);
	off.keep_supported(|feature| panic!("asked about {feature}"));
	assert_eq!(off.incremental_present, Some(INCREMENTAL_PRESENT));
}

#[test]
fn the_memory_budget_is_found_by_its_name_and_turned_off_by_its_short_one() {
	use super::optional::MEMORY_BUDGET;
	assert_eq!(MEMORY_BUDGET, c"VK_EXT_memory_budget");
	let offered = names(&["VK_EXT_memory_budget", "VK_KHR_swapchain"]);
	let on = Optional::found(&offered, None);
	assert_eq!(on.memory_budget, Some(MEMORY_BUDGET));
	assert_eq!(on.names(), ["memory_budget"]);
	// No feature to turn on: only the extension.
	assert_eq!(on.features(), Vec::<i32>::new());
	let off = Optional::found(&offered, Some("memory_budget"));
	assert_eq!(off.memory_budget, None);
	assert!(
		Optional::found(&names(&["VK_EXT_memory_budget2"]), None)
			.memory_budget
			.is_none()
	);
}
