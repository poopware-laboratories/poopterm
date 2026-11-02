//! The settings portal reader against a fake portal on a private bus: the
//! call it makes, the older `Read` it falls back to, the replies it takes
//! apart, and the fallback layout whenever the desktop cannot say.
use std::{
	sync::mpsc,
	thread,
	time::{Duration, Instant},
};

use crate::{
	dbus::{
		Connection, Sender,
		message::{Kind, Message},
		test_bus::TestBus,
		wire::Value,
	},
	portal::{button_layout, read_button_layout, reply_text},
	ui::window_controls::{ButtonLayout, WindowButton::*},
};

const PORTAL: &str = "org.freedesktop.portal.Desktop";

fn text(text: &str) -> Value {
	Value::Str(text.into())
}

fn variant(value: Value) -> Value {
	Value::Variant(Box::new(value))
}

fn answer(to: &Message, kind: Kind, body: Vec<Value>) -> Message {
	let mut reply = Message::method_call("", "/", "", "");
	(reply.kind, reply.path, reply.interface, reply.member) = (kind, None, None, None);
	if kind == Kind::Error {
		reply.error_name = Some("org.freedesktop.DBus.Error.UnknownMethod".into());
	}
	reply.destination = to.sender.clone();
	reply.reply_serial = Some(to.serial);
	reply.body = body;
	reply
}

/// A portal on `bus` that answers each call with what `respond` returns
/// for it, `None` meaning an error; it hands back the calls it saw.
fn portal(
	bus: &TestBus,
	respond: impl Fn(&Message) -> Option<Option<Value>> + Send + 'static,
) -> mpsc::Receiver<Message> {
	let mut server = Connection::connect(&bus.address, None).unwrap();
	let mut request = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"RequestName",
	);
	request.body = vec![text(PORTAL), Value::U32(0)];
	server.call(request).unwrap();
	let (mut tx, mut rx) = server.split();
	rx.set_timeout(Duration::from_secs(5));
	let (seen_tx, seen) = mpsc::channel();
	thread::spawn(move || {
		while let Ok(message) = rx.receive() {
			if message.kind != Kind::MethodCall {
				continue;
			}
			let reply = match respond(&message) {
				Some(Some(value)) => answer(&message, Kind::Return, vec![value]),
				Some(None) => answer(&message, Kind::Error, Vec::new()),
				// Never answered.
				None => {
					let _ = seen_tx.send(message);
					continue;
				}
			};
			send(&mut tx, reply);
			let _ = seen_tx.send(message);
		}
	});
	seen
}

fn send(tx: &mut Sender, message: Message) {
	tx.send(message).unwrap();
}

fn member(message: &Message) -> &str {
	message.member.as_deref().unwrap_or_default()
}

#[test]
fn read_one_asks_the_portal_for_gnomes_button_layout() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let seen = portal(&bus, |_| {
		Some(Some(variant(text("close,minimize:maximize"))))
	});
	let mut client = Connection::connect(&bus.address, None).unwrap();
	let layout = button_layout(&mut client).unwrap();
	assert_eq!(layout.left, [Close, Minimize]);
	assert_eq!(layout.right, [Maximize]);
	let call = seen.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(member(&call), "ReadOne");
	assert_eq!(call.destination.as_deref(), Some(PORTAL));
	assert_eq!(
		call.path.as_deref(),
		Some("/org/freedesktop/portal/desktop")
	);
	assert_eq!(
		call.interface.as_deref(),
		Some("org.freedesktop.portal.Settings")
	);
	assert_eq!(
		call.body,
		[
			text("org.gnome.desktop.wm.preferences"),
			text("button-layout")
		]
	);
}

#[test]
fn portals_older_than_read_one_are_asked_with_read() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	// Version 1 portals have only Read, whose reply wraps one more variant.
	let seen = portal(&bus, |call| match member(call) {
		"ReadOne" => Some(None),
		_ => Some(Some(variant(variant(text("appmenu:close"))))),
	});
	let mut client = Connection::connect(&bus.address, None).unwrap();
	let layout = button_layout(&mut client).unwrap();
	assert_eq!((layout.left, layout.right), (vec![], vec![Close]));
	// The portal may note a call after replying to it: wait for each.
	let calls: Vec<_> = (0..2)
		.map(|_| seen.recv_timeout(Duration::from_secs(5)).unwrap())
		.map(|call| member(&call).to_owned())
		.collect();
	assert_eq!(calls, ["ReadOne", "Read"]);
	assert!(seen.try_recv().is_err(), "no third call");
}

#[test]
fn a_portal_without_the_setting_is_an_error() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| Some(None));
	let mut client = Connection::connect(&bus.address, None).unwrap();
	assert!(button_layout(&mut client).is_err());
}

