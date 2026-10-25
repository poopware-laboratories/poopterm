//! Desktop notifications through the freedesktop.org Notifications service
//! on the session bus, with our own D-Bus client. A thread sends requests;
//! another reads replies and the service's activation and close signals.
use std::{
	io,
	path::PathBuf,
	sync::{Arc, Condvar, Mutex, mpsc},
	thread,
	time::Duration,
};

use super::Report;
use crate::{
	dbus::{
		Connection, Sender,
		message::{Kind, Message},
		wire::Value,
	},
	term::notifications::{Event, Notification, Urgency},
};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
/// Most notifications whose service ids are remembered.
const MAX_TRACKED: usize = 256;
/// Longest wait for the id of a notification an update replaces.
const REPLY_WAIT: Duration = Duration::from_secs(1);
/// Most icon files kept on disk.
const MAX_ICON_FILES: usize = 64;

enum Command {
	Show {
		key: u64,
		replaces: Option<u64>,
		notification: Box<Notification>,
	},
	Close(u64),
}

/// Sends notifications to the service; `key`s name them in reports.
pub(crate) struct Freedesktop {
	commands: mpsc::Sender<Command>,
}

impl Freedesktop {
	/// Connects in the background to the bus at `address`, or the session
	/// bus; without one, notifications are dropped.
	pub(crate) fn start(address: Option<String>, report: Report) -> Self {
		let (commands, receiver) = mpsc::channel();
		let _ = thread::Builder::new()
			.name("notifications".into())
			.spawn(move || run(address, receiver, report));
		Self { commands }
	}

	pub(crate) fn show(&self, key: u64, replaces: Option<u64>, notification: &Notification) {
		let _ = self.commands.send(Command::Show {
			key,
			replaces,
			notification: Box::new(notification.clone()),
		});
	}

	pub(crate) fn close(&self, key: u64) {
		let _ = self.commands.send(Command::Close(key));
	}
}

/// Which service notification each key is, and calls awaiting replies.
#[derive(Default)]
struct Tracked {
	/// Notify calls by serial.
	pending: Vec<(u32, u64)>,
	/// Shown notifications by service id.
	shown: Vec<(u32, u64)>,
	/// The service's unique bus name, from its replies: only its signals
	/// are believed.
	service: Option<String>,
}

fn run(
	address: Option<String>,
	commands: mpsc::Receiver<Command>,
	report: Report,
) -> io::Result<()> {
	let mut connection = match address {
		Some(address) => Connection::connect(&address, None)?,
		None => Connection::session()?,
	};
	let markup = connection
		.call(Message::method_call(
			SERVICE,
			PATH,
			SERVICE,
			"GetCapabilities",
		))
		.is_ok_and(|reply| match reply.body.first() {
			Some(Value::Array(_, names)) => names
				.iter()
				.any(|name| name.as_str() == Some("body-markup")),
			_ => false,
		});
	connection.add_match(&format!(
		"type='signal',interface='{SERVICE}',path='{PATH}'"
	))?;
	let (mut sender, mut receiver) = connection.split();
	let shared = Arc::new((Mutex::new(Tracked::default()), Condvar::new()));
	let reader = shared.clone();
	thread::Builder::new()
		.name("notification events".into())
		.spawn(move || {
			while let Ok(message) = receiver.receive() {
				handle(&message, &reader.0, &report);
				reader.1.notify_all();
			}
		})?;
	let (tracked, replied) = &*shared;
	for command in commands {
		let mut tracked = tracked.lock().unwrap();
		// Updates and closes need the id of the notification they act on.
		let earlier = match command {
			Command::Show { replaces, .. } => replaces,
			Command::Close(key) => Some(key),
		};
		if let Some(earlier) = earlier {
			tracked = replied
				.wait_timeout_while(tracked, REPLY_WAIT, |tracked| {
					tracked.pending.iter().any(|(_, key)| *key == earlier)
				})
				.unwrap()
				.0;
		}
		match command {
			Command::Show {
				key,
				replaces,
				notification,
			} => {
				let replaces = replaces
					.and_then(|replaces| tracked.shown.iter().find(|(_, key)| *key == replaces))
					.map_or(0, |(id, _)| *id);
				let icon = notification.icon.as_deref().and_then(icon_file);
				let call = notify_call(&notification, replaces, markup, icon);
				let serial = sender.send(call)?;
				if tracked.pending.len() >= MAX_TRACKED {
					tracked.pending.remove(0);
				}
				tracked.pending.push((serial, key));
			}
			Command::Close(key) => close(&mut sender, &tracked, key)?,
		}
	}
	Ok(())
}

fn close(sender: &mut Sender, tracked: &Tracked, key: u64) -> io::Result<()> {
	if let Some((id, _)) = tracked.shown.iter().find(|(_, shown)| *shown == key) {
		let mut call = Message::method_call(SERVICE, PATH, SERVICE, "CloseNotification");
		call.body = vec![Value::U32(*id)];
		sender.send(call)?;
	}
	Ok(())
}

