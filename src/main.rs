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

mod app;
mod bench;
#[cfg(target_os = "linux")]
mod cgroup;
#[cfg(target_os = "linux")]
mod cgroup_tree;
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
mod graphics;
mod hugepages;
mod inflate;
mod input;
mod notifier;
mod parser;
mod platform;
mod pty;
mod render;
mod session_store;
mod shell_integration;
mod sixel;
mod term;
mod terminfo;
mod terminfo_caps;
mod theme;
mod ui;
mod vt;
mod wallpaper;
mod width;

use std::{env, error::Error};

const HELP: &str =
	"Usage: poopterm [--version | --help | --print-terminfo | --print-shell-integration SHELL]
       poopterm --benchmark [--mib N] [--size COLSxROWS] [--only a,b] [--no-render] [--list]

Launches your login shell. Configure appearance and session restore with Settings.
POOPTERM_CONFIG overrides the config path; POOPTERM_THEME overrides the theme.

Print shell hooks with --print-shell-integration zsh|bash|fish.
Install the terminfo entry with: poopterm --print-terminfo | tic -x -";

fn main() -> Result<(), Box<dyn Error>> {
	let mut args = env::args().skip(1);
	let Some(argument) = args.next() else {
		return app::run();
	};
	match argument.as_str() {
		"--version" | "-V" => println!("poopterm {}", env!("CARGO_PKG_VERSION")),
		"--help" | "-h" => println!("Poopterm {}\n\n{HELP}", env!("CARGO_PKG_VERSION")),
		"--print-terminfo" => print!("{}", terminfo::SOURCE),
		"--benchmark" => bench::run(args)?,
		"--bench-corpus" => bench::print_corpus(args)?,
		"--print-shell-integration" => {
			let shell = args
				.next()
				.ok_or("missing shell name; choose zsh, bash, or fish")?;
			print!("{}", shell_integration::script(&shell)?);
		}
		_ => return Err(format!("unknown option: {argument}; try --help").into()),
	}
	Ok(())
}
