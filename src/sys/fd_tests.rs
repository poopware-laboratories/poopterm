use std::io::{self, Read, Write};

use super::*;

#[test]
fn a_full_pipe_refuses_more_instead_of_waiting() {
	let (mut reader, mut writer) = io::pipe().unwrap();
	set_nonblocking(&writer).unwrap();
	let chunk = [b'x'; 4096];
	let mut written = 0;
	let error = loop {
		match writer.write(&chunk) {
			Ok(n) => written += n,
			Err(error) => break error,
		}
		assert!(written < 64 << 20, "a pipe holds far less");
	};
	assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
	let mut first = [0u8; 10];
	reader.read_exact(&mut first).unwrap();
	assert_eq!(first, [b'x'; 10], "what fit is still there");
}

#[test]
fn an_empty_pipe_read_says_so_at_once() {
	let (mut reader, _writer) = io::pipe().unwrap();
	set_nonblocking(&reader).unwrap();
	let error = reader.read(&mut [0u8; 1]).unwrap_err();
	assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn any_open_descriptor_can_be_made_nonblocking() {
	let (reader, writer) = io::pipe().unwrap();
	drop(reader);
	set_nonblocking(&writer).unwrap();
	let file = std::fs::File::open("/dev/null").unwrap();
	assert!(set_nonblocking(&file).is_ok(), "any open descriptor");
}