#[test]
fn a_reply_that_is_not_a_string_is_an_error() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| Some(Some(variant(Value::U32(3)))));
	let mut client = Connection::connect(&bus.address, None).unwrap();
	assert!(button_layout(&mut client).is_err());
}

#[test]
fn the_reader_thread_hands_over_the_desktops_layout() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| Some(Some(variant(text(":close")))));
	let (tx, rx) = mpsc::channel();
	read_button_layout(
		Some(bus.address.clone()),
		Duration::from_secs(5),
		move |layout| tx.send(layout).unwrap(),
	);
	let layout = rx.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!((layout.left, layout.right), (vec![], vec![Close]));
}

#[test]
fn the_reader_thread_falls_back_when_the_portal_cannot_say() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| Some(None));
	let (tx, rx) = mpsc::channel();
	read_button_layout(
		Some(bus.address.clone()),
		Duration::from_secs(5),
		move |layout| tx.send(layout).unwrap(),
	);
	let layout = rx.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(layout, ButtonLayout::fallback());
}

#[test]
fn the_reader_thread_falls_back_without_a_bus() {
	let (tx, rx) = mpsc::channel();
	let nowhere = "unix:path=/nonexistent/poopterm/bus".to_owned();
	read_button_layout(Some(nowhere), Duration::from_secs(5), move |layout| {
		tx.send(layout).unwrap()
	});
	let layout = rx.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(layout, ButtonLayout::fallback());
}

#[test]
fn a_portal_that_never_answers_times_out_into_the_fallback() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| None);
	let (tx, rx) = mpsc::channel();
	let started = Instant::now();
	read_button_layout(
		Some(bus.address.clone()),
		Duration::from_millis(200),
		move |layout| tx.send(layout).unwrap(),
	);
	let layout = rx.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(layout, ButtonLayout::fallback());
	assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn replies_hold_the_string_in_at_most_two_variants() {
	assert_eq!(reply_text(&[variant(text("a:b"))]), Some("a:b"));
	assert_eq!(reply_text(&[variant(variant(text("a:b")))]), Some("a:b"));
	assert_eq!(reply_text(&[text("a:b")]), Some("a:b"));
	assert_eq!(reply_text(&[variant(variant(variant(text("a:b"))))]), None);
	assert_eq!(reply_text(&[variant(Value::Bool(true))]), None);
	assert_eq!(reply_text(&[]), None);
	// Only the first value counts.
	assert_eq!(reply_text(&[Value::U32(1), text("a:b")]), None);
}

#[test]
fn a_desktop_that_asks_for_no_buttons_gets_none() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	// An empty button-layout is a choice, not a failure: no fallback.
	let _seen = portal(&bus, |_| Some(Some(variant(text("")))));
	let mut client = Connection::connect(&bus.address, None).unwrap();
	assert_eq!(button_layout(&mut client).unwrap(), ButtonLayout::default());
}

#[test]
fn a_zero_timeout_is_refused_by_the_socket_and_waits_instead() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _seen = portal(&bus, |_| Some(Some(variant(text("close:")))));
	let mut client = Connection::connect(&bus.address, None).unwrap();
	client.set_timeout(Duration::ZERO);
	let layout = button_layout(&mut client).unwrap();
	assert_eq!(layout.left, [Close]);
}

#[test]
fn only_the_layout_reply_counts_when_other_messages_arrive_first() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	// The portal emits a signal before replying; the reader skips it.
	let mut server = Connection::connect(&bus.address, None).unwrap();
	let mut request = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"RequestName",
	);
	request.body = vec![text(PORTAL), Value::U32(0)];
	server.call(request).unwrap();
	let (mut tx, mut rx) = server.split();
	rx.set_timeout(Duration::from_secs(5));
	thread::spawn(move || {
		let call = rx.receive().unwrap();
		let mut noise = Message::method_call(
			"",
			"/org/freedesktop/portal/desktop",
			"org.freedesktop.portal.Settings",
			"SettingChanged",
		);
		(noise.kind, noise.destination) = (Kind::Signal, call.sender.clone());
		noise.body = vec![
			text("org.gnome.desktop.wm.preferences"),
			text("button-layout"),
			variant(text("close:")),
		];
		send(&mut tx, noise);
		let reply = answer(&call, Kind::Return, vec![variant(text(":minimize,close"))]);
		send(&mut tx, reply);
	});
	let mut client = Connection::connect(&bus.address, None).unwrap();
	let layout = button_layout(&mut client).unwrap();
	assert_eq!((layout.left, layout.right), (vec![], vec![Minimize, Close]));
}
