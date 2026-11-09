"""Width table generator: UCD parsing, the width rules and their order, and packing."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


table = load('width_table')

FILES = {
    'UnicodeData.txt': '',
    'EastAsianWidth.txt': '',
    'DerivedCoreProperties.txt': '',
    'HangulSyllableType.txt': '',
    'auxiliary/GraphemeBreakProperty.txt': '',
}

# A tiny database where each code point tests one rule, or two rules' order.
UCD = {
    'UnicodeData.txt': '0007;<control>;Cc;0;BN;;;;;N;BELL;;;;\n'
                       '0041;LATIN CAPITAL LETTER A;Lu;0;L;;;;;N;;;;0061;\n'
                       '0301;COMBINING ACUTE ACCENT;Mn;230;NSM;;;;;N;;;;;\n'
                       '3164;HANGUL FILLER;Lo;0;L;<compat> 1160;;;;N;;;;;\n'
                       'FF9E;HALFWIDTH KATAKANA VOICED SOUND MARK;Lm;0;L;<narrow> 3099;;;;N;;;;;\n',
    'EastAsianWidth.txt': '# @missing: 3400..4DBF; W\n'
                          '1100..115F;W  # Hangul leading consonants\n'
                          '1F1E6..1F1FF;N\n'
                          '1F600;W\n'
                          'FF01;F\n'
                          '3000;F\n'
                          'A8FA;W\n'
                          '0600;N\n'
                          'FF9E;H\n'
                          '3164;W\n'
                          '3402..3404;W\n'
                          '3403;N # overrides the line before\n',
    'DerivedCoreProperties.txt': '0300..036F    ; Grapheme_Extend # Mn\n'
                                 'FF9E          ; Grapheme_Extend # Lm\n'
                                 '0300..036F    ; XID_Continue\n'
                                 '00AD          ; Default_Ignorable_Code_Point\n'
                                 '115F          ; Default_Ignorable_Code_Point\n'
                                 '3000          ; Default_Ignorable_Code_Point\n'
                                 '3164          ; Default_Ignorable_Code_Point\n',
    'HangulSyllableType.txt': '1100..115F ; L\n1160..11A7 ; V\n11A8..11FF ; T\n',
    'auxiliary/GraphemeBreakProperty.txt': '0600..0605 ; Prepend\n0D4E ; Prepend\n',
}


def ucd(files):
    directory = tempfile.TemporaryDirectory()
    for name, text in {**FILES, **files}.items():
        path = Path(directory.name) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
    return directory


def lookup(packed, c):
    block = packed[c >> 7]
    entry = packed[0x110000 // 128 + block * 32 + (c & 127) // 4]
    return entry >> 2 * (c & 3) & 3


class Parsing(unittest.TestCase):
    def test_spans_read_points_and_ranges_without_comments(self):
        with ucd({'PropList.txt': '# header\n\n0041 ; Dash # A\n0300..0302;Other\n'}) as directory:
            spans = list(table.spans(Path(directory) / 'PropList.txt'))
        self.assertEqual(spans, [(0x41, 0x41, 'Dash'), (0x300, 0x302, 'Other')])

    def test_missing_defaults_are_comments_like_any_other(self):
        with ucd({'EastAsianWidth.txt': '# @missing: 0000..10FFFF; N\n0041;Na\n'}) as directory:
            path = Path(directory) / 'EastAsianWidth.txt'
            self.assertEqual(list(table.spans(path)), [(0x41, 0x41, 'Na')])

    def test_values_let_later_lines_override_earlier_ones(self):
        with ucd(UCD) as directory:
            east_asian = table.values(Path(directory) / 'EastAsianWidth.txt')
        self.assertEqual(
            [east_asian[c] for c in (0x41, 0x3401, 0x3402, 0x3403, 0x3404, 0x1100, 0xFF01)],
            [None, None, 'W', 'N', 'W', 'W', 'F'],
        )

    def test_values_leave_unlisted_points_unset(self):
        with ucd(UCD) as directory:
            hangul = table.values(Path(directory) / 'HangulSyllableType.txt')
        self.assertEqual([hangul[0x115F], hangul[0x1160], hangul[0x11FF], hangul[0x1200]], ['L', 'V', 'T', None])

    def test_having_picks_named_properties_among_several_per_point(self):
        with ucd(UCD) as directory:
            path = Path(directory) / 'DerivedCoreProperties.txt'
            extend = table.having(path, 'Grapheme_Extend')
            either = table.having(path, 'Grapheme_Extend', 'Default_Ignorable_Code_Point')
        self.assertEqual(extend, set(range(0x300, 0x370)) | {0xFF9E})
        self.assertEqual(either, extend | {0xAD, 0x115F, 0x3000, 0x3164})


class Rules(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with ucd(UCD) as directory:
            cls.widths = table.widths(directory)

    def assertWidths(self, expected):
        self.assertEqual({hex(c): self.widths[c] for c in expected}, {hex(c): w for c, w in expected.items()})

    def test_every_code_point_gets_a_width(self):
        self.assertEqual(len(self.widths), 0x110000)
        self.assertLessEqual(set(self.widths), {0, 1, 2, 3})

    def test_unlisted_and_ordinary_points_are_narrow(self):
        self.assertWidths({0x41: 1, 0x10FFFF: 1, 0x0: 1, 0x1F1E6: 1, 0x1F1FF: 1})

    def test_controls_and_joining_marks_take_no_cells(self):
        self.assertWidths({0x7: 0, 0x300: 0, 0x36F: 0, 0xAD: 0, 0x0D4E: 0})

    def test_letters_that_extend_a_grapheme_still_take_their_own_cell(self):
        # Unicode 18 made the halfwidth katakana sound marks Grapheme_Extend;
        # they are spacing letters (Lm), unlike the combining marks beside them.
        self.assertWidths({0xFF9E: 1, 0x301: 0})

    def test_hangul_vowels_and_trailing_consonants_join_the_leading_one(self):
        self.assertWidths({0x1160: 0, 0x11A7: 0, 0x11A8: 0, 0x11FF: 0})

    def test_hangul_leading_consonants_are_wide_even_when_ignorable(self):
        self.assertWidths({0x1100: 2, 0x115F: 2})

    def test_default_ignorable_beats_east_asian_width(self):
        # Letters too: U+3164 HANGUL FILLER is a wide Lo.
        self.assertWidths({0x3000: 0, 0x3164: 0})

    def test_wide_and_fullwidth_points_take_two_cells(self):
        self.assertWidths({0xFF01: 2, 0x1F600: 2, 0x3402: 2, 0x3404: 2, 0x3403: 1})

    def test_missing_defaults_do_not_widen_anything(self):
        self.assertWidths({0x3400: 1, 0x3401: 1, 0x4DBF: 1})

    def test_exceptions_beat_every_property(self):
        self.assertWidths({0x600: 1, 0x605: 0, 0xA8FA: 0, 0x17D8: 3, 0x17A4: 2, 0x2D7F: 1})


class Packing(unittest.TestCase):
    def test_lookup_returns_every_width_packed(self):
        widths = [(c * 7 + c // 300) % 4 for c in range(0x110000)]
        packed = table.pack(widths)
        for c in [*range(0, 1024), *range(0x10FC00, 0x110000), 0x1F600, 0x4E00]:
            self.assertEqual(lookup(packed, c), widths[c], hex(c))

    def test_identical_blocks_are_stored_once(self):
        widths = [1] * 0x110000
        widths[0x4E00:0x4E80] = [2] * 128
        widths[0x9F00:0x9F80] = [2] * 128
        packed = table.pack(widths)
        self.assertEqual(len(packed), 0x110000 // 128 + 2 * 32)
        self.assertEqual(packed[0], 0)
        self.assertEqual(packed[0x4E00 >> 7], packed[0x9F00 >> 7])
        self.assertEqual([lookup(packed, c) for c in (0x4DFF, 0x4E00, 0x4E7F, 0x4E80)], [1, 2, 2, 1])

    def test_widths_pack_four_to_a_byte_lowest_bits_first(self):
        widths = [0] * 0x110000
        widths[1:4] = [1, 2, 3]
        packed = table.pack(widths)
        self.assertEqual(packed[0x110000 // 128 + packed[0] * 32], 0b11_10_01_00)

    def test_block_numbers_fit_one_byte_or_are_refused(self):
        def distinct(count):
            # Blank blocks, and count - 1 that differ from each other.
            widths = [0] * 0x110000
            for block in range(count - 1):
                widths[block * 128 + block % 128] = 1 + block // 128
            return widths

        self.assertEqual(len(table.pack(distinct(256))), 0x110000 // 128 + 256 * 32)
        with self.assertRaisesRegex(ValueError, '257 distinct blocks'):
            table.pack(distinct(257))


class CommandLine(unittest.TestCase):
    def test_writes_the_packed_table_where_asked(self):
        script = Path(__file__).parents[1] / 'scripts' / 'width_table.py'
        with ucd(UCD) as directory:
            out = Path(directory) / 'width.bin'
            subprocess.run([sys.executable, '-I', script, directory, out], check=True)
            packed = out.read_bytes()
        self.assertEqual([lookup(packed, c) for c in (0x41, 0x300, 0x1F600, 0x17D8)], [1, 0, 2, 3])


if __name__ == '__main__':
    unittest.main()
