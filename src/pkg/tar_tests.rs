//! The tar reader: archives written by hand and by the system's `tar`,
//! long names, and every entry a package must not contain.
use super::tar::{Entry, Kind, entries};

/// A 512-byte header for `name` of `kind` (a type flag) and `size`, with
/// `mode`, its checksum filled in.
fn header(name: &str, kind: u8, size: usize, mode: u32) -> Vec<u8> {
	let mut block = vec![0u8; 512];
	block[..name.len()].copy_from_slice(name.as_bytes());
	block[100..107].copy_from_slice(format!("{mode:07o}").as_bytes());
	block[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
	block[156] = kind;
	block[257..263].copy_from_slice(b"ustar\0");
	block[263..265].copy_from_slice(b"00");
	block[148..156].fill(b' ');
	let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum();
	block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
	block
}

/// `data` padded to whole blocks.
fn padded(data: &[u8]) -> Vec<u8> {
	let mut out = data.to_vec();
	out.resize(data.len().div_ceil(512) * 512, 0);
	out
}

/// An archive of `(name, kind, data, mode)` entries, ended by two zero blocks.
fn archive(items: &[(&str, u8, &[u8], u32)]) -> Vec<u8> {
	let mut out = Vec::new();
	for &(name, kind, data, mode) in items {
		out.extend(header(name, kind, data.len(), mode));
		out.extend(padded(data));
	}
	out.extend([0u8; 1024]);
	out
}

fn file<'a>(entry: &'a Entry<'a>) -> (&'a str, &'a [u8], bool) {
	let Kind::File { executable } = entry.kind else {
		panic!("{entry:?}");
	};
	(entry.path.as_str(), entry.data, executable)
}

#[test]
fn files_and_directories_come_back_in_order_with_their_data() {
	let bytes = archive(&[
		("bin/", b'5', b"", 0o755),
		("bin/jot", b'0', b"\xcf\xfa\xed\xfe library", 0o755),
		("share/doc.txt", 0, &[b'x'; 700], 0o644),
	]);
	let got = entries(&bytes).unwrap();
	assert_eq!(got.len(), 3);
	assert_eq!(
		(got[0].path.as_str(), got[0].kind),
		("bin", Kind::Directory)
	);
	assert_eq!(
		file(&got[1]),
		("bin/jot", &b"\xcf\xfa\xed\xfe library"[..], true)
	);
	assert_eq!(file(&got[2]), ("share/doc.txt", &[b'x'; 700][..], false));
}

#[test]
fn leading_dot_slashes_are_dropped_and_the_root_is_skipped() {
	let bytes = archive(&[("./", b'5', b"", 0o755), ("./bin/a", b'0', b"a", 0o755)]);
	let got = entries(&bytes).unwrap();
	assert_eq!(got.len(), 1);
	assert_eq!(got[0].path, "bin/a");
}

#[test]
fn the_ustar_prefix_is_joined_to_the_name() {
	let mut block = header("tool", b'0', 2, 0o755);
	block[345..349].copy_from_slice(b"deep");
	block[148..156].fill(b' ');
	let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum();
	block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
	let mut bytes = block;
	bytes.extend(padded(b"hi"));
	bytes.extend([0u8; 1024]);
	assert_eq!(entries(&bytes).unwrap()[0].path, "deep/tool");
}

#[test]
fn a_pax_path_record_names_the_next_entry() {
	let long = format!("share/{}/file", "d".repeat(150));
	let record = format!("{} path={long}\n", long.len() + 10);
	assert_eq!(record.len(), long.len() + 10, "the length counts itself");
	let bytes = archive(&[
		("PaxHeader/file", b'x', record.as_bytes(), 0o644),
		("share/ddd/file", b'0', b"long", 0o644),
		("bin/b", b'0', b"short", 0o755),
	]);
	let got = entries(&bytes).unwrap();
	assert_eq!(got[0].path, long);
	assert_eq!(got[1].path, "bin/b", "the record names one entry only");
}