/// Applies a reply or signal from the bus.
fn handle(message: &Message, tracked: &Mutex<Tracked>, report: &Report) {
	let mut tracked = tracked.lock().unwrap();
	match message.kind {
		Kind::Return | Kind::Error => {
			let Some(index) = tracked
				.pending
				.iter()
				.position(|(serial, _)| Some(*serial) == message.reply_serial)
			else {
				return;
			};
			let (_, key) = tracked.pending.remove(index);
			if let Some(id) = message.body.first().and_then(Value::as_u32) {
				tracked.service.clone_from(&message.sender);
				tracked.shown.retain(|&(shown, _)| shown != id);
				if tracked.shown.len() >= MAX_TRACKED {
					tracked.shown.remove(0);
				}
				tracked.shown.push((id, key));
			}
		}
		Kind::Signal if message.sender.is_some() && message.sender == tracked.service => {
			let Some(id) = message.body.first().and_then(Value::as_u32) else {
				return;
			};
			let Some(index) = tracked.shown.iter().position(|&(shown, _)| shown == id) else {
				return;
			};
			let key = tracked.shown[index].1;
			match (message.member.as_deref(), message.body.get(1)) {
				(Some("ActionInvoked"), Some(Value::Str(action))) => {
					let button = match action.as_str() {
						"default" => None,
						number => match number.parse() {
							Ok(button) => Some(button),
							Err(_) => return,
						},
					};
					report(key, Event::Activated(button));
				}
				(Some("NotificationClosed"), _) => {
					tracked.shown.remove(index);
					report(key, Event::Closed);
				}
				_ => {}
			}
		}
		_ => {}
	}
}

/// The `Notify` call showing `notification`.
fn notify_call(
	notification: &Notification,
	replaces: u32,
	markup: bool,
	icon: Option<PathBuf>,
) -> Message {
	let text = |text: &str| Value::Str(text.into());
	let hint = |name: &str, value: Value| {
		Value::DictEntry(
			Box::new(text(name)),
			Box::new(Value::Variant(Box::new(value))),
		)
	};
	let urgency = match notification.urgency {
		Urgency::Low => 0,
		Urgency::Normal => 1,
		Urgency::Critical => 2,
	};
	let mut hints = vec![
		hint("urgency", Value::Byte(urgency)),
		hint("desktop-entry", text("poopterm")),
	];
	if let Some(category) = notification.types.first() {
		hints.push(hint("category", text(category)));
	}
	match notification.sound.as_deref() {
		None | Some("system") => {}
		Some("silent") => hints.push(hint("suppress-sound", Value::Bool(true))),
		Some(sound) => hints.push(hint("sound-name", text(standard_sound(sound)))),
	}
	let icon_name = notification
		.icon_names
		.first()
		.map(|name| standard_icon(name).to_owned())
		.or_else(|| notification.app.clone());
	if icon_name.is_none()
		&& let Some(path) = icon
	{
		hints.push(hint("image-path", text(&path.to_string_lossy())));
	}
	let mut actions = vec![text("default"), text("")];
	for (number, label) in notification.buttons.iter().enumerate() {
		actions.extend([text(&(number + 1).to_string()), text(label)]);
	}
	let body = if markup {
		escape_markup(&notification.body)
	} else {
		notification.body.clone()
	};
	let expire = notification
		.expire
		.map_or(-1, |millis| i32::try_from(millis).unwrap_or(i32::MAX));
	let mut call = Message::method_call(SERVICE, PATH, SERVICE, "Notify");
	call.body = vec![
		text("poopterm"),
		Value::U32(replaces),
		text(icon_name.as_deref().unwrap_or("poopterm")),
		text(&notification.title),
		Value::Str(body),
		Value::Array("s".into(), actions),
		Value::Array("{sv}".into(), hints),
		Value::I32(expire),
	];
	call
}

/// Theme icon names for the protocol's well-known names.
fn standard_icon(name: &str) -> &str {
	match name {
		"error" => "dialog-error",
		"warn" | "warning" => "dialog-warning",
		"info" => "dialog-information",
		"question" => "dialog-question",
		"help" => "help-browser",
		"file-manager" => "system-file-manager",
		"system-monitor" => "utilities-system-monitor",
		"text-editor" => "accessories-text-editor",
		other => other,
	}
}

/// Sound theme names for the protocol's well-known sounds.
fn standard_sound(name: &str) -> &str {
	match name {
		"error" => "dialog-error",
		"warn" | "warning" => "dialog-warning",
		"info" => "dialog-information",
		"question" => "dialog-question",
		other => other,
	}
}

fn escape_markup(text: &str) -> String {
	text
		.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
}

/// Writes icon data to a private file the service can read, named by its
/// content so repeated icons share one.
fn icon_file(data: &[u8]) -> Option<PathBuf> {
	use std::{
		hash::{DefaultHasher, Hash, Hasher},
		os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
	};
	let base = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
	// SAFETY: getuid takes no arguments and cannot fail.
	let dir = base.join(format!("poopterm-icons-{}", unsafe { libc::getuid() }));
	std::fs::DirBuilder::new()
		.recursive(true)
		.mode(0o700)
		.create(&dir)
		.ok()?;
	// The directory may be in a shared place: use it only if it is ours.
	let owner = std::fs::symlink_metadata(&dir).ok()?;
	// SAFETY: getuid takes no arguments and cannot fail.
	if !owner.is_dir() || owner.uid() != unsafe { libc::getuid() } || owner.mode() & 0o077 != 0 {
		return None;
	}
	let mut hasher = DefaultHasher::new();
	data.hash(&mut hasher);
	let path = dir.join(format!("{:016x}", hasher.finish()));
	if !path.exists() {
		if std::fs::read_dir(&dir).ok()?.count() >= MAX_ICON_FILES {
			for entry in std::fs::read_dir(&dir).ok()?.flatten() {
				let _ = std::fs::remove_file(entry.path());
			}
		}
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.mode(0o600)
			.open(&path)
			.ok()?;
		std::io::Write::write_all(&mut file, data).ok()?;
	}
	Some(path)
}
