//! Applies escape sequences to a [`Term`].
//!
//! [`crate::vt`] tokenizes the byte stream; the [`Handler`] impl here routes
//! each event to the submodule that applies it.
mod ascii;
#[cfg(test)]
mod ascii_tests;
mod cluster;
mod color_spec;
mod csi;
mod dcs;
#[cfg(all(test, target_arch = "x86_64"))]
mod dispatch_tests;
mod erase;
mod esc;
mod modes;
mod narrow;
mod osc;
mod osc_clipboard;
mod osc_color;
mod osc_iterm;
mod osc_notify;
mod osc_pointer;
mod osc_shell;
mod osc_status;
mod osc_tab;
mod osc_text_size;
mod params;
mod print;
mod sgr;
#[cfg(test)]
mod sgr_tests;
mod window_ops;

use crate::{
	term::Term,
	vt::{self, Handler},
};
#[cfg(test)]
pub(crate) use color_spec::encode as encode_color;
use params::ParamList;

/// Stateful parser for one PTY stream. Keep it beside `Term` in a session so
/// escape sequences and UTF-8 characters split across reads remain intact.
#[derive(Default)]
pub(crate) struct Parser(vt::Parser);

impl Parser {
	pub(crate) fn new() -> Self {
		Self::default()
	}

	/// Feeds one PTY read to the terminal.
	#[inline]
	pub(crate) fn advance(&mut self, term: &mut Term, bytes: &[u8]) {
		self.0.advance(term, bytes);
	}
}

impl Handler for Term {
	#[inline(always)]
	fn print_ascii(&mut self, text: &[u8]) {
		if self.iterm.capture.is_some() {
			// Printable ASCII is UTF-8.
			self
				.iterm
				.capture(std::str::from_utf8(text).unwrap_or_default());
		}
		self.print_ascii(text);
	}
	#[inline(always)]
	fn print(&mut self, text: &str) {
		if self.iterm.capture.is_some() {
			self.iterm.capture(text);
		}
		self.print_text(text);
	}
	#[inline(always)]
	fn execute(&mut self, byte: u8) {
		if self.iterm.capture.is_some() && matches!(byte, b'\n' | 0x0b | 0x0c) {
			self.iterm.capture("\n");
		}
		self.apply_execute(byte);
	}
	#[inline(always)]
	fn csi_dispatch(&mut self, params: &vt::Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.dispatch_csi(params, intermediates, ignore, action as char);
	}
	#[inline(always)]
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self.apply_esc_dispatch(intermediates, ignore, byte);
	}
	#[inline(always)]
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.apply_osc_dispatch(params, bell);
	}
	#[inline(always)]
	fn hook(&mut self, params: &vt::Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.dispatch_hook(params, intermediates, ignore, action as char);
	}
	#[inline(always)]
	fn put(&mut self, bytes: &[u8]) {
		if let Some(transfer) = &mut self.sixel_transfer {
			transfer.extend(bytes);
		} else if let Some(request) = &mut self.dcs_request {
			request.extend(bytes);
		}
	}
	#[inline(always)]
	fn unhook(&mut self) {
		self.finish_sixel();
		if let Some(request) = self.dcs_request.take() {
			self.answer_dcs(request);
		}
	}
	fn apc_start(&mut self) {
		self.kitty_receiver.start();
	}
	fn apc_put(&mut self, bytes: &[u8]) {
		self.kitty_receiver.put(bytes);
	}
	fn apc_end(&mut self, complete: bool) {
		if let Some(transmission) = self.kitty_receiver.end(complete)
			&& let Some(reply) = self.apply_kitty(transmission)
		{
			self.responses.push(reply);
		}
	}
}

impl Term {
	fn dispatch_hook(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		use crate::term::dcs::{Kind, Request};
		let target = &mut *self;
		target.sixel_transfer = if !ignore && intermediates.is_empty() && action == 'q' {
			let mut values = params.iter().map(|p| p.first().copied().unwrap_or(0));
			Some(crate::sixel::Transfer::new(
				values.next().unwrap_or(0),
				values.next().unwrap_or(0),
			))
		} else {
			None
		};
		// DECRQSS and XTGETTCAP take no parameters.
		let kind = match (intermediates, action) {
			_ if ignore
				|| params
					.iter()
					.any(|param| param.iter().any(|&value| value != 0)) =>
			{
				None
			}
			(b"$", 'q') => Some(Kind::Status),
			(b"+", 'q') => Some(Kind::Capabilities),
			_ => None,
		};
		target.dcs_request = kind.map(Request::new);
	}

	#[inline(always)]
	fn dispatch_csi(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		if ignore {
			return;
		}
		// Most sequences have no intermediates; skip the checks below for them.
		if !intermediates.is_empty() {
			self.dispatch_mode_sequences(params, intermediates, action);
			if matches!(intermediates, b"?$" | b"$") && action == 'p' {
				return;
			}
		}
		self.apply_csi_dispatch(params, intermediates, false, action);
	}

	/// DECRQM (`CSI ? Ps $ p`, `CSI Ps $ p`) and synchronized output (`CSI ? 2026 h/l`),
	/// which the terminal answers itself.
	fn dispatch_mode_sequences(
		&mut self,
		params: &impl ParamList,
		intermediates: &[u8],
		action: char,
	) {
		if intermediates == b"?$" && action == 'p' {
			for param in params.iter() {
				let mode = param[0];
				let state = if mode == 2026 {
					Some(self.synchronized.is_some())
				} else {
					self.private_mode(mode)
				};
				let status = match state {
					Some(true) => 1,
					Some(false) => 2,
					None => 0,
				};
				self
					.responses
					.push(format!("\x1b[?{mode};{status}$y").into_bytes());
			}
			return;
		}
		if intermediates == b"$" && action == 'p' {
			let mode = params.get(0).map_or(0, |p| p[0]);
			let status = self
				.ansi_mode(mode)
				.map_or(0, |set| if set { 1 } else { 2 });
			self
				.responses
				.push(format!("\x1b[{mode};{status}$y").into_bytes());
			return;
		}
		if intermediates == b"?"
			&& matches!(action, 'h' | 'l')
			&& params.iter().any(|p| p.first() == Some(&2026))
		{
			if action == 'h' {
				self.begin_sync();
			} else {
				self.finish_sync();
			}
		}
	}
}

/// vte driving a [`Term`] one character at a time: the reference that the
/// batched paths are compared against in tests.
#[cfg(test)]
pub(crate) struct PerCharacter<'a>(pub &'a mut Term);

#[cfg(test)]
impl vte::Perform for PerCharacter<'_> {
	fn print(&mut self, c: char) {
		self.0.apply_print(c);
	}
	fn execute(&mut self, byte: u8) {
		self.0.execute(byte);
	}
	fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
		self.0.dispatch_hook(params, intermediates, ignore, action);
	}
	fn put(&mut self, byte: u8) {
		self.0.put(&[byte]);
	}
	fn unhook(&mut self) {
		self.0.unhook();
	}
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.0.osc_dispatch(params, bell);
	}
	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		self.0.dispatch_csi(params, intermediates, ignore, action);
	}
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self.0.esc_dispatch(intermediates, ignore, byte);
	}
}
