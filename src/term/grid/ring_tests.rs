//! Full-screen scrolling keeps physical row allocations fixed and preserves logical order.
use super::*;
fn blank() -> Cell {
	Cell::default()
}
fn populated() -> Grid {
	let mut grid = Grid::new(4, 4, blank());
	for y in 0..4 {
		grid[y][0].ch = char::from(b'a' + y as u8);
	}
	grid
}
#[test]
fn full_screen_scroll_reuses_rows_without_moving_the_header_array() {
	let mut grid = populated();
	let physical: Vec<_> = grid.rows.iter().map(|row| row.as_ptr()).collect();
	let mut evicted = Vec::new();
	grid.scroll_up(0, 3, 1, blank(), |row, _| {
		evicted.push(row[0].ch);
		row.len()
	});
	assert_eq!(
		grid.rows.iter().map(|row| row.as_ptr()).collect::<Vec<_>>(),
		physical
	);
	assert_eq!(evicted, vec!['a']);
	assert_eq!((0..4).map(|y| grid[y][0].ch).collect::<String>(), "bcd ");
	grid.assert_extents();
}

fn logical(grid: &Grid) -> Vec<Row> {
	grid.iter().cloned().collect()
}
fn scroll(grid: &mut Grid, count: usize) {
	let bottom = grid.rows.len() - 1;
	grid.scroll_up(0, bottom, count, blank(), |row, _| row.len());
}
#[test]
fn repeated_wraps_preserve_indexing_mutation_and_iteration() {
	let mut grid = populated();
	let mut expected = logical(&grid);
	for n in 0..37 {
		scroll(&mut grid, 1);
		expected.remove(0);
		expected.push(vec![blank(); 4]);
		let cell = Cell {
			ch: char::from(b'A' + n % 26),
			..blank()
		};
		grid[3][2] = cell;
		expected[3][2] = cell;
		assert_eq!(logical(&grid), expected);
		for y in 0..4 {
			assert_eq!(grid[y], expected[y]);
		}
		grid.assert_extents();
	}
}
#[test]
fn partial_regions_normalize_a_wrapped_grid_without_reordering_other_rows() {
	for shift in 1..4 {
		for down in [false, true] {
			let mut grid = populated();
			scroll(&mut grid, shift);
			let mut expected = logical(&grid);
			if down {
				grid.scroll_down(1, 2, 1, blank());
				expected.remove(2);
				expected.insert(1, vec![blank(); 4]);
			} else {
				grid.scroll_up(1, 2, 1, blank(), |row, _| row.len());
				expected.remove(1);
				expected.insert(2, vec![blank(); 4]);
			}
			assert_eq!(logical(&grid), expected);
			grid.assert_extents();
		}
	}
}
#[test]
fn wrapped_resize_retains_the_logical_top_left_for_growth_and_shrinkage() {
	for shift in 1..4 {
		for (cols, rows) in [(2, 2), (7, 6), (4, 1), (0, 0)] {
			let mut grid = populated();
			scroll(&mut grid, shift);
			let mut expected = logical(&grid);
			expected.truncate(rows);
			for row in &mut expected {
				row.resize(cols, blank());
			}
			expected.resize(rows, vec![blank(); cols]);
			grid.resize(cols, rows, blank());
			assert_eq!(logical(&grid), expected);
			grid.assert_extents();
		}
	}
}
#[test]
fn row_extent_operations_use_logical_indices_after_wrap() {
	let mut grid = populated();
	scroll(&mut grid, 3);
	let changed = Cell { ch: 'z', ..blank() };
	grid.row_unmarked(1)[1] = changed;
	grid.mark_used(1, 2);
	assert_eq!(grid.used(1), 2);
	assert_eq!(grid[1][1], changed);
	grid.fill(1, 1, 4, blank());
	assert_eq!(grid.used(1), 1);
	assert_eq!(grid[1][1], blank());
	grid.row_upto(2, 3)[2] = changed;
	assert_eq!(grid.used(2), 3);
	grid.assert_extents();
}
#[test]
fn mutable_iteration_and_clone_keep_the_logical_order() {
	let mut grid = populated();
	scroll(&mut grid, 2);
	let mut copy = grid.clone();
	for (y, row) in copy.iter_mut().enumerate() {
		row[1].ch = char::from(b'0' + y as u8);
	}
	assert_eq!((0..4).map(|y| copy[y][1].ch).collect::<String>(), "0123");
	assert_eq!((0..4).map(|y| grid[y][1].ch).collect::<String>(), "    ");
	copy.assert_extents();
	grid.assert_extents();
}
#[test]
fn oversized_full_scroll_evicts_every_logical_row_in_order() {
	let mut grid = populated();
	scroll(&mut grid, 1);
	let before = logical(&grid);
	let mut evicted = Vec::new();
	grid.scroll_up(0, 3, usize::MAX, blank(), |row, _| {
		evicted.push(row.to_vec());
		row.len()
	});
	assert_eq!(evicted, before);
	assert_eq!(logical(&grid), vec![vec![blank(); 4]; 4]);
	assert_eq!((0..4).map(|y| grid.used(y)).collect::<Vec<_>>(), vec![0; 4]);
	grid.assert_extents();
}
#[test]
fn invalid_logical_indices_still_panic_at_every_ring_position() {
	for shift in 0..4 {
		let mut grid = populated();
		scroll(&mut grid, shift);
		assert!(std::panic::catch_unwind(|| &grid[4]).is_err());
		assert!(std::panic::catch_unwind(|| grid.used(4)).is_err());
		assert!(
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| grid[4][0] = blank())).is_err()
		);
	}
}
#[test]
fn mixed_operations_match_an_independent_row_list_model() {
	let mut grid = populated();
	let mut expected = logical(&grid);
	let mut seed = 0x5eed_u64;
	for step in 0..1000 {
		seed ^= seed << 13;
		seed ^= seed >> 7;
		seed ^= seed << 17;
		let rows = expected.len();
		let cols = expected[0].len();
		let count = (seed as usize / 7) % (rows + 2);
		let y = seed as usize % rows;
		match seed % 6 {
			0 | 1 => {
				let top = if seed % 6 == 0 { 0 } else { y };
				let bottom = rows - 1;
				let n = count.min(bottom + 1 - top);
				let mut actual = Vec::new();
				grid.scroll_up(top, bottom, count, blank(), |row, _| {
					actual.push(row.to_vec());
					row.len()
				});
				let mut removed = Vec::new();
				for _ in 0..n {
					removed.push(expected.remove(top));
					expected.insert(bottom, vec![blank(); cols]);
				}
				assert_eq!(actual, removed, "evictions at {step}");
			}
			2 => {
				grid.scroll_down(y, rows - 1, count, blank());
				for _ in 0..count.min(rows - y) {
					expected.pop();
					expected.insert(y, vec![blank(); cols]);
				}
			}
			3 => {
				let cols = 1 + count;
				let rows = 1 + (seed as usize % 7);
				grid.resize(cols, rows, blank());
				expected.truncate(rows);
				for row in &mut expected {
					row.resize(cols, blank());
				}
				expected.resize(rows, vec![blank(); cols]);
			}
			4 => {
				let cell = Cell {
					ch: char::from(b'a' + (seed % 26) as u8),
					..blank()
				};
				let end = seed as usize % (cols + 1);
				grid.fill(y, 0, end, cell);
				expected[y][..end].fill(cell);
			}
			_ => {
				let x = seed as usize % cols;
				grid[y][x].ch = 'X';
				expected[y][x].ch = 'X';
			}
		}
		assert_eq!(logical(&grid), expected, "state at {step}");
		grid.assert_extents();
	}
}
