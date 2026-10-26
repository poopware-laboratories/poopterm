//! Frames shown on an X11 window: through MIT-SHM where the server shares
//! memory with us, else in `PutImage` requests, with libxcb opened at run
//! time.
pub(crate) mod client;
#[cfg(test)]
mod fake_tests;
pub(crate) mod presenter;
#[cfg(test)]
mod presenter_tests;
#[cfg(test)]
mod real_tests;
pub(crate) mod setup;
#[cfg(test)]
mod setup_tests;
