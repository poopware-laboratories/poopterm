//! Summaries of repeated benchmark runs: the median with its spread, and the
//! overall rate of a set of workloads.

/// The median of a set of rates with the slowest and fastest run, so noisy
/// runs are visible next to the figure that is compared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spread {
	pub median: f64,
	pub min: f64,
	pub max: f64,
}

impl Spread {
	/// The spread of `rates`, or `None` without any.
	pub(crate) fn of(rates: &[f64]) -> Option<Self> {
		let mut sorted = rates.to_vec();
		sorted.sort_by(f64::total_cmp);
		let (&min, &max) = (sorted.first()?, sorted.last()?);
		let middle = sorted.len() / 2;
		let median = if sorted.len() % 2 == 1 {
			sorted[middle]
		} else {
			(sorted[middle - 1] + sorted[middle]) / 2.0
		};
		Some(Self { median, min, max })
	}
}

/// Total bytes over total time, timing each workload at its median rate.
/// The geometric mean would hide a slow workload behind fast ones; this
/// weighs each by the time it takes. `runs` holds (bytes, MB/s) pairs.
pub(crate) fn overall(runs: impl IntoIterator<Item = (usize, f64)>) -> Option<f64> {
	let (mut bytes, mut seconds) = (0.0, 0.0);
	for (size, rate) in runs {
		if rate <= 0.0 {
			return None;
		}
		bytes += size as f64;
		seconds += size as f64 / rate / 1e6;
	}
	(seconds > 0.0).then_some(bytes / seconds / 1e6)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_median_of_an_odd_count_is_the_middle_run() {
		let spread = Spread::of(&[30.0, 10.0, 20.0]).unwrap();
		assert_eq!(
			spread,
			Spread {
				median: 20.0,
				min: 10.0,
				max: 30.0
			}
		);
	}

	#[test]
	fn the_median_of_an_even_count_is_between_the_middle_runs() {
		let spread = Spread::of(&[40.0, 10.0, 30.0, 20.0]).unwrap();
		assert_eq!((spread.median, spread.min, spread.max), (25.0, 10.0, 40.0));
	}

	#[test]
	fn one_run_is_its_own_spread() {
		let spread = Spread::of(&[7.5]).unwrap();
		assert_eq!((spread.median, spread.min, spread.max), (7.5, 7.5, 7.5));
	}

	#[test]
	fn no_runs_have_no_spread() {
		assert_eq!(Spread::of(&[]), None);
	}

	#[test]
	fn the_input_order_is_left_alone() {
		let rates = [3.0, 1.0, 2.0];
		Spread::of(&rates);
		assert_eq!(rates, [3.0, 1.0, 2.0]);
	}

	#[test]
	fn overall_weighs_each_workload_by_its_time() {
		// 100 MB at 100 MB/s and 100 MB at 25 MB/s: 200 MB in 5 s.
		let rate = overall([(100_000_000, 100.0), (100_000_000, 25.0)]).unwrap();
		assert!((rate - 40.0).abs() < 1e-9, "{rate}");
	}

	#[test]
	fn a_bigger_workload_weighs_more() {
		// 300 MB at 100 MB/s (3 s) and 100 MB at 25 MB/s (4 s): 400 MB in
		// 7 s, not the 40 MB/s that equal weights would give.
		let rate = overall([(300_000_000, 100.0), (100_000_000, 25.0)]).unwrap();
		assert!((rate - 400.0 / 7.0).abs() < 1e-9, "{rate}");
	}

	#[test]
	fn the_spread_does_not_depend_on_the_run_order() {
		let orders = [
			[1.0, 2.0, 3.0, 4.0],
			[4.0, 3.0, 2.0, 1.0],
			[2.0, 4.0, 1.0, 3.0],
		];
		for rates in orders {
			let spread = Spread::of(&rates).unwrap();
			assert_eq!(
				(spread.median, spread.min, spread.max),
				(2.5, 1.0, 4.0),
				"{rates:?}"
			);
		}
	}

	#[test]
	fn overall_of_one_workload_is_its_rate() {
		let rate = overall([(1 << 20, 123.0)]).unwrap();
		assert!((rate - 123.0).abs() < 1e-9, "{rate}");
	}

	#[test]
	fn overall_of_nothing_is_none() {
		assert_eq!(overall([]), None);
	}

	#[test]
	fn a_run_too_fast_to_time_adds_its_bytes_but_no_time() {
		// A tiny workload can finish within the clock's resolution: bytes
		// over zero seconds is infinite, and must not make the total so.
		let rate = overall([(100_000_000, f64::INFINITY), (100_000_000, 100.0)]).unwrap();
		assert!((rate - 200.0).abs() < 1e-9, "{rate}");
	}

	#[test]
	fn only_untimeable_runs_have_no_overall_rate() {
		assert_eq!(overall([(100, f64::INFINITY)]), None);
	}

	#[test]
	fn an_untimeable_run_is_the_fastest_of_its_spread() {
		let spread = Spread::of(&[f64::INFINITY, 5.0, 4.0]).unwrap();
		assert_eq!((spread.median, spread.min), (5.0, 4.0));
		assert!(spread.max.is_infinite());
	}

	#[test]
	fn a_stalled_workload_has_no_overall_rate() {
		// A zero rate means no time measurement could finish; it must not
		// divide into infinity and read as zero time.
		assert_eq!(overall([(100, 50.0), (100, 0.0)]), None);
	}
}
