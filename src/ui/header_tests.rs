//! Tests of the header model: placement, fills, badges, close buttons,
//! progress, buttons and colors.
use super::{
	contrast::{TEXT, ratio, tab_text_colors},
	header::{HeaderInput, TabInfo, header, header_colors},
	tabs::{Marker, TabLayout, TabStripOptions, tab_layout_with_compaction},
};
use crate::{
	render::{
		blend::blend,
		header::{Badge, Header, Icon, close_rect},
	},
	term::{
		status::{Progress, ProgressState},
		tab_style::TabStyle,
	},
	theme::{Theme, ThemeKind},
};

const BOUNDS: (usize, usize) = (0, 44);

fn theme() -> Theme {
	Theme::from_kind(ThemeKind::MacOsDark)
}

fn layout(count: usize, active: usize, show_buttons: bool) -> TabLayout {
	tab_layout_with_compaction(
		1000,
		count,
		1,
		TabStripOptions {
			left_reserve: 0,
			scroll: 0,
			show_plus: show_buttons,
			show_settings: show_buttons,
			active_tab: active,
			compact_tabs: false,
		},
	)
}

fn info(title: &str) -> TabInfo {
	TabInfo {
		title: title.to_owned(),
		style: TabStyle::default(),
		marker: None,
		progress: None,
	}
}

/// The header for `tabs`, all visible, with the pointer at `cursor`.
fn build<'a>(
	layout: &'a TabLayout,
	tabs: &'a [TabInfo],
	active: usize,
	cursor: (f64, f64),
	theme: &'a Theme,
) -> Header<'a> {
	header(&HeaderInput {
		layout,
		tabs,
		count: tabs.len(),
		active,
		cursor,
		bounds: BOUNDS,
		px: 14.0,
		theme,
	})
}

const AWAY: (f64, f64) = (-1.0, -1.0);

#[test]
fn tabs_take_their_places_and_titles_from_the_layout() {
	let (layout, theme) = (layout(3, 1, true), theme());
	let tabs = [info("one"), info("two"), info("three")];
	let header = build(&layout, &tabs, 1, AWAY, &theme);
	assert_eq!(header.tabs.len(), 3);
	for (index, tab) in header.tabs.iter().enumerate() {
		assert_eq!(
			(tab.x, tab.width),
			(layout.tab_x(index), layout.tab_width(index))
		);
		assert_eq!(tab.label.title, tabs[index].title);
	}
	assert_eq!(
		(header.y, header.height, header.scale, header.px),
		(0, 44, 1, 14.0)
	);
}

#[test]
fn the_active_tab_is_raised_and_a_hovered_one_lit() {
	let (layout, theme) = (layout(3, 1, true), theme());
	let tabs = [info("one"), info("two"), info("three")];
	let colors = header_colors(&theme);
	let over_first = ((layout.tab_x(0) + 20) as f64, 20.0);
	let header = build(&layout, &tabs, 1, over_first, &theme);
	let fills: Vec<_> = header.tabs.iter().map(|tab| tab.fill).collect();
	assert_eq!(fills, [Some(colors.hover), Some(colors.raised), None]);
}

#[test]
fn a_tab_color_tints_the_tab_more_when_active_or_hovered() {
	let (layout, theme) = (layout(3, 1, true), theme());
	let mut tabs = [info("one"), info("two"), info("three")];
	for tab in &mut tabs {
		tab.style.color = Some(0xff0000);
	}
	let bar = header_colors(&theme).bar;
	let over_third = ((layout.tab_x(2) + 20) as f64, 20.0);
	let header = build(&layout, &tabs, 1, over_third, &theme);
	let fills: Vec<_> = header.tabs.iter().map(|tab| tab.fill).collect();
	let tint = |shade| Some(blend(bar, 0xff0000, shade));
	assert_eq!(fills, [tint(80), tint(150), tint(110)]);
}

