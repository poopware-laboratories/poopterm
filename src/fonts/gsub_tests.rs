//! Tests for glyph substitution, on `GSUB` and `GDEF` tables assembled
//! field by field: feature selection, each lookup type and format, lookup
//! flags, bounds on hostile tables, and emoji sequences in Noto Color Emoji.
use super::{super::face::Table, Gsub, MAX_GLYPHS};
use crate::fonts::{Font, otl::Gdef};

/// A field of a table being built: a value, or an offset to a child table
/// placed after the table's fields, from the table's start.
pub(crate) enum F {
	V(u16),
	O(Vec<u8>),
	O32(Vec<u8>),
	/// An offset of 0: no table.
	Null,
}

/// `fields` laid out, with their child tables after them.
pub(crate) fn table(fields: Vec<F>) -> Vec<u8> {
	let header: usize = fields
		.iter()
		.map(|field| if matches!(field, F::O32(_)) { 4 } else { 2 })
		.sum();
	let (mut out, mut children) = (Vec::new(), Vec::new());
	for field in fields {
		let at = header + children.len();
		match field {
			F::V(value) => out.extend(value.to_be_bytes()),
			F::Null => out.extend([0, 0]),
			F::O(child) => {
				out.extend((at as u16).to_be_bytes());
				children.extend(child);
			}
			F::O32(child) => {
				out.extend((at as u32).to_be_bytes());
				children.extend(child);
			}
		}
	}
	out.extend(children);
	out
}

pub(crate) fn values(values: &[u16]) -> Vec<F> {
	values.iter().map(|&value| F::V(value)).collect()
}

/// A tag as the two values it takes.
fn tag(tag: &[u8; 4]) -> [F; 2] {
	[
		F::V(u16::from_be_bytes([tag[0], tag[1]])),
		F::V(u16::from_be_bytes([tag[2], tag[3]])),
	]
}

/// Coverage format 1 of sorted `glyphs`.
pub(crate) fn coverage(glyphs: &[u16]) -> Vec<u8> {
	let mut fields = values(&[1, glyphs.len() as u16]);
	fields.extend(values(glyphs));
	table(fields)
}

/// Coverage format 2: `(first, last, first coverage index)` ranges.
pub(crate) fn coverage_ranges(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
	let mut fields = values(&[2, ranges.len() as u16]);
	for &(first, last, index) in ranges {
		fields.extend(values(&[first, last, index]));
	}
	table(fields)
}

/// Class definition format 2: `(first, last, class)` ranges.
pub(crate) fn classes(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
	let mut fields = values(&[2, ranges.len() as u16]);
	for &(first, last, class) in ranges {
		fields.extend(values(&[first, last, class]));
	}
	table(fields)
}

/// A lookup of `kind` with `flag` and its `subtables`.
pub(crate) fn lookup(kind: u16, flag: u16, subtables: Vec<Vec<u8>>) -> Vec<u8> {
	let mut fields = values(&[kind, flag, subtables.len() as u16]);
	fields.extend(subtables.into_iter().map(F::O));
	table(fields)
}

/// A language system using `features`, with `required` (0xffff for none).
fn language(required: u16, features: &[u16]) -> Vec<u8> {
	let mut fields = values(&[0, required, features.len() as u16]);
	fields.extend(values(features));
	table(fields)
}

/// A `GSUB` table whose scripts each have a default language system using
/// the listed features, with the features' tags and lookups.
pub(crate) fn gsub_with(
	scripts: Vec<(&[u8; 4], Vec<u8>)>,
	features: &[(&[u8; 4], &[u16])],
	lookups: Vec<Vec<u8>>,
) -> Vec<u8> {
	let mut script_list = vec![F::V(scripts.len() as u16)];
	for (name, language) in scripts {
		script_list.extend(tag(name));
		script_list.push(F::O(table(vec![F::O(language), F::V(0)])));
	}
	let mut feature_list = vec![F::V(features.len() as u16)];
	for (name, indices) in features {
		feature_list.extend(tag(name));
		let mut feature = values(&[0, indices.len() as u16]);
		feature.extend(values(indices));
		feature_list.push(F::O(table(feature)));
	}
	let mut lookup_list = vec![F::V(lookups.len() as u16)];
	lookup_list.extend(lookups.into_iter().map(F::O));
	table(vec![
		F::V(1),
		F::V(0),
		F::O(table(script_list)),
		F::O(table(feature_list)),
		F::O(table(lookup_list)),
	])
}

