#![feature(portable_simd)]
//! Poopterm: a small software-rendered terminal emulator.
//!
//! Module map (see `docs/architecture.md` for the full picture):
//! - `term`, `parser`: terminal emulation core (grid state and escape sequences).
//! - `pty`: shell processes and the background output reader.
//! - `app`: winit event loop, input handling and frame composition.
//! - `render`, `ui`, `input`: pure helpers for pixels, layout and key encoding.

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "macos")]
mod about_menu;
mod allocator;
mod app;
mod bench;
#[cfg(test)]
mod busy_tests;
#[cfg(target_os = "linux")]
mod cgroup;
#[cfg(target_os = "linux")]
mod cgroup_tree;
mod cli;
mod clipboard;
#[cfg(test)]
#[path = "../build/codename.rs"]
mod codename;
mod config;
#[cfg(target_arch = "x86_64")]
mod cpu;
#[cfg(not(target_os = "macos"))]
mod dbus;
mod fonts;
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "the GPU renderers read it next (#30)")
)]
mod gpu;
mod graphics;
mod inflate;
mod input;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod library;
mod mapped;
mod notifier;
#[cfg(target_os = "macos")]
#[cfg_attr(
	not(test),
	expect(
		dead_code,
		reason = "the Metal renderer draws windows with it next (#26)"
	)
)]
mod objc;
mod parser;
mod platform;
#[cfg(test)]
mod platform_tests;
#[cfg(not(target_os = "macos"))]
mod portal;
#[cfg(all(test, not(target_os = "macos")))]
mod portal_tests;
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "the Wayland present path uses it next")
)]
mod present;
mod pty;
mod render;
#[cfg(test)]
mod scratch_tests;
mod session_store;
mod shell_integration;
mod sixel;
mod term;
mod terminfo;
mod terminfo_caps;
mod theme;
mod theme_credits;
mod theme_more;
#[cfg(test)]
mod theme_palette_tests;
mod ui;
mod vt;
mod wallpaper;
mod warnings;
#[cfg(test)]
mod warnings_property_tests;
#[cfg(test)]
mod warnings_tests;
mod width;

use cli::{Command, Paint};
use std::{
	env,
	error::Error,
	io::{stderr, stdout},
	process::ExitCode,
};

fn main() -> ExitCode {
	allocator::purge_eagerly();
	match run() {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprint!("{}", cli::error(&error.to_string(), Paint::of(&stderr())));
			ExitCode::FAILURE
		}
	}
}

fn run() -> Result<(), Box<dyn Error>> {
	let paint = Paint::of(&stdout());
	match cli::parse(env::args().skip(1))? {
		Command::Run => app::run()?,
		Command::Help => print!("{}", cli::help(paint)),
		Command::Version => println!("{}", cli::version(paint)),
		Command::PrintTerminfo => print!("{}", terminfo::SOURCE),
		Command::ShellIntegration(shell) => print!("{}", shell_integration::script(&shell)?),
		Command::Benchmark(args) => bench::run(args.into_iter())?,
		Command::BenchCorpus(args) => bench::print_corpus(args.into_iter())?,
	}
	Ok(())
}
