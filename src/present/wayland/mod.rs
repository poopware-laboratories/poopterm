//! Frames shown on a Wayland window through `wl_shm`, with
//! libwayland-client opened at run time.
#![expect(
	clippy::multiple_unsafe_ops_per_block,
	reason = "libwayland calls give way to our own wire protocol code, which removes this"
)]
pub(crate) mod client;
#[cfg(test)]
mod client_tests;
mod events;
#[cfg(test)]
mod events_tests;
#[cfg(test)]
mod fake_rules_tests;
#[cfg(test)]
mod fake_tests;
#[cfg(test)]
pub(crate) mod outside_tests;
pub(crate) mod presenter;
#[cfg(test)]
mod presenter_edge_tests;
#[cfg(test)]
mod presenter_globals_tests;
#[cfg(test)]
mod presenter_tests;
#[cfg(test)]
mod real_tests;