/// A `GSUB` table whose default script uses every feature in `features`.
pub(crate) fn gsub(features: &[(&[u8; 4], &[u16])], lookups: Vec<Vec<u8>>) -> Vec<u8> {
	let all: Vec<u16> = (0..features.len() as u16).collect();
	gsub_with(vec![(b"DFLT", language(0xffff, &all))], features, lookups)
}

/// Shapes `glyphs` with a `GSUB` table, and a `GDEF` one when given, both
/// in one buffer as in a font.
pub(crate) fn shape(gsub: &[u8], gdef: Option<&[u8]>, glyphs: &[u16]) -> Vec<u16> {
	let data = [gsub, gdef.unwrap_or_default()].concat();
	let at = |offset, len| Table { offset, len };
	let parsed = Gsub::parse(&data, at(0, gsub.len())).expect("a GSUB table");
	let gdef = gdef.map(|gdef| Gdef::parse(&data, at(gsub.len(), gdef.len())).expect("a GDEF table"));
	let mut glyphs = glyphs.to_vec();
	parsed.apply(&data, gdef.as_ref(), &mut glyphs);
	glyphs
}

/// Single substitution format 2 for `pairs` of sorted glyphs and their
/// substitutes.
pub(crate) fn single(pairs: &[(u16, u16)]) -> Vec<u8> {
	let glyphs: Vec<u16> = pairs.iter().map(|pair| pair.0).collect();
	let mut fields = vec![F::V(2), F::O(coverage(&glyphs)), F::V(pairs.len() as u16)];
	fields.extend(pairs.iter().map(|pair| F::V(pair.1)));
	table(fields)
}

/// A first glyph and its ligatures, as (ligature glyph, the components
/// after the first).
pub(crate) type LigatureSet<'a> = (u16, &'a [(u16, &'a [u16])]);

/// Ligature substitution: a set per sorted first glyph.
pub(crate) fn ligatures(sets: &[LigatureSet]) -> Vec<u8> {
	let firsts: Vec<u16> = sets.iter().map(|set| set.0).collect();
	let mut fields = vec![F::V(1), F::O(coverage(&firsts)), F::V(sets.len() as u16)];
	for (_, ligatures) in sets {
		let mut set = vec![F::V(ligatures.len() as u16)];
		for (glyph, rest) in *ligatures {
			let mut ligature = values(&[*glyph, rest.len() as u16 + 1]);
			ligature.extend(values(rest));
			set.push(F::O(table(ligature)));
		}
		fields.push(F::O(table(set)));
	}
	table(fields)
}

/// Multiple substitution: each sorted glyph becomes its sequence.
pub(crate) fn multiple(sequences: &[(u16, &[u16])]) -> Vec<u8> {
	let glyphs: Vec<u16> = sequences.iter().map(|sequence| sequence.0).collect();
	let mut fields = vec![
		F::V(1),
		F::O(coverage(&glyphs)),
		F::V(sequences.len() as u16),
	];
	for (_, sequence) in sequences {
		let mut sequence_fields = vec![F::V(sequence.len() as u16)];
		sequence_fields.extend(values(sequence));
		fields.push(F::O(table(sequence_fields)));
	}
	table(fields)
}

pub(crate) const CCMP: &[u8; 4] = b"ccmp";

#[test]
fn single_substitution_by_delta_wraps_around() {
	let delta = table(vec![F::V(1), F::O(coverage(&[3, 65535])), F::V(10)]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(1, 0, vec![delta])]);
	assert_eq!(shape(&gsub, None, &[3, 4, 65535]), [13, 4, 9]);
}

#[test]
fn single_substitution_by_list_replaces_covered_glyphs() {
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(1, 0, vec![single(&[(2, 20), (5, 50)])])],
	);
	assert_eq!(shape(&gsub, None, &[1, 2, 5, 2]), [1, 20, 50, 20]);
}

