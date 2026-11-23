//! Downloads: files read from disk, the system's HTTP stack answering a
//! server on this machine, and the URLs and sizes refused.
use std::{
	io::{Read, Write},
	net::TcpListener,
};

use super::fetch::fetch;

#[test]
fn a_file_url_reads_the_file() {
	let path = std::env::temp_dir().join(format!("poopterm-fetch-{}", std::process::id()));
	std::fs::write(&path, b"index text").unwrap();
	let url = format!("file://{}", path.display());
	assert_eq!(fetch(&url, 100).unwrap(), b"index text");
	assert!(
		fetch(&url, 5).unwrap_err().contains("larger"),
		"past the limit"
	);
	std::fs::remove_file(&path).unwrap();
	assert!(fetch(&url, 100).is_err(), "gone");
}

#[test]
fn urls_from_anywhere_but_https_files_or_this_machine_are_refused() {
	for url in ["http://example.org/index", "ftp://example.org/x", "index"] {
		let error = fetch(url, 100).unwrap_err();
		assert!(error.contains("https"), "{url}: {error}");
	}
}

/// Serves `status` and `body` to one request on a port of this machine.
fn serve_once(status: &'static str, body: &'static [u8]) -> String {
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let port = listener.local_addr().unwrap().port();
	std::thread::spawn(move || {
		let (mut stream, _) = listener.accept().unwrap();
		let mut request = [0u8; 2048];
		let _ = stream.read(&mut request);
		let head = format!(
			"HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
			body.len()
		);
		let _ = stream.write_all(head.as_bytes());
		let _ = stream.write_all(body);
	});
	format!("http://127.0.0.1:{port}/index")
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[test]
fn the_systems_http_stack_downloads_from_this_machine() {
	let url = serve_once("200 OK", b"jot 1 hash 3 jot.tar.gz\n");
	assert_eq!(fetch(&url, 1000).unwrap(), b"jot 1 hash 3 jot.tar.gz\n");
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[test]
fn an_http_error_or_a_body_past_the_limit_is_refused() {
	let url = serve_once("404 Not Found", b"missing");
	let error = fetch(&url, 1000).unwrap_err();
	assert!(error.contains("404"), "{error}");
	let url = serve_once("200 OK", b"0123456789");
	assert!(fetch(&url, 4).unwrap_err().contains("larger"));
}
