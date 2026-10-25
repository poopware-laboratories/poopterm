//! Tests of the status bar model: what each end says, and in which colors.
use super::{
	header::TabPalette,
	status_bar::{SEPARATOR, StatusInput, status_bar, status_text},
};
use crate::{
	shell_integration::{Phase, ShellIntegration},
	theme::{Theme, ThemeKind},
};

fn input(shell: &ShellIntegration) -> StatusInput<'_> {
	StatusInput {
		link: None,
		place: None,
		cwd: "poopterm",
		shell,
		size: (80, 24),
		theme: "poop",
	}
}

fn shell(
	phase: Phase,
	command: Option<&str>,
	error: Option<&str>,
	exit: Option<i32>,
) -> ShellIntegration {
	let mut shell = ShellIntegration::default();
	shell.phase = phase;
	shell.command_line = command.map(str::to_owned);
	shell.last_error = error.map(str::to_owned);
	shell.last_exit_code = exit;
	shell
}

/// The right-hand text for `shell`.
fn right(shell: &ShellIntegration) -> String {
	status_text(&input(shell)).1
}

#[test]
fn the_left_shows_the_working_directory() {
	let shell = ShellIntegration::default();
	assert_eq!(status_text(&input(&shell)).0, "poopterm");
}

#[test]
fn a_place_comes_before_the_directory() {
	let shell = ShellIntegration::default();
	let input = StatusInput {
		place: Some("root@box"),
		..input(&shell)
	};
	assert_eq!(
		status_text(&input).0,
		format!("root@box{SEPARATOR}poopterm")
	);
}

#[test]
fn a_link_under_the_pointer_replaces_place_and_directory() {
	let shell = ShellIntegration::default();
	let input = StatusInput {
		link: Some("https://example.com"),
		place: Some("root@box"),
		..input(&shell)
	};
	assert_eq!(status_text(&input).0, "→ https://example.com");
}

#[test]
fn control_characters_never_reach_the_bar() {
	let shell = shell(Phase::Finished, None, Some("bad\x1b[31m\x07"), None);
	let input = StatusInput {
		place: Some("ro\not"),
		cwd: "di\r\x1br",
		theme: "po\top",
		..input(&shell)
	};
	let (left, right) = status_text(&input);
	assert_eq!(left, format!("root{SEPARATOR}dir"));
	assert_eq!(
		right,
		format!("80 × 24{SEPARATOR}poop{SEPARATOR}error bad[31m")
	);
	let link = StatusInput {
		link: Some("https://a\u{9b}b\x00c"),
		..input
	};
	assert_eq!(status_text(&link).0, "→ https://abc");
	let running = self::shell(Phase::Output, Some("ls\x1b]0;x\x07 -l"), None, None);
	assert_eq!(right_of(&running), "running ls]0;x -l");
}

fn right_of(shell: &ShellIntegration) -> String {
	right(shell).rsplit(SEPARATOR).next().unwrap().to_owned()
}

#[test]
fn the_right_shows_the_grid_size_and_theme() {
	let shell = ShellIntegration::default();
	let input = StatusInput {
		size: (132, 43),
		theme: "catppuccin-latte",
		..input(&shell)
	};
	assert_eq!(
		status_text(&input).1,
		format!("132 × 43{SEPARATOR}catppuccin-latte")
	);
}

#[test]
fn a_running_command_shows_its_line() {
	let shell = shell(Phase::Output, Some("cargo build"), None, Some(1));
	assert_eq!(
		right(&shell),
		format!("80 × 24{SEPARATOR}poop{SEPARATOR}running cargo build")
	);
}

#[test]
fn long_command_lines_are_cut_at_32_characters() {
	let exact = "x".repeat(32);
	let shell = self::shell(Phase::Output, Some(&exact), None, None);
	assert_eq!(right_of(&shell), format!("running {exact}"));
	let long = "y".repeat(33);
	let shell = self::shell(Phase::Output, Some(&long), None, None);
	assert_eq!(right_of(&shell), format!("running {}…", "y".repeat(32)));
}

