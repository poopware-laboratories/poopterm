//! Properties of `header_target` that hold for every tab strip: swept pixel
//! by pixel across widths, scales, tab counts, scroll positions, compact
//! tabs and window button layouts.
use super::{
	header_target::{HeaderTarget, HeaderTarget::*, header_target},
	tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction},
	window_controls::{ButtonLayout, place},
};
use crate::render::header::close_rect;

struct Case {
	layout: TabLayout,
	count: usize,
	width: usize,
	bar: (usize, usize),
	what: String,
}

fn cases() -> Vec<Case> {
	let mut cases = Vec::new();
	for scale in 1..=3 {
		for width in [400, 1100].map(|width| width * scale) {
			for buttons in [
				"appmenu:",
				"appmenu:minimize,maximize,close",
				"close:maximize",
			] {
				let controls = place(&ButtonLayout::parse(buttons), width, scale);
				for count in [1, 2, 5, 40] {
					for scroll in [0, 3, 40] {
						for compact_tabs in [false, true] {
							let options = TabStripOptions {
								left_reserve: controls.left_reserve,
								right_reserve: controls.right_reserve,
								scroll,
								show_plus: true,
								show_settings: true,
								active_tab: scroll.min(count - 1),
								compact_tabs,
							};
							cases.push(Case {
								layout: tab_layout_with_compaction(width, count, scale, options),
								count,
								width,
								bar: (4 * scale, 44 * scale),
								what: format!(
									"{buttons} at {width}x{scale}, {count} tabs from {scroll}, compact {compact_tabs}"
								),
							});
						}
					}
				}
			}
		}
	}
	cases
}

fn row(case: &Case, y: usize) -> Vec<Option<HeaderTarget>> {
	(0..case.width)
		.map(|x| header_target(&case.layout, case.count, case.bar, (x as f64, y as f64)))
		.collect()
}

fn middle(case: &Case) -> usize {
	case.bar.0 + case.bar.1 / 2
}

#[test]
fn each_button_is_found_exactly_across_its_own_width() {
	for case in cases() {
		let found = row(&case, middle(&case));
		let l = &case.layout;
		for (x, target) in found.iter().enumerate() {
			let settings = (l.settings_x..l.settings_x + l.settings_w).contains(&x);
			let plus = (l.plus_x..l.plus_x + l.plus_w).contains(&x);
			assert_eq!(*target == Some(Settings), settings, "{} at {x}", case.what);
			assert_eq!(*target == Some(NewTab), plus, "{} at {x}", case.what);
		}
	}
}

#[test]
fn every_shown_tab_is_found_in_one_run_and_in_order() {
	for case in cases() {
		let found = row(&case, middle(&case));
		let tabs: Vec<(usize, usize)> = found
			.iter()
			.enumerate()
			.filter_map(|(x, target)| match target {
				Some(Tab { index, .. }) => Some((x, *index)),
				_ => None,
			})
			.collect();
		let l = &case.layout;
		let shown = l.first_visible..(l.first_visible + l.visible_count).min(case.count);
		for index in shown.clone() {
			let xs: Vec<usize> = tabs.iter().filter(|t| t.1 == index).map(|t| t.0).collect();
			assert_eq!(xs.len(), l.tab_width(index), "{}: tab {index}", case.what);
			assert_eq!(
				xs.first(),
				Some(&l.tab_x(index)),
				"{}: tab {index}",
				case.what
			);
			assert!(
				xs.windows(2).all(|pair| pair[1] == pair[0] + 1),
				"{}",
				case.what
			);
		}
		assert!(tabs.iter().all(|t| shown.contains(&t.1)), "{}", case.what);
		assert!(
			tabs.windows(2).all(|pair| pair[0].1 <= pair[1].1),
			"{}",
			case.what
		);
		// No tab reaches the + button.
		assert!(tabs.iter().all(|t| t.0 < l.plus_x), "{}", case.what);
	}
}

#[test]
fn arrows_are_inert_or_scroll_toward_hidden_tabs_and_nothing_else_is_none() {
	for case in cases() {
		let found = row(&case, middle(&case));
		let l = &case.layout;
		let hidden_left = l.first_visible > 0;
		let hidden_right = l.first_visible + l.visible_count < case.count;
		for (x, target) in found.iter().enumerate() {
			let left = (l.left_arrow_x..l.left_arrow_x + l.arrow_w).contains(&x);
			let right = (l.right_arrow_x..l.right_arrow_x + l.arrow_w).contains(&x);
			let what = format!("{} at {x}", case.what);
			match target {
				Some(ScrollLeft) => assert!(left && hidden_left, "{what}"),
				Some(ScrollRight) => assert!(right && hidden_right, "{what}"),
				None => assert!((left && !hidden_left) || (right && !hidden_right), "{what}"),
				_ => assert!(!left && !right, "{what}"),
			}
		}
	}
}

#[test]
fn the_close_cross_is_found_only_inside_it_and_never_on_the_only_tab() {
	for case in cases() {
		let l = &case.layout;
		let shown = l.first_visible..(l.first_visible + l.visible_count).min(case.count);
		for index in shown {
			let (left, width) = (l.tab_x(index), l.tab_width(index));
			let (cx, cy, size, _) = close_rect(left, width, case.bar, l.scale);
			for x in left..left + width {
				let y = cy + size / 2;
				let found = header_target(l, case.count, case.bar, (x as f64, y as f64));
				let inside = (cx..cx + size).contains(&x) && case.count > 1;
				assert_eq!(
					found,
					Some(Tab {
						index,
						close: inside
					}),
					"{} at {x}",
					case.what
				);
			}
		}
	}
}

#[test]
fn nothing_is_found_above_or_below_the_bar() {
	for case in cases() {
		let (top, height) = case.bar;
		for y in [top - 1, top + height, top + height + 1] {
			assert!(
				row(&case, y).iter().all(Option::is_none),
				"{} at {y}",
				case.what
			);
		}
		assert!(row(&case, top).iter().any(Option::is_some), "{}", case.what);
		assert!(
			row(&case, top + height - 1).iter().any(Option::is_some),
			"{}",
			case.what
		);
	}
}

#[test]
fn the_close_cross_is_found_only_within_its_own_rows() {
	for case in cases().into_iter().filter(|case| case.count > 1) {
		let l = &case.layout;
		let index = l.first_visible;
		let (cx, cy, size, _) = close_rect(l.tab_x(index), l.tab_width(index), case.bar, l.scale);
		let x = (cx + size / 2) as f64;
		for y in case.bar.0..case.bar.0 + case.bar.1 {
			let found = header_target(l, case.count, case.bar, (x, y as f64));
			let close = (cy..cy + size).contains(&y);
			assert_eq!(found, Some(Tab { index, close }), "{} at {y}", case.what);
		}
	}
}
