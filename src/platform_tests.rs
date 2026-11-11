//! The process's user ID against what the system says owns what we make,
//! and against what other tools report for us.
use super::platform::uid;
use std::os::unix::fs::MetadataExt;

#[test]
fn files_we_create_are_owned_by_our_uid() {
	let dir = std::env::temp_dir().join(format!("poopterm-uid-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let file = dir.join("owned");
	std::fs::write(&file, b"x").unwrap();
	let (dir_owner, file_owner) = (
		std::fs::metadata(&dir).unwrap().uid(),
		std::fs::metadata(&file).unwrap().uid(),
	);
	std::fs::remove_dir_all(&dir).unwrap();
	assert_eq!(file_owner, uid());
	assert_eq!(dir_owner, uid());
}

#[test]
fn the_uid_is_what_id_reports() {
	let Ok(output) = std::process::Command::new("id").arg("-ru").output() else {
		return; // No `id` to ask.
	};
	assert!(output.status.success());
	let reported: u32 = String::from_utf8(output.stdout)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	assert_eq!(reported, uid());
}

#[cfg(target_os = "linux")]
#[test]
fn the_uid_is_the_real_one_the_kernel_lists_for_this_process() {
	let status = std::fs::read_to_string("/proc/self/status").unwrap();
	let ids = status
		.lines()
		.find_map(|line| line.strip_prefix("Uid:"))
		.unwrap();
	// Real, effective, saved and file system user IDs; ours is the real one.
	let real: u32 = ids.split_whitespace().next().unwrap().parse().unwrap();
	assert_eq!(real, uid());
}

#[test]
fn the_uid_does_not_change_between_calls_or_threads() {
	let here = uid();
	let there = std::thread::spawn(uid).join().unwrap();
	assert_eq!((here, there), (uid(), uid()));
}
