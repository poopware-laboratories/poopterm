//! Desktop notifications from sessions: whether to show them, delivering
//! them, and reporting activation and closing back to the program.
use std::time::Instant;

use super::*;
use crate::{
	notifier::{Notifier, RateLimit, Routes, wanted},
	term::notifications::{Event, Request},
};

#[derive(Default)]
pub(super) struct Notifications {
	/// Started with the first notification.
	notifier: Option<Notifier>,
	routes: Routes,
	rates: Vec<(u64, RateLimit)>,
}

impl Notifications {
	/// Forgets a closed session.
	pub(super) fn forget(&mut self, session: u64) {
		self.routes.remove_session(session);
		self.rates.retain(|(id, _)| *id != session);
	}
}

impl App {
	/// Shows or closes what session `id` asked for.
	pub(super) fn deliver_notifications(&mut self, id: u64, requests: Vec<Request>) {
		let visible = self.tab_of(id) == Some(self.active) && !self.occluded;
		let now = Instant::now();
		let mut refused = Vec::new();
		for request in requests {
			let state = &mut self.notifications;
			match request {
				Request::Show(notification) => {
					let rate = match state.rates.iter().position(|(session, _)| *session == id) {
						Some(index) => &mut state.rates[index].1,
						None => {
							state.rates.push((id, RateLimit::new(now)));
							&mut state.rates.last_mut().unwrap().1
						}
					};
					if !wanted(notification.occasion, self.focused, visible) || !rate.allow(now) {
						refused.push(notification.token);
						continue;
					}
					let (key, replaces) = state.routes.add(
						id,
						notification.token,
						notification.focus,
						notification.replaces,
					);
					let proxy = self.proxy.clone();
					state
						.notifier
						.get_or_insert_with(|| {
							Notifier::new(Box::new(move |key, event| {
								let _ = proxy.send_event(UserEvent::Notification(key, event));
							}))
						})
						.show(key, replaces, &notification);
				}
				Request::Close(token) => {
					if let (Some(key), Some(notifier)) = (state.routes.key(id, token), &state.notifier) {
						notifier.close(key);
					}
				}
			}
		}
		// Notifications not shown count as closed at once.
		for token in refused {
			self.report_notification(id, token, Event::Closed);
		}
	}

	/// Handles the desktop's report on the notification shown as `key`.
	pub(super) fn notification_event(&mut self, key: u64, event: Event) {
		let Some(route) = self.notifications.routes.get(key) else {
			return;
		};
		if event == Event::Closed {
			self.notifications.routes.remove(key);
		}
		self.report_notification(route.session, route.token, event);
		if matches!(event, Event::Activated(_))
			&& route.focus
			&& let Some(index) = self.tab_of(route.session)
		{
			self.activate_tab(index);
			self.focus_pane(route.session);
			if let Some(window) = &self.window {
				window.focus_window();
			}
		}
	}

	/// Tells the program in session `id` what happened, if it asked.
	fn report_notification(&mut self, id: u64, token: u64, event: Event) {
		let Some(session) = self.all_sessions().find(|session| session.id == id) else {
			return;
		};
		let replies = {
			let mut term = session.term();
			term.notification_event(token, event);
			std::mem::take(&mut term.responses)
		};
		for reply in replies {
			session.write(&reply);
		}
	}
}
