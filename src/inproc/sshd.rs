//! The `sshd` built-in: an ssh server on the device, so another computer
//! can log in to the app's shell. Dropbear's server library does the
//! protocol, keys and authentication (`sys::sshd`); each session it starts
//! is a shell of the app's own, as a tab's is, or one command's output.
//! The server runs only while the app does, and one connection at a time.
use std::{
	collections::{HashMap, VecDeque},
	net::{IpAddr, TcpListener, TcpStream, UdpSocket},
	os::fd::OwnedFd,
	path::{Path, PathBuf},
	sync::{
		Arc, Mutex, MutexGuard,
		atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering},
		mpsc,
	},
	thread,
	time::{Duration, SystemTime},
};

use super::{Size, native, spawn_with};
use crate::sys::sshd::{Account, Refused, Server, Sessions};

/// Where the server listens unless told: above the privileged ports, off
/// the usual 22 an app may not bind.
pub(crate) const DEFAULT_PORT: u16 = 2222;

/// Lines of the log kept.
const LOG_LINES: usize = 100;

/// What `sshd` was asked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
	Start {
		port: u16,
		/// None: one is made up.
		password: Option<String>,
		keys_only: bool,
	},
	Stop,
	Status,
	Log,
}

/// The request of `sshd`'s arguments: `start [-p PORT] [-P PASSWORD |
/// --keys-only]`, `stop`, `status` (also none) or `log`.
pub(crate) fn parse(args: &[String]) -> Result<Request, String> {
	let mut words = args.iter().map(String::as_str);
	match words.next() {
		None | Some("status") => no_more(words).map(|()| Request::Status),
		Some("stop") => no_more(words).map(|()| Request::Stop),
		Some("log") => no_more(words).map(|()| Request::Log),
		Some("start") => {
			let (mut port, mut password, mut keys_only) = (DEFAULT_PORT, None, false);
			while let Some(flag) = words.next() {
				match flag {
					"-p" | "--port" => {
						port = words
							.next()
							.and_then(|text| text.parse().ok())
							.filter(|port| *port >= 1024)
							.ok_or("sshd: -p needs a port number from 1024 up")?;
					}
					"-P" | "--password" => {
						let given = words.next().ok_or("sshd: -P needs a password")?;
						if given.is_empty() || given.len() > 100 {
							return Err("sshd: the password must be 1 to 100 characters".to_owned());
						}
						password = Some(given.to_owned());
					}
					"--keys-only" => keys_only = true,
					other => return Err(format!("sshd: unknown option {other}")),
				}
			}
			if keys_only && password.is_some() {
				return Err("sshd: --keys-only and -P exclude each other".to_owned());
			}
			Ok(Request::Start {
				port,
				password,
				keys_only,
			})
		}
		Some(other) => Err(format!(
			"sshd: unknown command {other}\nusage: sshd [start [-p PORT] [-P PASSWORD | --keys-only] | stop | status | log]"
		)),
	}
}

fn no_more<'a>(mut words: impl Iterator<Item = &'a str>) -> Result<(), String> {
	match words.next() {
		None => Ok(()),
		Some(extra) => Err(format!("sshd: unexpected {extra}")),
	}
}

/// A password of eight characters from `random` bytes, without the ones
/// that look alike.
pub(crate) fn password_from(random: &[u8; 8]) -> String {
	const LETTERS: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";
	random
		.iter()
		.map(|byte| char::from(LETTERS[usize::from(*byte) % LETTERS.len()]))
		.collect()
}

fn random_password() -> String {
	let mut random = [0u8; 8];
	let filled = std::fs::File::open("/dev/urandom")
		.and_then(|mut file| std::io::Read::read_exact(&mut file, &mut random))
		.is_ok();
	if !filled {
		// No randomness: the clock, which is poor but not fixed.
		let now = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map_or(0, |time| time.as_nanos());
		random.copy_from_slice(&now.to_le_bytes()[..8]);
	}
	password_from(&random)
}

