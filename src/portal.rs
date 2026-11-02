//! Desktop settings read from the XDG settings portal: the window buttons
//! GNOME and the desktops that follow it choose, and on which side.
use std::{io, thread, time::Duration};

use crate::{
	dbus::{Connection, message::Message, wire::Value},
	ui::window_controls::ButtonLayout,
};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";

fn read(member: &str) -> Message {
	let mut call = Message::method_call(PORTAL, PATH, SETTINGS, member);
	call.body = ["org.gnome.desktop.wm.preferences", "button-layout"]
		.map(|text| Value::Str(text.into()))
		.to_vec();
	call
}

/// The string a reply carries: in a variant, which `Read` wraps in one more.
pub(crate) fn reply_text(body: &[Value]) -> Option<&str> {
	let mut value = body.first()?;
	for _ in 0..2 {
		if let Value::Variant(inner) = value {
			value = inner;
		}
	}
	value.as_str()
}

/// The desktop's button layout, asked with `ReadOne`, or with `Read` on
/// portals older than its version 2.
pub(crate) fn button_layout(connection: &mut Connection) -> io::Result<ButtonLayout> {
	let reply = connection
		.call(read("ReadOne"))
		.or_else(|_| connection.call(read("Read")))?;
	let text = reply_text(&reply.body).ok_or_else(|| io::Error::other("not a string"))?;
	Ok(ButtonLayout::parse(text))
}

/// Reads the layout from the bus at `address` (the session bus when
/// `None`) on its own thread, so a portal slow to start never holds up the
/// window, and hands `done` it, or the fallback when the desktop cannot say
/// within `timeout`.
pub(crate) fn read_button_layout(
	address: Option<String>,
	timeout: Duration,
	done: impl FnOnce(ButtonLayout) + Send + 'static,
) {
	let read = move || {
		let mut connection = match address {
			Some(address) => Connection::connect(&address, None)?,
			None => Connection::session()?,
		};
		connection.set_timeout(timeout);
		button_layout(&mut connection)
	};
	let spawned = thread::Builder::new()
		.name("desktop settings".into())
		.spawn(move || done(read().unwrap_or_else(|_| ButtonLayout::fallback())));
	if let Err(error) = spawned {
		eprintln!("poopterm: could not read the desktop's window buttons: {error}");
	}
}