#[test]
fn global_pax_headers_are_skipped() {
	let bytes = archive(&[
		("pax_global_header", b'g', b"18 comment=hello\n", 0o644),
		("a", b'0', b"a", 0o644),
	]);
	let got = entries(&bytes).unwrap();
	assert_eq!(got.len(), 1);
	assert_eq!(got[0].path, "a");
}

#[test]
fn what_the_systems_tar_writes_reads_back() {
	let dir = std::env::temp_dir().join(format!("poopterm-tar-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&dir);
	let deep = format!("share/{}", "n".repeat(120));
	std::fs::create_dir_all(dir.join("bin")).unwrap();
	std::fs::create_dir_all(dir.join(&deep)).unwrap();
	std::fs::write(dir.join("bin/tool"), b"#tool").unwrap();
	std::fs::write(dir.join(&deep).join("notes"), b"notes\n").unwrap();
	let out = std::process::Command::new("tar")
		.env("COPYFILE_DISABLE", "1")
		.args(["-cf", "-", "-C"])
		.arg(&dir)
		.args(["bin", "share"])
		.output()
		.unwrap();
	std::fs::remove_dir_all(&dir).unwrap();
	let got = entries(&out.stdout).unwrap();
	let files: Vec<_> = got
		.iter()
		.filter(|entry| matches!(entry.kind, Kind::File { .. }))
		.map(|entry| (entry.path.clone(), entry.data.to_vec()))
		.collect();
	assert!(
		files.contains(&("bin/tool".to_owned(), b"#tool".to_vec())),
		"{files:?}"
	);
	let notes = (format!("{deep}/notes"), b"notes\n".to_vec());
	assert!(files.contains(&notes), "{files:?}");
}

#[test]
fn paths_that_climb_out_or_start_at_the_root_are_refused() {
	for name in [
		"../evil",
		"bin/../../evil",
		"/etc/passwd",
		"a/./../../b",
		"..",
	] {
		let bytes = archive(&[(name, b'0', b"x", 0o644)]);
		assert!(entries(&bytes).is_err(), "{name}");
	}
}

#[test]
fn links_devices_and_unknown_kinds_are_refused() {
	for kind in *b"123467LKZ" {
		let bytes = archive(&[("thing", kind, b"", 0o644)]);
		let error = entries(&bytes).unwrap_err();
		assert!(error.contains("thing"), "{} {error}", kind as char);
	}
}

#[test]
fn a_damaged_checksum_is_refused() {
	let mut bytes = archive(&[("a", b'0', b"data", 0o644)]);
	bytes[0] = b'b';
	assert!(entries(&bytes).unwrap_err().contains("checksum"));
}

#[test]
fn data_shorter_than_its_size_is_refused() {
	let bytes = archive(&[("a", b'0', &[1; 600], 0o644)]);
	assert!(entries(&bytes[..512 + 100]).is_err());
	assert!(entries(&bytes[..300]).is_err(), "a partial header");
}

#[test]
fn an_archive_may_end_without_its_zero_blocks() {
	let bytes = archive(&[("a", b'0', b"data", 0o644)]);
	assert_eq!(entries(&bytes[..1024]).unwrap().len(), 1);
	assert_eq!(entries(&[]).unwrap().len(), 0);
}

#[test]
fn sizes_that_are_not_octal_are_refused() {
	let mut block = header("a", b'0', 0, 0o644);
	block[124..135].copy_from_slice(b"0000000009x");
	block[148..156].fill(b' ');
	let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum();
	block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
	assert!(entries(&block).is_err());
}

#[test]
fn a_name_that_is_not_utf8_is_refused() {
	let mut block = header("a", b'0', 0, 0o644);
	block[0] = 0xff;
	block[148..156].fill(b' ');
	let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum();
	block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
	assert!(entries(&block).is_err());
}