/// The address another computer reaches this one at: the interface a
/// packet to the outside would leave by (nothing is sent).
fn local_address() -> Option<IpAddr> {
	let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
	socket.connect("192.0.2.1:9").ok()?;
	let address = socket.local_addr().ok()?.ip();
	(!address.is_unspecified()).then_some(address)
}

/// The server's recent log lines, shared with the connection thread.
#[derive(Default)]
struct Log(Mutex<VecDeque<String>>);

impl Log {
	fn lock(&self) -> MutexGuard<'_, VecDeque<String>> {
		self.0.lock().unwrap_or_else(|poison| poison.into_inner())
	}

	fn push(&self, line: &str) {
		let mut lines = self.lock();
		if lines.len() == LOG_LINES {
			lines.pop_front();
		}
		lines.push_back(line.to_owned());
	}

	fn text(&self) -> String {
		self.lock().iter().map(|line| format!("{line}\n")).collect()
	}
}

/// One session while the connection lasts.
struct Live {
	size: Option<Arc<Size>>,
	/// -1 until a command ends.
	status: Arc<AtomicI32>,
}

/// What the server asks of the app for a connection's sessions.
struct Hooks {
	home: PathBuf,
	support: PathBuf,
	log: Arc<Log>,
	sessions: Mutex<HashMap<i64, Live>>,
	next: AtomicI64,
}

impl Hooks {
	fn sessions(&self) -> MutexGuard<'_, HashMap<i64, Live>> {
		self
			.sessions
			.lock()
			.unwrap_or_else(|poison| poison.into_inner())
	}
}

impl Sessions for Hooks {
	fn open(
		&self,
		command: Option<&str>,
		term: Option<&str>,
		size: (u16, u16),
	) -> Result<(OwnedFd, OwnedFd, i64), i32> {
		let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
		let status = Arc::new(AtomicI32::new(-1));
		let Some(command) = command else {
			let extra: Vec<(String, String)> = term
				.filter(|term| !term.is_empty() && term.len() < 64)
				.map(|term| ("TERM".to_owned(), term.to_owned()))
				.into_iter()
				.collect();
			let device = spawn_with(
				u64::try_from(id).unwrap_or(0),
				self.home.clone(),
				self.home.clone(),
				self.support.clone(),
				(usize::from(size.0), usize::from(size.1)),
				&extra,
			)
			.map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))?;
			self.sessions().insert(
				id,
				Live {
					size: Some(device.size.clone()),
					status,
				},
			);
			return Ok((device.input.into(), device.output.into(), id));
		};
		let (keys, input) =
			std::io::pipe().map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))?;
		let (output, mut printed) =
			std::io::pipe().map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))?;
		let mut shell = super::login_shell(self.home.clone(), self.home.clone(), self.support.clone());
		if let Some(term) = term {
			shell.env.insert("TERM".to_owned(), term.to_owned());
		}
		let command = command.to_owned();
		let done = status.clone();
		self.sessions().insert(id, Live { size: None, status });
		thread::Builder::new()
			.name(format!("sshd-command-{id}"))
			.spawn(move || {
				let ran = shell.run(&command);
				let _ = std::io::Write::write_all(&mut printed, &ran.output);
				let _ = std::io::Write::write_all(&mut printed, &ran.errors);
				// The status is visible before the output ends.
				done.store(shell.status & 0xff, Ordering::SeqCst);
				drop((printed, keys));
			})
			.map_err(|error| error.raw_os_error().unwrap_or(libc::EAGAIN))?;
		Ok((input.into(), output.into(), id))
	}

	fn resize(&self, id: i64, (cols, rows): (u16, u16)) {
		if let Some(size) = self.sessions().get(&id).and_then(|live| live.size.clone()) {
			size.set(usize::from(cols), usize::from(rows));
		}
	}

	fn status(&self, id: i64) -> Option<i32> {
		let status = self.sessions().get(&id)?.status.load(Ordering::SeqCst);
		(status >= 0).then_some(status)
	}

	fn log(&self, line: &str) {
		self.log.push(line);
	}
}