#[test]
fn a_substitute_index_past_the_list_changes_nothing() {
	// Glyph 7 is covered (index 1) but only one substitute is listed.
	let short = table(vec![F::V(2), F::O(coverage(&[3, 7])), F::V(1), F::V(30)]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(1, 0, vec![short])]);
	assert_eq!(shape(&gsub, None, &[3, 7]), [30, 7]);
}

#[test]
fn multiple_substitution_expands_and_an_empty_sequence_deletes() {
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(2, 0, vec![multiple(&[(2, &[7, 8, 9]), (4, &[])])])],
	);
	assert_eq!(shape(&gsub, None, &[1, 2, 3, 4, 5]), [1, 7, 8, 9, 3, 5]);
	assert_eq!(shape(&gsub, None, &[4, 4, 4]), [] as [u16; 0]);
}

#[test]
fn multiple_substitution_never_grows_a_cluster_past_the_bound() {
	let many: Vec<u16> = (100..100 + MAX_GLYPHS as u16).collect();
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(2, 0, vec![multiple(&[(1, &many)])])],
	);
	assert_eq!(
		shape(&gsub, None, &[1]).len(),
		MAX_GLYPHS,
		"one glyph may become the bound"
	);
	assert_eq!(shape(&gsub, None, &[1, 2]), [1, 2], "but not more");
}

#[test]
fn alternate_substitution_takes_the_first_alternate() {
	let set = table(values(&[3, 40, 41, 42]));
	let empty = table(values(&[0]));
	let alternates = table(vec![
		F::V(1),
		F::O(coverage(&[4, 5])),
		F::V(2),
		F::O(set),
		F::O(empty),
	]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(3, 0, vec![alternates])]);
	assert_eq!(
		shape(&gsub, None, &[4, 5]),
		[40, 5],
		"an empty set changes nothing"
	);
}

#[test]
fn the_first_ligature_whose_components_follow_wins() {
	let ligatures = ligatures(&[(1, &[(100, &[2, 3]), (101, &[2]), (102, &[9])])]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(4, 0, vec![ligatures])]);
	assert_eq!(shape(&gsub, None, &[1, 2, 3]), [100]);
	assert_eq!(
		shape(&gsub, None, &[1, 2, 4]),
		[101, 4],
		"falls back to a shorter one"
	);
	assert_eq!(
		shape(&gsub, None, &[1, 2]),
		[101],
		"the longer one needs a glyph past the end"
	);
	assert_eq!(shape(&gsub, None, &[2, 1, 2, 3, 1]), [2, 100, 1]);
	assert_eq!(shape(&gsub, None, &[1, 4]), [1, 4], "no components follow");
}

#[test]
fn ligatures_of_one_component_replace_the_glyph() {
	let single = ligatures(&[(5, &[(50, &[])])]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(4, 0, vec![single])]);
	assert_eq!(shape(&gsub, None, &[5, 5]), [50, 50]);
}

#[test]
fn a_ligature_with_no_components_is_skipped() {
	// A component count of 0 is malformed; the next ligature still applies.
	let broken = table(vec![
		F::V(1),
		F::O(coverage(&[1])),
		F::V(1),
		F::O(table(vec![
			F::V(2),
			F::O(table(values(&[99, 0]))),
			F::O(table(values(&[100, 2, 2]))),
		])),
	]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(4, 0, vec![broken])]);
	assert_eq!(shape(&gsub, None, &[1, 2]), [100]);
}

