//! Requests sent as device control strings, collected until the string
//! ends: DECRQSS (`DCS $ q`), XTGETTCAP (`DCS + q`) and DECRSPS
//! (`DCS Ps $ t`).

/// Longest request kept; longer ones are answered as invalid.
pub(crate) const MAX_REQUEST: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
	Status,
	Capabilities,
	/// DECRSPS restoring the tab stops a DECTABSR reported.
	Presentation(u16),
}

#[derive(Clone, Debug)]
pub(crate) struct Request {
	pub kind: Kind,
	pub data: Vec<u8>,
	/// More than [`MAX_REQUEST`] bytes arrived.
	pub overflow: bool,
}

impl Request {
	pub(crate) fn new(kind: Kind) -> Self {
		Self {
			kind,
			data: Vec::new(),
			overflow: false,
		}
	}

	pub(crate) fn extend(&mut self, bytes: &[u8]) {
		if self.data.len() + bytes.len() > MAX_REQUEST {
			self.overflow = true;
			self.data = Vec::new();
		} else if !self.overflow {
			self.data.extend_from_slice(bytes);
		}
	}
}