/// A turn of the accept loop longer than this: the app was suspended.
const SUSPENDED_AFTER: Duration = Duration::from_secs(3);

/// Whether the listening socket must be opened again: iOS takes the
/// sockets of a suspended app, which then refuses connections, so after a
/// `gap` that long, or an `error` other than there being nothing to accept.
fn reopen(gap: Duration, error: &std::io::Error) -> bool {
	use std::io::ErrorKind::{Interrupted, WouldBlock};
	gap > SUSPENDED_AFTER || !matches!(error.kind(), WouldBlock | Interrupted)
}

/// A listener on `port` of every interface, which never blocks.
fn listen(port: u16) -> std::io::Result<TcpListener> {
	let listener = TcpListener::bind(("0.0.0.0", port))?;
	listener.set_nonblocking(true)?;
	Ok(listener)
}

/// Sends each connection on until `stop`, opening the socket again when
/// `reopen` says so; `clock` is the wall clock, which runs while the app
/// does not.
fn accept_connections(
	listener: TcpListener,
	port: u16,
	stop: &AtomicBool,
	connections: &mpsc::Sender<TcpStream>,
	log: &Log,
	clock: impl Fn() -> SystemTime,
) {
	let mut listener = Some(listener);
	let mut last = clock();
	while !stop.load(Ordering::SeqCst) {
		let now = clock();
		let gap = now.duration_since(last).unwrap_or_default();
		last = now;
		let Some(open) = &listener else {
			listener = listen(port).ok();
			if listener.is_some() {
				log.push(&format!("listening again on port {port}"));
			} else {
				thread::sleep(Duration::from_secs(1));
			}
			last = clock();
			continue;
		};
		let error = match open.accept() {
			Ok((stream, _)) => {
				if connections.send(stream).is_err() {
					return;
				}
				continue;
			}
			Err(error) => error,
		};
		if reopen(gap, &error) {
			// Closed first, so the port is free to bind.
			listener = None;
		} else {
			thread::sleep(Duration::from_millis(100));
		}
	}
}

