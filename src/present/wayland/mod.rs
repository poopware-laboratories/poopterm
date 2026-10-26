//! Frames shown on a Wayland window through `wl_shm`, with
//! libwayland-client opened at run time.
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
pub(crate) mod memory;
#[cfg(test)]
mod memory_tests;
#[cfg(test)]
pub(crate) mod outside_tests;
pub(crate) mod presenter;
#[cfg(test)]
mod presenter_edge_tests;
#[cfg(test)]
mod presenter_tests;
#[cfg(test)]
mod real_tests;