#[test]
fn lookups_run_in_lookup_list_order_and_each_sees_the_last_ones_output() {
	// Feature order lists lookup 1 first, but lookup 0 runs first: 1 → 2,
	// then the ligature 2 3 → 9.
	let gsub = gsub(
		&[(CCMP, &[1]), (b"liga", &[0])],
		vec![
			lookup(1, 0, vec![single(&[(1, 2)])]),
			lookup(4, 0, vec![ligatures(&[(2, &[(9, &[3])])])]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1, 3]), [9]);
}

#[test]
fn a_lookup_two_features_share_runs_once() {
	let gsub = gsub(
		&[(CCMP, &[0]), (b"liga", &[0])],
		vec![lookup(
			1,
			0,
			vec![table(vec![F::V(1), F::O(coverage(&[1, 2])), F::V(1)])],
		)],
	);
	assert_eq!(shape(&gsub, None, &[1]), [2], "not 3");
}

#[test]
fn only_composition_and_ligature_features_apply() {
	for (feature, applies) in [
		(b"ccmp", true),
		(b"liga", true),
		(b"rlig", true),
		(b"smcp", false),
		(b"dlig", false),
		(b"calt", false),
	] {
		let gsub = gsub(
			&[(feature, &[0])],
			vec![lookup(1, 0, vec![single(&[(1, 2)])])],
		);
		let expected = if applies { [2] } else { [1] };
		assert_eq!(
			shape(&gsub, None, &[1]),
			expected,
			"{}",
			String::from_utf8_lossy(feature)
		);
	}
}

#[test]
fn the_required_feature_applies_whatever_its_tag() {
	let gsub = gsub_with(
		vec![(b"DFLT", language(0, &[]))],
		&[(b"zzzz", &[0])],
		vec![lookup(1, 0, vec![single(&[(1, 2)])])],
	);
	assert_eq!(shape(&gsub, None, &[1]), [2]);
}

#[test]
fn the_default_script_wins_then_latin_then_the_first() {
	let lookups = || {
		vec![
			lookup(1, 0, vec![single(&[(1, 10)])]),
			lookup(1, 0, vec![single(&[(1, 20)])]),
			lookup(1, 0, vec![single(&[(1, 30)])]),
		]
	};
	let features: &[(&[u8; 4], &[u16])] = &[(CCMP, &[0]), (CCMP, &[1]), (CCMP, &[2])];
	let script = |name, feature| (name, language(0xffff, &[feature]));
	let cases = [
		(
			vec![script(b"cyrl", 2), script(b"DFLT", 0), script(b"latn", 1)],
			10,
		),
		(vec![script(b"cyrl", 2), script(b"latn", 1)], 20),
		(vec![script(b"grek", 2), script(b"cyrl", 1)], 30),
	];
	for (scripts, expected) in cases {
		let gsub = gsub_with(scripts, features, lookups());
		assert_eq!(shape(&gsub, None, &[1]), [expected]);
	}
}

#[test]
fn a_script_without_a_default_language_uses_its_first() {
	// Script table: no default, one language record ("ENG ") using feature 0.
	let script = table(vec![
		F::Null,
		F::V(1),
		F::V(u16::from_be_bytes(*b"EN")),
		F::V(u16::from_be_bytes(*b"G ")),
		F::O(language(0xffff, &[0])),
	]);
	let gsub = table(vec![
		F::V(1),
		F::V(0),
		F::O(table(vec![
			F::V(1),
			F::V(u16::from_be_bytes(*b"DF")),
			F::V(u16::from_be_bytes(*b"LT")),
			F::O(script),
		])),
		F::O(table(vec![
			F::V(1),
			F::V(u16::from_be_bytes(*b"cc")),
			F::V(u16::from_be_bytes(*b"mp")),
			F::O(table(values(&[0, 1, 0]))),
		])),
		F::O(table(vec![
			F::V(1),
			F::O(lookup(1, 0, vec![single(&[(1, 2)])])),
		])),
	]);
	assert_eq!(shape(&gsub, None, &[1]), [2]);
}