/// A server that listens.
struct Running {
	port: u16,
	password: String,
	stop: Arc<AtomicBool>,
	log: Arc<Log>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

fn running() -> MutexGuard<'static, Option<Running>> {
	RUNNING.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// The host key file, in the app's support folder (not the shared one the
/// Files app shows).
pub(crate) fn host_key(support: &Path) -> PathBuf {
	support.join("Library/poopterm/ssh/host_ed25519")
}

/// The `sshd` built-in: its output and exit status. `env` is the shell's;
/// `home` is where the account lands and keeps `.ssh/authorized_keys`.
pub(crate) fn command(
	args: &[String],
	home: &Path,
	support: &Path,
	env: &std::collections::BTreeMap<String, String>,
) -> (String, i32) {
	let request = match parse(args) {
		Ok(request) => request,
		Err(message) => return (format!("{message}\n"), 2),
	};
	let user = env.get("USER").filter(|user| !user.is_empty()).cloned();
	let user = user.unwrap_or_else(|| "mobile".to_owned());
	match request {
		Request::Stop => {
			let Some(server) = running().take() else {
				return ("sshd: not running\n".to_owned(), 1);
			};
			server.stop.store(true, Ordering::SeqCst);
			("sshd: stopped\n".to_owned(), 0)
		}
		Request::Log => match running().as_ref() {
			Some(server) => (server.log.text(), 0),
			None => ("sshd: not running\n".to_owned(), 1),
		},
		Request::Status => (status_text(&user, home), 0),
		Request::Start {
			port,
			password,
			keys_only,
		} => start(port, password, keys_only, &user, home, support, env),
	}
}

fn connect_text(user: &str, port: u16) -> String {
	let host = local_address().map_or_else(|| "<this device>".to_owned(), |ip| ip.to_string());
	format!("  ssh -p {port} {user}@{host}\n")
}

fn status_text(user: &str, home: &Path) -> String {
	match running().as_ref() {
		Some(server) => format!(
			"sshd: listening on port {}\n{}{}keys: {}\n",
			server.port,
			connect_text(user, server.port),
			if server.password.is_empty() {
				"password: none (keys only)\n".to_owned()
			} else {
				format!("password: {}\n", server.password)
			},
			home.join(".ssh/authorized_keys").display()
		),
		None => "sshd: not running; `sshd start` starts it\n".to_owned(),
	}
}

fn start(
	port: u16,
	password: Option<String>,
	keys_only: bool,
	user: &str,
	home: &Path,
	support: &Path,
	env: &std::collections::BTreeMap<String, String>,
) -> (String, i32) {
	if running().is_some() {
		return (status_text(user, home), 1);
	}
	let fail = |message: String| (format!("sshd: {message}\n"), 1);
	let Some(library) = native::find("sshd", env.get("PATH").map(String::as_str), home) else {
		return fail("the ssh server is not installed".to_owned());
	};
	let server = match Server::open(&library) {
		Ok(server) => server,
		Err(error) => return fail(error),
	};
	let key = host_key(support);
	if !key.is_file() {
		if let Some(parent) = key.parent() {
			let _ = std::fs::create_dir_all(parent);
		}
		let mut shell = super::login_shell(home.to_owned(), home.to_owned(), support.to_owned());
		shell
			.env
			.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
		let made = shell.run(&format!("dropbearkey -t ed25519 -f '{}'", key.display()));
		if !key.is_file() {
			let text = String::from_utf8_lossy(&[made.output, made.errors].concat()).into_owned();
			return fail(format!("could not make a host key: {}", text.trim()));
		}
	}
	let listener = match listen(port) {
		Ok(listener) => listener,
		Err(error) => return fail(format!("cannot listen on port {port}: {error}")),
	};
	let password = if keys_only {
		String::new()
	} else {
		password.unwrap_or_else(random_password)
	};
	let stop = Arc::new(AtomicBool::new(false));
	let log = Arc::new(Log::default());
	let hooks = Hooks {
		home: home.to_owned(),
		support: support.to_owned(),
		log: log.clone(),
		sessions: Mutex::new(HashMap::new()),
		next: AtomicI64::new(0),
	};
	let (connections, accepted) = mpsc::channel();
	let (flag, lines) = (stop.clone(), log.clone());
	let acceptor = thread::Builder::new()
		.name("sshd-accept".to_owned())
		.spawn(move || {
			accept_connections(listener, port, &flag, &connections, &lines, SystemTime::now);
		});
	if acceptor.is_err() {
		return fail("cannot start its thread".to_owned());
	}
	let identity = (
		user.to_owned(),
		home.display().to_string(),
		password.clone(),
		key.display().to_string(),
	);
	let handler = thread::Builder::new()
		.name("sshd-serve".to_owned())
		.spawn(move || {
			let account = Account {
				username: &identity.0,
				home: &identity.1,
				password: &identity.2,
				hostkey: &identity.3,
			};
			for stream in accepted {
				if stream.set_nonblocking(false).is_err() {
					continue;
				}
				let _ = stream.set_nodelay(true);
				hooks.log.push("connection");
				match server.serve(OwnedFd::from(stream), &account, &hooks) {
					Ok(()) => {}
					Err((Refused::NoHostKey, _)) => hooks.log.push("no host key"),
					Err((Refused::Other(code), _)) => hooks.log.push(&format!("refused: {code}")),
				}
				hooks.sessions().clear();
			}
		});
	if handler.is_err() {
		stop.store(true, Ordering::SeqCst);
		return fail("cannot start its thread".to_owned());
	}
	*running() = Some(Running {
		port,
		password,
		stop,
		log,
	});
	(format!("sshd: started\n{}", status_text(user, home)), 0)
}

#[cfg(test)]
#[path = "sshd_tests.rs"]
mod tests;
