//! A D-Bus client written from the specification: finding the session bus,
//! authenticating, and exchanging messages over its Unix socket.
#[cfg(test)]
mod bus_tests;
pub(crate) mod message;
#[cfg(test)]
pub(crate) mod test_bus;
pub(crate) mod wire;
#[cfg(test)]
mod wire_tests;

use std::{
	io::{self, Read, Write},
	os::unix::net::UnixStream,
	path::PathBuf,
};

use message::{Kind, Message};
use wire::Value;

const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
/// Longest authentication line read from the bus.
const MAX_LINE: usize = 512;

/// Where a `unix:` address points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
	Path(PathBuf),
	/// A Linux abstract socket name.
	Abstract(Vec<u8>),
}

/// The sockets a D-Bus address names, in order; other transports are
/// skipped.
pub(crate) fn targets(address: &str, runtime_dir: Option<&str>) -> Vec<Target> {
	let mut targets = Vec::new();
	for entry in address.split(';') {
		let Some(keys) = entry.strip_prefix("unix:") else {
			continue;
		};
		for pair in keys.split(',') {
			let Some((key, value)) = pair.split_once('=') else {
				continue;
			};
			let Some(value) = unescape(value) else {
				continue;
			};
			match key {
				"path" => targets.push(Target::Path(PathBuf::from(
					String::from_utf8_lossy(&value).into_owned(),
				))),
				"abstract" => targets.push(Target::Abstract(value)),
				"runtime" if value == b"yes" => {
					if let Some(dir) = runtime_dir {
						targets.push(Target::Path(PathBuf::from(dir).join("bus")));
					}
				}
				_ => {}
			}
		}
	}
	targets
}

/// Undoes the `%xx` escapes of address values.
fn unescape(value: &str) -> Option<Vec<u8>> {
	let mut out = Vec::with_capacity(value.len());
	let mut bytes = value.bytes();
	while let Some(byte) = bytes.next() {
		if byte == b'%' {
			let digits = [bytes.next()?, bytes.next()?];
			out.push(u8::from_str_radix(std::str::from_utf8(&digits).ok()?, 16).ok()?);
		} else {
			out.push(byte);
		}
	}
	Some(out)
}

/// A connection to a bus, with its unique name.
pub(crate) struct Connection {
	sender: Sender,
	receiver: Receiver,
	name: String,
}

impl Connection {
	/// Connects to the session bus of `DBUS_SESSION_BUS_ADDRESS`, or the
	/// default one in the runtime directory.
	pub(crate) fn session() -> io::Result<Self> {
		let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
		let address =
			std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| "unix:runtime=yes".to_owned());
		Self::connect(&address, runtime.as_deref())
	}

	pub(crate) fn connect(address: &str, runtime_dir: Option<&str>) -> io::Result<Self> {
		let mut error = io::Error::new(io::ErrorKind::NotFound, "no usable D-Bus address");
		for target in targets(address, runtime_dir) {
			match open(&target).and_then(Self::start) {
				Ok(connection) => return Ok(connection),
				Err(failure) => error = failure,
			}
		}
		Err(error)
	}

	/// Authenticates as this user and says Hello.
	fn start(mut stream: UnixStream) -> io::Result<Self> {
		// SAFETY: getuid takes no arguments and cannot fail.
		let uid = unsafe { libc::getuid() }.to_string();
		let hex: String = uid.bytes().map(|byte| format!("{byte:02x}")).collect();
		stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
		let reply = read_line(&mut stream)?;
		if !reply.starts_with("OK ") {
			return Err(io::Error::new(
				io::ErrorKind::PermissionDenied,
				"D-Bus refused authentication",
			));
		}
		stream.write_all(b"BEGIN\r\n")?;
		let mut connection = Self {
			sender: Sender {
				stream: stream.try_clone()?,
				serial: 0,
			},
			receiver: Receiver {
				stream,
				buffer: Vec::new(),
			},
			name: String::new(),
		};
		let reply = connection.call(Message::method_call(BUS, BUS_PATH, BUS, "Hello"))?;
		connection.name = reply
			.body
			.first()
			.and_then(Value::as_str)
			.unwrap_or_default()
			.to_owned();
		Ok(connection)
	}

	/// The name the bus gave this connection.
	#[cfg(test)]
	pub(crate) fn unique_name(&self) -> &str {
		&self.name
	}

	/// Sends a method call and waits for its reply, dropping other messages.
	pub(crate) fn call(&mut self, message: Message) -> io::Result<Message> {
		let serial = self.sender.send(message)?;
		loop {
			let reply = self.receiver.receive()?;
			if reply.reply_serial != Some(serial) {
				continue;
			}
			if reply.kind == Kind::Error {
				let name = reply.error_name.unwrap_or_default();
				return Err(io::Error::other(format!("D-Bus error {name}")));
			}
			return Ok(reply);
		}
	}

	/// Asks the bus for signals matching `rule`.
	pub(crate) fn add_match(&mut self, rule: &str) -> io::Result<()> {
		let mut message = Message::method_call(BUS, BUS_PATH, BUS, "AddMatch");
		message.body = vec![Value::Str(rule.into())];
		self.call(message).map(drop)
	}

	/// Splits the connection so one thread can send while another receives.
	pub(crate) fn split(self) -> (Sender, Receiver) {
		(self.sender, self.receiver)
	}
}