#[test]
fn feature_and_lookup_indices_past_their_lists_are_ignored() {
	let gsub = gsub_with(
		vec![(b"DFLT", language(7, &[0, 9]))],
		&[(CCMP, &[5, 0])],
		vec![lookup(1, 0, vec![single(&[(1, 2)])])],
	);
	assert_eq!(shape(&gsub, None, &[1]), [2]);
}

/// An extension subtable holding a subtable of `kind`.
fn extension(kind: u16, subtable: Vec<u8>) -> Vec<u8> {
	table(vec![F::V(1), F::V(kind), F::O32(subtable)])
}

#[test]
fn extension_lookups_apply_the_subtables_they_hold() {
	let ligature = ligatures(&[(1, &[(9, &[2])])]);
	let gsub = gsub(
		&[(CCMP, &[0, 1])],
		vec![
			lookup(7, 0, vec![extension(4, ligature)]),
			lookup(7, 0, vec![extension(1, single(&[(9, 10)]))]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1, 2]), [10]);
}

#[test]
fn an_extension_of_an_extension_is_refused() {
	let nested = extension(7, extension(1, single(&[(1, 2)])));
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(7, 0, vec![nested])]);
	assert_eq!(shape(&gsub, None, &[1]), [1]);
}

#[test]
fn unknown_versions_and_lookup_types_are_ignored() {
	let mut versioned = gsub(&[(CCMP, &[0])], vec![lookup(1, 0, vec![single(&[(1, 2)])])]);
	versioned[1] = 2;
	assert!(
		Gsub::parse(
			&versioned,
			Table {
				offset: 0,
				len: versioned.len()
			}
		)
		.is_none()
	);
	let unknown = gsub(&[(CCMP, &[0])], vec![lookup(9, 0, vec![single(&[(1, 2)])])]);
	assert_eq!(shape(&unknown, None, &[1]), [1]);
}

#[test]
fn truncated_tables_never_panic() {
	let full = gsub(
		&[(CCMP, &[0, 1, 2])],
		vec![
			lookup(4, 0, vec![ligatures(&[(1, &[(9, &[2, 3])])])]),
			lookup(2, 0, vec![multiple(&[(9, &[4, 5])])]),
			lookup(1, 0, vec![single(&[(4, 6)])]),
		],
	);
	assert_eq!(shape(&full, None, &[1, 2, 3]), [6, 5]);
	for len in 0..full.len() {
		let bytes = &full[..len];
		if let Some(gsub) = Gsub::parse(bytes, Table { offset: 0, len }) {
			let mut glyphs = vec![1, 2, 3];
			gsub.apply(bytes, None, &mut glyphs);
			assert!(glyphs.len() <= MAX_GLYPHS);
		}
	}
}

/// A Noto Color Emoji style font from `chars`, sorted, whose `GSUB` is
/// `gsub`.
fn color_font_with(chars: &[char], gsub: Vec<u8>) -> Font {
	use super::super::cbdt_tests::{Image, Strike, font_tables, png, sfnt};
	let images = (0..chars.len() + 4)
		.map(|_| {
			Some(Image {
				metrics: [2, 2, 0, 2, 2],
				png: png(2, 2, &[0xff00_ff00; 4]),
			})
		})
		.collect();
	let strike = Strike {
		ppem: 20,
		first: 1,
		index_format: 1,
		image_format: 17,
		images,
	};
	let mut tables = font_tables(chars, &[strike]);
	tables.push((b"GSUB", gsub));
	tables.sort_by_key(|table| *table.0);
	Font::from_bytes(sfnt(&tables), 0).unwrap()
}