#[test]
fn command_lines_are_cut_by_characters_not_bytes() {
	let wide = "é".repeat(32);
	let shell = shell(Phase::Output, Some(&wide), None, None);
	assert_eq!(right_of(&shell), format!("running {wide}"));
	let wider = "世".repeat(40);
	let shell = self::shell(Phase::Output, Some(&wider), None, None);
	assert_eq!(right_of(&shell), format!("running {}…", "世".repeat(32)));
}

#[test]
fn a_running_command_without_a_line_says_running() {
	let shell = shell(Phase::Output, None, Some("failed"), Some(2));
	assert_eq!(right_of(&shell), "running");
}

#[test]
fn after_a_command_its_error_shows_before_its_exit_code() {
	for phase in [
		Phase::Prompt,
		Phase::Command,
		Phase::Finished,
		Phase::Unknown,
	] {
		let failed = shell(phase, Some("make"), Some("oops"), Some(2));
		assert_eq!(right_of(&failed), "error oops", "{phase:?}");
		let exited = shell(phase, Some("make"), None, Some(2));
		assert_eq!(right_of(&exited), "exit 2", "{phase:?}");
	}
	assert_eq!(
		right_of(&shell(Phase::Prompt, None, None, Some(0))),
		"exit 0"
	);
}

#[test]
fn without_a_command_state_the_right_ends_at_the_theme() {
	let shell = ShellIntegration::default();
	assert_eq!(right(&shell), format!("80 × 24{SEPARATOR}poop"));
	let prompt = self::shell(Phase::Prompt, Some("ls"), None, None);
	assert_eq!(right(&prompt), format!("80 × 24{SEPARATOR}poop"));
}

#[test]
fn the_bar_takes_the_header_colors_with_dim_details() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let shell = ShellIntegration::default();
	let bar = status_bar(&input(&shell), (0, 500, 800, 26), (1, 12.5), &theme);
	let palette = TabPalette::new(&theme);
	assert_eq!(bar.rect, (0, 500, 800, 26));
	assert_eq!((bar.style.scale, bar.style.px), (1, 12.5));
	assert_eq!(bar.style.colors, palette.colors);
	assert_eq!(bar.left, ("poopterm".to_owned(), theme.fg));
	assert_eq!(bar.right, (format!("80 × 24{SEPARATOR}poop"), palette.dim));
}

#[test]
fn a_link_shows_in_the_accent_color() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let shell = ShellIntegration::default();
	let input = StatusInput {
		link: Some("https://example.com"),
		..input(&shell)
	};
	let bar = status_bar(&input, (0, 0, 800, 26), (2, 25.0), &theme);
	assert_eq!(bar.left.1, TabPalette::new(&theme).accent);
	assert_eq!(bar.style.scale, 2);
}

#[test]
fn every_theme_keeps_status_text_readable_on_the_bar() {
	use super::contrast::{TEXT, ratio};
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	for kind in kinds {
		let theme = Theme::from_kind(kind);
		let shell = ShellIntegration::default();
		let link = StatusInput {
			link: Some("x"),
			..input(&shell)
		};
		let plain = status_bar(&input(&shell), (0, 0, 100, 26), (1, 12.0), &theme);
		let linked = status_bar(&link, (0, 0, 100, 26), (1, 12.0), &theme);
		let bar = plain.style.colors.bar;
		for color in [plain.left.1, plain.right.1, linked.left.1] {
			assert!(
				ratio(color, bar) >= TEXT,
				"{kind:?}: {color:06x} on {bar:06x}"
			);
		}
	}
}

#[test]
fn a_directory_alone_loses_its_control_characters_too() {
	let shell = ShellIntegration::default();
	let input = StatusInput {
		cwd: "\x1b]8;;evil\x07src",
		..input(&shell)
	};
	assert_eq!(status_text(&input).0, "]8;;evilsrc");
}