#[test]
fn close_buttons_show_on_the_active_and_hovered_tabs_only() {
	let (layout, theme) = (layout(3, 0, true), theme());
	let tabs = [info("one"), info("two"), info("three")];
	let over_second = ((layout.tab_x(1) + 20) as f64, 20.0);
	let header = build(&layout, &tabs, 0, over_second, &theme);
	let closes: Vec<_> = header.tabs.iter().map(|tab| tab.close).collect();
	assert_eq!(closes, [Some(false), Some(false), None]);
	// On the close button itself, it lights up.
	let (x, y, w, h) = close_rect(layout.tab_x(1), layout.tab_width(1), BOUNDS, 1);
	let on_close = ((x + w / 2) as f64, (y + h / 2) as f64);
	let header = build(&layout, &tabs, 0, on_close, &theme);
	assert_eq!(header.tabs[1].close, Some(true));
	assert_eq!(header.tabs[0].close, Some(false));
}

#[test]
fn a_lone_tab_has_no_close_button() {
	let (layout, theme) = (layout(1, 0, true), theme());
	let tabs = [info("only")];
	let on_tab = ((layout.tab_x(0) + 20) as f64, 20.0);
	assert_eq!(build(&layout, &tabs, 0, on_tab, &theme).tabs[0].close, None);
}

#[test]
fn markers_become_badges_in_their_colors() {
	let (layout, theme) = (layout(1, 0, true), theme());
	let (dim, accent) = tab_text_colors(&theme);
	let palette = theme.palette;
	for (marker, badge) in [
		(Marker::Bell, Badge::Bell(accent)),
		(Marker::Blocked, Badge::Dot(palette[3])),
		(Marker::Error, Badge::Dot(palette[1])),
		(Marker::Done, Badge::Dot(palette[2])),
		(Marker::Working, Badge::Dot(accent)),
		(Marker::Activity, Badge::Dot(dim)),
	] {
		let mut tab = info("t");
		tab.marker = Some(marker);
		// The terminal's own marker wins over a program's indicator.
		tab.style.indicator = Some(0x123456);
		let tabs = [tab];
		let header = build(&layout, &tabs, 0, AWAY, &theme);
		assert_eq!(header.tabs[0].badge, Some(badge), "{marker:?}");
	}
	let mut tab = info("t");
	tab.style.indicator = Some(0x123456);
	let tabs = [tab, info("plain")];
	let header = build(&layout, &tabs[..1], 0, AWAY, &theme);
	assert_eq!(header.tabs[0].badge, Some(Badge::Dot(0x123456)));
	let header = build(&layout, &tabs[1..], 0, AWAY, &theme);
	assert_eq!(header.tabs[0].badge, None);
}

#[test]
fn statuses_keep_their_color_or_take_the_dim_one() {
	let (layout, theme) = (layout(2, 0, true), theme());
	let (dim, _) = tab_text_colors(&theme);
	let mut colored = info("build");
	colored.style.status = Some("ok".into());
	colored.style.status_color = Some(0x00ff00);
	let mut plain = info("vim");
	plain.style.status = Some("main".into());
	let tabs = [colored, plain];
	let header = build(&layout, &tabs, 0, AWAY, &theme);
	assert_eq!(header.tabs[0].label.status, Some("ok"));
	assert_eq!(header.tabs[0].label.colors, (theme.fg, 0x00ff00));
	assert_eq!(header.tabs[1].label.colors, (theme.fg, dim));
}

#[test]
fn progress_takes_the_color_of_its_state() {
	let (layout, theme) = (layout(1, 0, true), theme());
	let palette = theme.palette;
	for (state, percent, color) in [
		(ProgressState::Normal, Some(40), palette[4]),
		(ProgressState::Indeterminate, None, palette[4]),
		(ProgressState::Error, Some(90), palette[1]),
		(ProgressState::Paused, Some(10), palette[3]),
	] {
		let mut tab = info("t");
		tab.progress = Some(Progress { state, percent });
		let tabs = [tab];
		let header = build(&layout, &tabs, 0, AWAY, &theme);
		assert_eq!(header.tabs[0].progress, Some((percent, color)), "{state:?}");
	}
}

