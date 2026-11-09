//! Tests of the Settings main page's row positions: every gap follows one
//! rule, and the last row clears the footer in the smallest window.
use super::settings_layout::{
	CONTROL, CREDIT, FOOTER, GAP_BLOCK, GAP_CREDIT, GAP_GROUP, GAP_HEADING, GAP_LABEL, GAP_ROW,
	LABEL, MAIN, MIN_HEIGHT, SECTION_TOP, WALLPAPER_BUTTONS, main_rows,
};

#[test]
fn the_table_is_computed_once_and_stays_the_same() {
	assert_eq!(main_rows(), MAIN);
}

#[test]
fn rows_run_down_the_page_in_the_order_they_are_drawn() {
	let m = MAIN;
	let order = [
		m.heading,
		m.theme_label,
		m.theme,
		m.credit,
		m.wallpaper_label,
		m.wallpaper,
		m.strength,
		m.opacity,
		m.font,
		m.scrollback,
		m.restore,
	];
	assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
}

#[test]
fn a_label_sits_a_fixed_gap_above_its_control() {
	let m = MAIN;
	assert_eq!(m.theme - (m.theme_label + LABEL), GAP_LABEL);
	assert_eq!(m.wallpaper - (m.wallpaper_label + LABEL), GAP_LABEL);
}

#[test]
fn blocks_are_a_fixed_gap_apart() {
	let m = MAIN;
	assert_eq!(m.theme_label - (m.heading + LABEL), GAP_HEADING);
	assert_eq!(m.wallpaper_label - (m.credit + CREDIT), GAP_BLOCK);
	assert_eq!(m.strength - (m.wallpaper + WALLPAPER_BUTTONS), GAP_BLOCK);
}

#[test]
fn the_credit_hugs_the_picker_it_belongs_to() {
	let m = MAIN;
	assert_eq!(m.credit - (m.theme + CONTROL), GAP_CREDIT);
	assert!(m.credit - (m.theme + CONTROL) < GAP_BLOCK);
}

#[test]
fn stepper_rows_keep_one_stride() {
	let m = MAIN;
	let stride = CONTROL + GAP_ROW;
	assert_eq!(m.opacity - m.strength, stride);
	assert_eq!(m.scrollback - m.font, stride);
	assert_eq!(m.restore - m.scrollback, stride);
}

#[test]
fn the_text_group_starts_one_extra_gap_below_the_appearance_group() {
	let m = MAIN;
	assert_eq!(m.font - m.opacity, CONTROL + GAP_ROW + GAP_GROUP);
}

#[test]
fn rows_never_overlap_the_one_below() {
	let m = MAIN;
	for (top, height, next) in [
		(m.theme, CONTROL, m.credit),
		(m.wallpaper, WALLPAPER_BUTTONS, m.strength),
		(m.strength, CONTROL, m.opacity),
		(m.opacity, CONTROL, m.font),
		(m.font, CONTROL, m.scrollback),
		(m.scrollback, CONTROL, m.restore),
	] {
		assert!(top + height < next, "{top} + {height} reaches {next}");
	}
}

#[test]
fn the_last_row_clears_the_footer_in_the_smallest_window() {
	let bottom = MAIN.restore + CONTROL;
	assert!(
		bottom + 8 <= MIN_HEIGHT - FOOTER,
		"ends at {bottom} of {}",
		MIN_HEIGHT - FOOTER
	);
}

#[test]
fn the_page_starts_below_the_header_buttons() {
	// The page buttons end at 20 + 36 = 56.
	const { assert!(MAIN.heading >= 56 + 6) };
	assert_eq!(MAIN.heading, SECTION_TOP);
}

#[test]
fn the_wallpaper_buttons_are_shorter_than_a_control() {
	const { assert!(WALLPAPER_BUTTONS < CONTROL) };
}
