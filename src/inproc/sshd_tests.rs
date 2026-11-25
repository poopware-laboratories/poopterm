use std::net::TcpStream;

use super::*;

fn args(words: &[&str]) -> Vec<String> {
	words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn no_arguments_and_status_ask_for_the_status() {
	assert_eq!(parse(&args(&[])), Ok(Request::Status));
	assert_eq!(parse(&args(&["status"])), Ok(Request::Status));
	assert_eq!(parse(&args(&["stop"])), Ok(Request::Stop));
	assert_eq!(parse(&args(&["log"])), Ok(Request::Log));
	assert!(
		parse(&args(&["stop", "now"]))
			.unwrap_err()
			.contains("unexpected now")
	);
}

#[test]
fn start_takes_a_port_and_a_password_or_keys_only() {
	assert_eq!(
		parse(&args(&["start"])),
		Ok(Request::Start {
			port: DEFAULT_PORT,
			password: None,
			keys_only: false
		})
	);
	assert_eq!(
		parse(&args(&["start", "-p", "2200", "-P", "hunter2"])),
		Ok(Request::Start {
			port: 2200,
			password: Some("hunter2".into()),
			keys_only: false
		})
	);
	assert_eq!(
		parse(&args(&["start", "--port", "4000", "--keys-only"])),
		Ok(Request::Start {
			port: 4000,
			password: None,
			keys_only: true
		})
	);
}

#[test]
fn bad_starts_say_what_is_wrong() {
	for (words, message) in [
		(&["start", "-p"][..], "-p needs a port"),
		(&["start", "-p", "22"], "from 1024 up"),
		(&["start", "-p", "http"], "-p needs a port"),
		(&["start", "-p", "70000"], "-p needs a port"),
		(&["start", "-P"], "-P needs a password"),
		(&["start", "-P", ""], "1 to 100 characters"),
		(&["start", "--keys-only", "-P", "x"], "exclude each other"),
		(&["start", "-x"], "unknown option -x"),
		(&["restart"], "unknown command restart"),
	] {
		let error = parse(&args(words)).unwrap_err();
		assert!(error.contains(message), "{words:?}: {error}");
	}
}

#[test]
fn generated_passwords_use_only_unambiguous_characters() {
	let password = password_from(&[0, 1, 30, 31, 255, 128, 64, 7]);
	assert_eq!(password.len(), 8);
	assert_eq!(password, "23z29649");
	for byte in 0..=255u8 {
		let made = password_from(&[byte; 8]);
		assert!(
			made
				.chars()
				.all(|ch| ch.is_ascii_alphanumeric() && !"01ilo".contains(ch) && !ch.is_ascii_uppercase()),
			"{made}"
		);
	}
	assert_ne!(
		random_password(),
		random_password(),
		"a fresh one each time"
	);
}

#[test]
fn the_host_key_lives_in_the_support_folder() {
	assert_eq!(
		host_key(Path::new("/app")),
		Path::new("/app/Library/poopterm/ssh/host_ed25519")
	);
}

#[test]
fn the_log_keeps_the_last_lines_only() {
	let log = Log::default();
	for number in 0..LOG_LINES + 5 {
		log.push(&format!("line {number}"));
	}
	let text = log.text();
	assert_eq!(text.lines().count(), LOG_LINES);
	assert!(text.starts_with("line 5\n") && text.ends_with(&format!("line {}\n", LOG_LINES + 4)));
}

#[test]
fn a_stopped_server_reports_so_and_cannot_be_stopped_twice() {
	let env = std::collections::BTreeMap::new();
	let (text, status) = command(&args(&["status"]), Path::new("/h"), Path::new("/s"), &env);
	assert_eq!(status, 0);
	assert!(text.contains("not running"), "{text}");
	assert_eq!(
		command(&args(&["stop"]), Path::new("/h"), Path::new("/s"), &env).1,
		1
	);
	assert_eq!(
		command(&args(&["log"]), Path::new("/h"), Path::new("/s"), &env).1,
		1
	);
	let (text, status) = command(
		&args(&["start", "-p", "22"]),
		Path::new("/h"),
		Path::new("/s"),
		&env,
	);
	assert_eq!(status, 2, "{text}");
}

#[test]
fn a_gap_or_a_broken_socket_means_the_listener_is_opened_again() {
	let waiting = std::io::Error::from(std::io::ErrorKind::WouldBlock);
	let interrupted = std::io::Error::from(std::io::ErrorKind::Interrupted);
	let broken = std::io::Error::from_raw_os_error(libc::EBADF);
	let aborted = std::io::Error::from(std::io::ErrorKind::ConnectionAborted);
	let short = Duration::from_millis(100);
	assert!(!reopen(short, &waiting), "nothing to accept yet");
	assert!(!reopen(short, &interrupted));
	assert!(reopen(short, &broken));
	assert!(reopen(short, &aborted));
	assert!(
		!reopen(SUSPENDED_AFTER, &waiting),
		"a slow turn is not a suspension"
	);
	assert!(
		reopen(SUSPENDED_AFTER + short, &waiting),
		"the app was suspended"
	);
}

/// A client's connection reaches the loop's channel.
fn reaches(port: u16, accepted: &mpsc::Receiver<TcpStream>) -> bool {
	let deadline = std::time::Instant::now() + Duration::from_secs(10);
	while std::time::Instant::now() < deadline {
		if TcpStream::connect(("127.0.0.1", port)).is_ok()
			&& accepted.recv_timeout(Duration::from_secs(1)).is_ok()
		{
			return true;
		}
		thread::sleep(Duration::from_millis(20));
	}
	false
}

#[test]
fn connections_arrive_before_and_after_the_app_was_suspended() {
	let listener = listen(0).unwrap();
	let port = listener.local_addr().unwrap().port();
	let stop = Arc::new(AtomicBool::new(false));
	let log = Arc::new(Log::default());
	// A clock the test moves an hour ahead, as a suspension would.
	let ahead = Arc::new(AtomicBool::new(false));
	let (connections, accepted) = mpsc::channel();
	let (flag, lines, jumped) = (stop.clone(), log.clone(), ahead.clone());
	let loop_thread = thread::spawn(move || {
		let clock = move || {
			let hour = Duration::from_secs(3600 * u64::from(jumped.load(Ordering::SeqCst)));
			SystemTime::now() + hour
		};
		accept_connections(listener, port, &flag, &connections, &lines, clock);
	});
	assert!(reaches(port, &accepted));
	assert_eq!(log.text(), "");
	ahead.store(true, Ordering::SeqCst);
	let deadline = std::time::Instant::now() + Duration::from_secs(10);
	while log.text().is_empty() && std::time::Instant::now() < deadline {
		thread::sleep(Duration::from_millis(20));
	}
	assert_eq!(log.text(), format!("listening again on port {port}\n"));
	assert!(reaches(port, &accepted), "the new socket accepts");
	stop.store(true, Ordering::SeqCst);
	loop_thread.join().unwrap();
	assert!(
		TcpStream::connect(("127.0.0.1", port)).is_err(),
		"closed once stopped"
	);
}