#[test]
fn the_new_tab_and_menu_buttons_follow_the_layout() {
	let theme = theme();
	let tabs = [info("one")];
	let shown = layout(1, 0, true);
	let header = build(&shown, &tabs, 0, AWAY, &theme);
	let icons: Vec<_> = header.buttons.iter().map(|button| button.icon).collect();
	assert_eq!(icons, [Icon::Plus, Icon::Menu]);
	let plus = header.buttons[0].rect;
	assert_eq!((plus.0, plus.2), (shown.plus_x, shown.plus_w));
	assert_eq!((plus.1, plus.3), (6, 32), "centered in the header");
	assert_eq!(header.buttons[1].rect.0, shown.settings_x);
	assert!(
		header
			.buttons
			.iter()
			.all(|button| button.enabled && !button.hovered)
	);
	let hidden = layout(1, 0, false);
	assert!(build(&hidden, &tabs, 0, AWAY, &theme).buttons.is_empty());
}

#[test]
fn a_button_under_the_pointer_is_hovered() {
	let (layout, theme) = (layout(1, 0, true), theme());
	let tabs = [info("one")];
	let on_plus = ((layout.plus_x + 4) as f64, 20.0);
	let header = build(&layout, &tabs, 0, on_plus, &theme);
	let hovered: Vec<_> = header.buttons.iter().map(|button| button.hovered).collect();
	assert_eq!(hovered, [true, false]);
}

#[test]
fn overflowing_tabs_add_arrows_enabled_toward_hidden_tabs() {
	let theme = theme();
	let layout = layout(30, 0, true);
	let tabs: Vec<_> = (0..layout.visible_count).map(|_| info("t")).collect();
	let header = header(&HeaderInput {
		layout: &layout,
		tabs: &tabs,
		count: 30,
		active: 0,
		cursor: AWAY,
		bounds: BOUNDS,
		px: 14.0,
		theme: &theme,
	});
	let arrows: Vec<_> = header
		.buttons
		.iter()
		.filter(|button| matches!(button.icon, Icon::Left | Icon::Right))
		.map(|button| (button.icon, button.rect.0, button.enabled))
		.collect();
	assert_eq!(
		arrows,
		[
			(Icon::Left, layout.left_arrow_x, false),
			(Icon::Right, layout.right_arrow_x, true)
		]
	);
	let two = self::layout(2, 0, true);
	let few = super::header::header(&HeaderInput {
		layout: &two,
		tabs: &tabs[..2],
		count: 2,
		active: 0,
		cursor: AWAY,
		bounds: BOUNDS,
		px: 14.0,
		theme: &theme,
	});
	assert!(
		few
			.buttons
			.iter()
			.all(|button| matches!(button.icon, Icon::Plus | Icon::Menu))
	);
}

#[test]
fn header_shades_step_from_the_background_toward_the_text() {
	for kind in [
		ThemeKind::MacOsDark,
		ThemeKind::MacOsLight,
		ThemeKind::CatppuccinLatte,
	] {
		let theme = Theme::from_kind(kind);
		let colors = header_colors(&theme);
		let distance = |color| ratio(color, theme.bg);
		assert!(distance(colors.bar) > 1.0, "{kind:?}");
		assert!(distance(colors.hover) > distance(colors.bar), "{kind:?}");
		assert!(distance(colors.raised) > distance(colors.hover), "{kind:?}");
		assert!(ratio(colors.separator, colors.bar) > 1.1, "{kind:?}");
		assert_eq!(colors.fg, theme.fg);
	}
}

#[test]
fn header_text_is_readable_on_every_header_shade() {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	for theme in kinds.into_iter().map(Theme::from_kind) {
		let colors = header_colors(&theme);
		let (dim, accent) = tab_text_colors(&theme);
		for bg in [colors.bar, colors.hover, colors.raised] {
			for (name, color) in [("dim", dim), ("accent", accent)] {
				let contrast = ratio(color, bg);
				assert!(
					contrast >= TEXT,
					"{name} on {bg:06x} in {:?}: {contrast:.2}",
					theme.kind
				);
			}
		}
		assert!(ratio(colors.dim, colors.bar) >= 2.0, "{:?}", theme.kind);
	}
}
