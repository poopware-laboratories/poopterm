use super::font_size::FontSize;

const HALF: FontSize = FontSize::hundredths(50);
const ONE: FontSize = FontSize::points(1);
const MIN: FontSize = FontSize::points(10);
const MAX: FontSize = FontSize::points(32);

fn size(text: &str) -> FontSize {
	FontSize::parse(text).unwrap()
}

#[test]
fn whole_and_fractional_sizes_parse() {
	assert_eq!(size("18"), FontSize::points(18));
	assert_eq!(size("13.5"), FontSize::hundredths(1350));
	assert_eq!(size(" 12.25 "), FontSize::hundredths(1225));
	assert_eq!(size("+14.0"), FontSize::points(14));
}

#[test]
fn other_decimal_spellings_parse() {
	assert_eq!(size("1.35e1"), FontSize::hundredths(1350));
	assert_eq!(size(".5"), FontSize::hundredths(50));
	assert_eq!(size("14."), FontSize::points(14));
}

#[test]
fn sizes_round_to_the_nearest_hundredth_of_a_point() {
	assert_eq!(size("13.333"), FontSize::hundredths(1333));
	assert_eq!(size("13.336"), FontSize::hundredths(1334));
}

#[test]
fn nonsense_negative_and_non_finite_sizes_are_rejected() {
	for text in ["", "abc", "-1", "-0.5", "NaN", "inf", "-inf", "13,5", "1/2"] {
		assert_eq!(FontSize::parse(text), None, "{text:?}");
	}
}

#[test]
fn huge_sizes_parse_without_overflow_and_clamp() {
	assert_eq!(size("1e300").clamp(MIN, MAX), MAX);
	assert_eq!(size("0").clamp(MIN, MAX), MIN);
}

#[test]
fn sizes_print_without_trailing_zeros() {
	assert_eq!(FontSize::points(18).to_string(), "18");
	assert_eq!(FontSize::hundredths(1350).to_string(), "13.5");
	assert_eq!(FontSize::hundredths(1325).to_string(), "13.25");
	assert_eq!(FontSize::hundredths(1305).to_string(), "13.05");
	assert_eq!(FontSize::hundredths(5).to_string(), "0.05");
}

#[test]
fn printed_sizes_parse_back_to_the_same_size() {
	for hundredths in [600, 1000, 1305, 1325, 1350, 1333, 7200] {
		let font = FontSize::hundredths(hundredths);
		assert_eq!(size(&font.to_string()), font);
	}
}

#[test]
fn the_size_converts_to_exact_points() {
	assert_eq!(FontSize::hundredths(1350).as_f64(), 13.5);
	assert_eq!(FontSize::hundredths(1225).as_f64(), 12.25);
	assert_eq!(FontSize::points(18).as_f64(), 18.0);
}

#[test]
fn steps_move_by_the_step_size_from_sizes_on_its_grid() {
	assert_eq!(
		FontSize::points(14).stepped(HALF, true, MIN, MAX),
		size("14.5")
	);
	assert_eq!(
		size("14.5").stepped(HALF, true, MIN, MAX),
		FontSize::points(15)
	);
	assert_eq!(
		FontSize::points(14).stepped(HALF, false, MIN, MAX),
		size("13.5")
	);
	assert_eq!(
		FontSize::points(14).stepped(ONE, true, MIN, MAX),
		FontSize::points(15)
	);
}

#[test]
fn steps_from_sizes_off_the_grid_snap_onto_it() {
	assert_eq!(size("13.3").stepped(HALF, true, MIN, MAX), size("13.5"));
	assert_eq!(
		size("13.3").stepped(HALF, false, MIN, MAX),
		FontSize::points(13)
	);
	assert_eq!(
		size("13.5").stepped(ONE, true, MIN, MAX),
		FontSize::points(14)
	);
	assert_eq!(
		size("13.5").stepped(ONE, false, MIN, MAX),
		FontSize::points(13)
	);
}

#[test]
fn steps_stop_at_the_bounds() {
	assert_eq!(MAX.stepped(HALF, true, MIN, MAX), MAX);
	assert_eq!(size("31.8").stepped(HALF, true, MIN, MAX), MAX);
	assert_eq!(MIN.stepped(HALF, false, MIN, MAX), MIN);
	assert_eq!(size("10.2").stepped(HALF, false, MIN, MAX), MIN);
	// A size outside the bounds is brought back inside by any step.
	assert_eq!(FontSize::points(40).stepped(ONE, true, MIN, MAX), MAX);
	assert_eq!(FontSize::points(2).stepped(ONE, false, MIN, MAX), MIN);
}

#[test]
fn stepping_down_from_zero_stays_at_the_minimum() {
	let zero = FontSize::hundredths(0);
	assert_eq!(zero.stepped(ONE, false, zero, MAX), zero);
}