#[test]
fn a_font_forms_one_glyph_from_a_sequence_it_has_a_ligature_for() {
	// Glyphs: 1 '1', 2 'x', 3 U+20E3, 4 FE0F; 5 is the keycap ligature.
	let chars = ['1', 'x', '\u{20e3}', '\u{fe0f}'];
	let font = color_font_with(
		&chars,
		gsub(
			&[(CCMP, &[0])],
			vec![lookup(4, 0, vec![ligatures(&[(1, &[(5, &[4, 3])])])])],
		),
	);
	assert_eq!(font.ligature(&['1', '\u{fe0f}', '\u{20e3}']), Some(5));
	assert_eq!(
		font.ligature(&['1', '\u{20e3}']),
		None,
		"no ligature for it"
	);
	assert_eq!(
		font.ligature(&['1']),
		None,
		"a single character is no ligature"
	);
	assert_eq!(
		font.ligature(&['1', '\u{fe0f}', '\u{20e3}', 'x']),
		None,
		"two glyphs"
	);
	let pixels = font
		.rasterize_color_glyph(5, 20.0)
		.map(|(_, pixels)| pixels[0]);
	assert_eq!(pixels, Some(0xff00_ff00), "the ligature draws in color");
}

#[test]
fn sequences_without_their_variation_selectors_are_tried_too() {
	// The font's ligature leaves FE0F out (glyphs: 1 '1', 2 U+20E3, 3 FE0F).
	let font = color_font_with(
		&['1', '\u{20e3}', '\u{fe0f}'],
		gsub(
			&[(CCMP, &[0])],
			vec![lookup(4, 0, vec![ligatures(&[(1, &[(5, &[2])])])])],
		),
	);
	assert_eq!(font.ligature(&['1', '\u{fe0f}', '\u{20e3}']), Some(5));
	assert_eq!(
		font.ligature(&['1', '\u{fe0f}']),
		None,
		"only the base is left"
	);
}

#[test]
fn a_ligature_to_glyph_zero_is_not_a_ligature() {
	let chars = ['a', 'b'];
	let font = color_font_with(
		&chars,
		gsub(
			&[(CCMP, &[0])],
			vec![lookup(4, 0, vec![ligatures(&[(1, &[(0, &[2])])])])],
		),
	);
	assert_eq!(font.ligature(&chars), None);
}

/// Noto Color Emoji, when it is installed.
pub(crate) fn noto() -> Option<Font> {
	let bytes = std::fs::read("/usr/share/fonts/noto/NotoColorEmoji.ttf").ok()?;
	Font::from_bytes(bytes, 0).ok()
}

#[test]
fn installed_noto_color_emoji_forms_its_emoji_sequences() {
	let Some(font) = noto() else {
		return;
	};
	let sequences = [
		"👨\u{200d}👩\u{200d}👧\u{200d}👦", // family, ZWJ
		"1\u{fe0f}\u{20e3}",                // keycap
		"🇩🇪",                               // flag, regional indicators
		"👋🏽",                               // skin tone modifier
		"🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}", // England, tags
		"🧑🏽\u{200d}💻",                     // ZWJ with a modifier
		"❤\u{fe0f}\u{200d}🔥",              // heart on fire
	];
	for sequence in sequences {
		let chars: Vec<char> = sequence.chars().collect();
		let glyph = font.ligature(&chars);
		assert!(glyph.is_some(), "{sequence} {chars:?}");
		let drawn = font.rasterize_color_glyph(glyph.unwrap(), 20.0);
		assert!(drawn.is_some(), "{sequence} draws in color");
	}
	assert_eq!(
		font.ligature(&['👨', '\u{200d}']),
		None,
		"a dangling joiner"
	);
}

#[test]
fn installed_noto_shows_an_unknown_flag_as_one_placeholder() {
	// Its contextual lookups turn a pair of regional indicators without a
	// flag into one "unknown flag" glyph and delete the second.
	let Some(font) = noto() else {
		return;
	};
	let unknown = font.ligature(&['🇦', '🇦']);
	let other = font.ligature(&['🇿', '🇿']);
	assert!(
		unknown.is_some() && unknown == other,
		"{unknown:?} {other:?}"
	);
	assert_ne!(unknown, font.ligature(&['🇩', '🇪']));
}

#[path = "gsub_edge_tests.rs"]
mod edge;