fn open(target: &Target) -> io::Result<UnixStream> {
	match target {
		Target::Path(path) => UnixStream::connect(path),
		#[cfg(any(target_os = "linux", target_os = "android"))]
		Target::Abstract(name) => {
			use std::os::{linux::net::SocketAddrExt, unix::net::SocketAddr};
			UnixStream::connect_addr(&SocketAddr::from_abstract_name(name)?)
		}
		#[cfg(not(any(target_os = "linux", target_os = "android")))]
		Target::Abstract(_) => Err(io::ErrorKind::Unsupported.into()),
	}
}

/// Reads one `\r\n`-terminated authentication line, byte by byte so no
/// message data after it is consumed.
fn read_line(stream: &mut UnixStream) -> io::Result<String> {
	let mut line = Vec::new();
	let mut byte = [0];
	while !line.ends_with(b"\r\n") {
		if line.len() >= MAX_LINE {
			return Err(io::ErrorKind::InvalidData.into());
		}
		stream.read_exact(&mut byte)?;
		line.push(byte[0]);
	}
	line.truncate(line.len() - 2);
	String::from_utf8(line).map_err(|_| io::ErrorKind::InvalidData.into())
}

/// The sending half of a connection.
pub(crate) struct Sender {
	stream: UnixStream,
	serial: u32,
}

impl Sender {
	/// Sends `message` with the next serial, which it returns.
	pub(crate) fn send(&mut self, mut message: Message) -> io::Result<u32> {
		self.serial = self.serial.checked_add(1).unwrap_or(1);
		message.serial = self.serial;
		self.stream.write_all(&message.encode())?;
		Ok(self.serial)
	}
}

/// The receiving half of a connection.
pub(crate) struct Receiver {
	stream: UnixStream,
	buffer: Vec<u8>,
}

impl Receiver {
	/// Waits for the next message, skipping well-framed ones this client
	/// cannot read. A broken frame ends the connection.
	pub(crate) fn receive(&mut self) -> io::Result<Message> {
		loop {
			let length = loop {
				if self.buffer.len() >= 16 {
					break message::frame_len(&self.buffer[..16])
						.ok_or(io::Error::from(io::ErrorKind::InvalidData))?;
				}
				self.fill()?;
			};
			while self.buffer.len() < length {
				self.fill()?;
			}
			let message = Message::decode(&self.buffer[..length]);
			self.buffer.drain(..length);
			if let Ok(message) = message {
				return Ok(message);
			}
		}
	}

	/// Makes [`Self::receive`] fail after waiting `timeout`.
	#[cfg(test)]
	pub(crate) fn set_timeout(&self, timeout: std::time::Duration) {
		self.stream.set_read_timeout(Some(timeout)).unwrap();
	}

	fn fill(&mut self) -> io::Result<()> {
		let mut chunk = [0; 4096];
		let read = self.stream.read(&mut chunk)?;
		if read == 0 {
			return Err(io::ErrorKind::UnexpectedEof.into());
		}
		self.buffer.extend_from_slice(&chunk[..read]);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn addresses_name_unix_sockets_in_order() {
		assert_eq!(
			targets(
				"tcp:host=x,port=1;unix:path=/run/user/1000/bu%73;unix:abstract=/tmp/dbus-%00x;unix:runtime=yes",
				Some("/run/user/7"),
			),
			[
				Target::Path("/run/user/1000/bus".into()),
				Target::Abstract(b"/tmp/dbus-\0x".to_vec()),
				Target::Path("/run/user/7/bus".into()),
			]
		);
		assert!(targets("unix:path=%zz;unix:tmpdir=/tmp", None).is_empty());
	}
}
