//! The delegated subtree on a fake cgroupfs: a temporary directory whose
//! control files are plain files, so the tests see what was written.
use std::{
	fs,
	path::{Path, PathBuf},
};

use super::{TERMINAL_CPU_WEIGHT, Tree, delegated, own_cgroup};

/// A fresh fake cgroup directory holding `procs`, offering `controllers`.
fn cgroup(name: &str, procs: &str, controllers: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-cgroup-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	fs::write(dir.join("cgroup.procs"), procs).unwrap();
	fs::write(dir.join("cgroup.controllers"), controllers).unwrap();
	dir
}

fn read(path: impl AsRef<Path>) -> String {
	fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn our_cgroup_is_found_under_the_mount() {
	let mount = Path::new("/sys/fs/cgroup");
	let path = own_cgroup(
		mount,
		"0::/user.slice/app.slice/app-poopterm-terminal-7.scope\n",
	);
	let expected = mount.join("user.slice/app.slice/app-poopterm-terminal-7.scope");
	assert_eq!(path, Some(expected));
}

#[test]
fn the_root_hybrid_and_odd_paths_are_never_ours() {
	let mount = Path::new("/sys/fs/cgroup");
	for text in [
		"0::/\n",
		"",
		"1:name=systemd:/user.slice\n0::/user.slice\n",
		"0::user.slice\n",
		"0::/user.slice/../system.slice\n",
		"0::/user.slice/./x\n",
		"0::/a//b\n",
	] {
		assert_eq!(own_cgroup(mount, text), None, "{text:?}");
	}
}

#[test]
fn adopting_moves_us_into_the_terminal_leaf_and_enables_controllers() {
	let dir = cgroup("adopt", "77\n", "cpuset cpu io memory pids\n");
	let tree = Tree::adopt(&dir, 77, true, Some(4096)).unwrap();
	assert_eq!(read(dir.join("terminal/cgroup.procs")), "77");
	assert_eq!(
		read(dir.join("cgroup.subtree_control")),
		"+cpu +memory +pids"
	);
	let weight = read(dir.join("terminal/cpu.weight"));
	assert_eq!(weight, TERMINAL_CPU_WEIGHT.to_string());
	assert_eq!(TERMINAL_CPU_WEIGHT, 400, "four times a tab's default 100");
	// The terminal is never throttled by memory.
	assert!(!dir.join("terminal/memory.high").exists());
	drop(tree);
}

#[test]
fn each_shell_gets_a_leaf_with_the_memory_limit() {
	let dir = cgroup("shells", "77\n", "cpu memory pids\n");
	let tree = Tree::adopt(&dir, 77, true, Some(4096)).unwrap();
	assert!(tree.place(100));
	assert!(tree.place(200));
	assert_eq!(read(dir.join("tab-100/cgroup.procs")), "100");
	assert_eq!(read(dir.join("tab-100/memory.high")), "4096");
	assert_eq!(read(dir.join("tab-200/cgroup.procs")), "200");
	// Tabs keep the default CPU weight.
	assert!(!dir.join("tab-100/cpu.weight").exists());
}

#[test]
fn only_delegated_controllers_are_enabled_and_used() {
	let dir = cgroup("pids-only", "77\n", "pids\n");
	let tree = Tree::adopt(&dir, 77, true, Some(4096)).unwrap();
	assert_eq!(read(dir.join("cgroup.subtree_control")), "+pids");
	assert!(!dir.join("terminal/cpu.weight").exists());
	assert!(tree.place(5));
	assert!(!dir.join("tab-5/memory.high").exists());
	// None at all: still grouped, nothing enabled.
	let dir = cgroup("no-controllers", "77\n", "");
	let tree = Tree::adopt(&dir, 77, true, Some(4096)).unwrap();
	assert!(!dir.join("cgroup.subtree_control").exists());
	assert!(tree.place(6));
	assert_eq!(read(dir.join("tab-6/cgroup.procs")), "6");
}

#[test]
fn without_a_memory_limit_tabs_are_not_throttled() {
	let dir = cgroup("no-limit", "77\n", "memory\n");
	let tree = Tree::adopt(&dir, 77, true, None).unwrap();
	assert!(tree.place(5));
	assert!(!dir.join("tab-5/memory.high").exists());
}

#[test]
fn a_cgroup_shared_with_other_processes_is_left_alone() {
	// Started from a shell whose cgroup we would otherwise take over.
	let dir = cgroup("shared", "12\n77\n", "cpu memory pids\n");
	assert!(Tree::adopt(&dir, 77, true, None).is_none());
	assert!(!dir.join("terminal").exists());
	assert!(!dir.join("cgroup.subtree_control").exists());
}

#[test]
fn in_a_scope_made_for_us_every_process_moves_into_the_terminal_leaf() {
	// Shells forked before the layout was built sit in the scope's root,
	// where they would block enabling controllers.
	let dir = cgroup("scope", "12\n77\n", "cpu memory pids\n");
	let tree = Tree::adopt(&dir, 77, false, None).unwrap();
	// The fake records only the last write: ours went first.
	assert_eq!(read(dir.join("terminal/cgroup.procs")), "12");
	assert_eq!(
		read(dir.join("cgroup.subtree_control")),
		"+cpu +memory +pids"
	);
	drop(tree);
}

#[test]
fn a_cgroup_we_are_not_in_is_not_adopted() {
	let dir = cgroup("elsewhere", "12\n", "cpu\n");
	assert!(Tree::adopt(&dir, 77, false, None).is_none());
	let dir = cgroup("empty", "", "cpu\n");
	assert!(Tree::adopt(&dir, 77, false, None).is_none());
	let dir = cgroup("garbage", "77x\n", "cpu\n");
	assert!(Tree::adopt(&dir, 77, false, None).is_none());
}

#[test]
fn a_missing_cgroup_is_not_adopted() {
	let missing = std::env::temp_dir().join("poopterm-cgroup-missing-nowhere");
	assert!(Tree::adopt(&missing, 77, true, None).is_none());
	assert!(!missing.exists());
}

/// Whether permissions bind us: root writes through read-only modes.
fn permissions_apply() -> bool {
	// SAFETY: geteuid takes no arguments and cannot fail.
	unsafe { libc::geteuid() != 0 }
}

#[test]
fn a_read_only_cgroup_is_not_adopted() {
	use std::os::unix::fs::PermissionsExt;
	if !permissions_apply() {
		return;
	}
	let dir = cgroup("read-only", "77\n", "cpu\n");
	fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
	let tree = Tree::adopt(&dir, 77, true, None);
	fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
	assert!(tree.is_none());
	assert!(!dir.join("terminal").exists());
}

#[test]
fn a_shell_that_cannot_be_moved_is_reported() {
	use std::os::unix::fs::PermissionsExt;
	if !permissions_apply() {
		return;
	}
	let dir = cgroup("stuck", "77\n", "\n");
	let tree = Tree::adopt(&dir, 77, true, None).unwrap();
	fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
	let placed = tree.place(5);
	fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
	assert!(!placed, "the caller falls back to a scope of its own");
}

#[test]
fn empty_leaves_of_closed_tabs_are_removed() {
	let dir = cgroup("prune", "77\n", "\n");
	let tree = Tree::adopt(&dir, 77, true, None).unwrap();
	// A closed tab's leaf is empty; a live one still lists its shell.
	fs::create_dir(dir.join("tab-1")).unwrap();
	assert!(tree.place(2));
	assert!(tree.place(3));
	assert!(!dir.join("tab-1").exists());
	assert!(
		dir.join("tab-2").exists(),
		"on cgroupfs, a busy cgroup stays"
	);
	assert!(dir.join("terminal").exists(), "only tab leaves are pruned");
	// Placing a shell whose leaf is left over reuses it.
	assert!(tree.place(2));
	assert_eq!(read(dir.join("tab-2/cgroup.procs")), "2");
}

/// `/proc/self/cgroup` as `delegated` reads it: `moves` reads in the old
/// cgroup, then in `to`.
fn proc_self(from: &'static str, to: String, moves: usize) -> impl Fn(&str) -> String {
	let reads = std::cell::Cell::new(0);
	move |path| {
		assert_eq!(path, "/proc/self/cgroup");
		reads.set(reads.get() + 1);
		if reads.get() > moves {
			to.clone()
		} else {
			from.into()
		}
	}
}

#[test]
fn once_systemd_moves_us_the_layout_is_built_in_its_scope() {
	let dir = cgroup("app-poopterm-terminal-77.scope", "12\n77\n", "cpu\n");
	let mount = dir.parent().unwrap();
	let name = dir.file_name().unwrap().to_str().unwrap();
	let read = proc_self("0::/user.slice\n", format!("0::/{name}\n"), 2);
	let tree = delegated(&read, mount, 77, None, Some(name), 50).unwrap();
	assert!(tree.place(5));
	assert_eq!(read_file(&dir, "tab-5/cgroup.procs"), "5");
}

fn read_file(dir: &Path, name: &str) -> String {
	read(dir.join(name))
}

#[test]
fn without_systemd_our_own_cgroup_is_used_when_it_is_ours_alone() {
	let dir = cgroup("own", "77\n", "memory\n");
	let mount = dir.parent().unwrap();
	let name = dir.file_name().unwrap().to_str().unwrap();
	let read = proc_self("", format!("0::/{name}\n"), 0);
	let tree = delegated(&read, mount, 77, Some(64), None, 50).unwrap();
	assert!(tree.place(5));
	assert_eq!(read_file(&dir, "tab-5/memory.high"), "64");
	// Shared with another process: nothing is touched.
	let dir = cgroup("own-shared", "12\n77\n", "memory\n");
	let name = dir.file_name().unwrap().to_str().unwrap();
	let read = proc_self("", format!("0::/{name}\n"), 0);
	assert!(delegated(&read, mount, 77, None, None, 50).is_none());
}

#[test]
fn if_systemd_never_moves_us_only_an_unshared_cgroup_is_used() {
	// The scope request was lost: we stay in a cgroup shared with others.
	let dir = cgroup("never-moved", "12\n77\n", "cpu\n");
	let mount = dir.parent().unwrap();
	let name = dir.file_name().unwrap().to_str().unwrap().to_owned();
	let current = format!("0::/{name}\n");
	let read = proc_self("", current, 0);
	let scope = Some("app-poopterm-terminal-77.scope");
	assert!(delegated(&read, mount, 77, None, scope, 3).is_none());
	assert!(!dir.join("terminal").exists());
}

#[test]
fn adopting_again_after_a_restart_reuses_the_terminal_leaf() {
	let dir = cgroup("again", "77\n", "cpu\n");
	Tree::adopt(&dir, 77, true, None).unwrap();
	// Back in the root, as after a crash and restart in the same scope.
	fs::write(dir.join("cgroup.procs"), "77\n").unwrap();
	assert!(Tree::adopt(&dir, 77, true, None).is_some());
	assert_eq!(read(dir.join("terminal/cgroup.procs")), "77");
}

#[test]
fn a_file_where_a_leaf_belongs_is_not_used_as_one() {
	let dir = cgroup("file-terminal", "77\n", "\n");
	fs::write(dir.join("terminal"), "").unwrap();
	assert!(Tree::adopt(&dir, 77, true, None).is_none());
	let dir = cgroup("file-tab", "77\n", "\n");
	let tree = Tree::adopt(&dir, 77, true, None).unwrap();
	fs::write(dir.join("tab-5"), "").unwrap();
	assert!(!tree.place(5));
}

#[test]
fn controllers_are_enabled_in_a_fixed_order_whatever_the_offer_looks_like() {
	let dir = cgroup("offer", "77\n", "pids   io\nmemory cpuset cpu\n");
	Tree::adopt(&dir, 77, true, None).unwrap();
	assert_eq!(
		read(dir.join("cgroup.subtree_control")),
		"+cpu +memory +pids"
	);
	// Names are matched whole: `cpuset` alone does not offer `cpu`.
	let dir = cgroup("cpuset", "77\n", "cpuset memoryx\n");
	Tree::adopt(&dir, 77, true, None).unwrap();
	assert!(!dir.join("cgroup.subtree_control").exists());
}

#[test]
fn controllers_that_fail_to_enable_are_not_used() {
	// The write fails, as cgroupfs refuses it with processes in the root.
	let dir = cgroup("refused", "77\n", "cpu memory\n");
	fs::create_dir(dir.join("cgroup.subtree_control")).unwrap();
	let tree = Tree::adopt(&dir, 77, true, Some(4096)).unwrap();
	assert!(!dir.join("terminal/cpu.weight").exists());
	assert!(tree.place(5));
	assert!(!dir.join("tab-5/memory.high").exists());
}

#[test]
fn process_lists_with_padding_parse_and_garbage_does_not() {
	let dir = cgroup("padded", "  77  \n", "\n");
	assert!(Tree::adopt(&dir, 77, true, None).is_some());
	for procs in ["77\n\n", "-77\n", "77 12\n", "4294967296\n"] {
		let dir = cgroup("bad-procs", procs, "\n");
		assert!(Tree::adopt(&dir, 77, false, None).is_none(), "{procs:?}");
	}
}

#[test]
fn only_empty_tab_leaves_are_pruned() {
	let dir = cgroup("prune-names", "77\n", "\n");
	let tree = Tree::adopt(&dir, 77, true, None).unwrap();
	for name in ["tablet", "tab", "app-1.scope"] {
		fs::create_dir(dir.join(name)).unwrap();
	}
	fs::create_dir(dir.join("tab-9")).unwrap();
	assert!(tree.place(5));
	for name in ["tablet", "tab", "app-1.scope"] {
		assert!(dir.join(name).exists(), "{name} is not ours to remove");
	}
	assert!(!dir.join("tab-9").exists());
}

#[test]
fn the_largest_limit_and_pid_are_written_whole() {
	let dir = cgroup("largest", "77\n", "memory\n");
	let tree = Tree::adopt(&dir, 77, true, Some(u64::MAX)).unwrap();
	assert!(tree.place(u32::MAX));
	let leaf = dir.join(format!("tab-{}", u32::MAX));
	assert_eq!(read(leaf.join("memory.high")), u64::MAX.to_string());
	assert_eq!(read(leaf.join("cgroup.procs")), u32::MAX.to_string());
}

#[test]
fn a_scope_we_were_moved_into_but_cannot_lay_out_is_not_replaced() {
	// systemd moved us, but the scope lists us nowhere (it was torn down):
	// our previous cgroup is not taken over instead.
	let dir = cgroup("torn-down", "12\n", "cpu\n");
	let mount = dir.parent().unwrap();
	let name = dir.file_name().unwrap().to_str().unwrap();
	let read = proc_self("", format!("0::/{name}\n"), 0);
	assert!(delegated(&read, mount, 77, None, Some(name), 5).is_none());
	assert!(!dir.join("terminal").exists());
}

#[test]
fn without_attempts_the_scope_is_not_waited_for() {
	let dir = cgroup("no-wait", "77\n", "\n");
	let mount = dir.parent().unwrap();
	let name = dir.file_name().unwrap().to_str().unwrap();
	let read = proc_self("", format!("0::/{name}\n"), 0);
	let tree = delegated(
		&read,
		mount,
		77,
		None,
		Some("app-poopterm-terminal-77.scope"),
		0,
	);
	assert!(tree.is_some(), "our own cgroup, unshared, is used at once");
}
