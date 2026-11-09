#!/usr/bin/env python3
"""Generates src/width.bin, the terminal width of every code point,
from the Unicode Character Database of one release:

    python3 scripts/width_table.py UCD_DIR [OUT]

UCD_DIR holds UnicodeData.txt, EastAsianWidth.txt, DerivedCoreProperties.txt,
HangulSyllableType.txt and auxiliary/GraphemeBreakProperty.txt from
https://www.unicode.org/Public/. Emoji presentation needs no file of its own:
every emoji with it is East Asian Wide, except the regional indicators, which
stay narrow.

Stage 1 of the table gives each block of 128 code points a one-byte block
number; stage 2 holds each distinct block as 128 two-bit widths, four per
byte, lowest bits first.
"""
from pathlib import Path
import sys

CODE_POINTS = 0x110000
BLOCK = 128

# Widths the properties below do not decide.
EXCEPTIONS = {
    0x17A4: 2,  # KHMER INDEPENDENT VOWEL QAA, standing for two characters
    0x17D8: 3,  # KHMER SIGN BEYYAL, standing for three
    0x2D7F: 1,  # TIFINAGH CONSONANT JOINER
    0xA8FA: 0,  # DEVANAGARI CARET
    # Number signs that span the digits after them.
    **dict.fromkeys([*range(0x0600, 0x0605), 0x06DD, 0x110BD, 0x110CD], 1),
}


def spans(path):
    """Yields (first, last, value) for each data line of a UCD file. Defaults
    in `@missing` comments are left out: EastAsianWidth.txt lists every wide
    range, unassigned ones included."""
    for line in path.read_text(encoding='utf-8').splitlines():
        data = line.split('#')[0].strip()
        if data:
            span, value = [field.strip() for field in data.split(';')[:2]]
            first, _, last = span.partition('..')
            yield int(first, 16), int(last or first, 16), value


def values(path):
    """Each code point's value of the one property a UCD file lists, or None."""
    found = [None] * CODE_POINTS
    for first, last, value in spans(path):
        found[first:last + 1] = [value] * (last + 1 - first)
    return found


def having(path, *names):
    """The code points with any of the binary properties `names`, from a UCD
    file that lists several properties."""
    return {c for first, last, value in spans(path) if value in names for c in range(first, last + 1)}


def widths(ucd):
    ucd = Path(ucd)
    east_asian = values(ucd / 'EastAsianWidth.txt')
    hangul = values(ucd / 'HangulSyllableType.txt')
    extend = having(ucd / 'DerivedCoreProperties.txt', 'Grapheme_Extend')
    ignorable = having(ucd / 'DerivedCoreProperties.txt', 'Default_Ignorable_Code_Point')
    grapheme = values(ucd / 'auxiliary/GraphemeBreakProperty.txt')
    # UnicodeData.txt: the name is the first field, the category the second.
    category = [None] * CODE_POINTS
    for line in (ucd / 'UnicodeData.txt').read_text(encoding='utf-8').splitlines():
        code, _, kind = line.split(';')[:3]
        category[int(code, 16)] = kind

    def width(c):
        if c in EXCEPTIONS:
            return EXCEPTIONS[c]
        if category[c] == 'Cc' or hangul[c] in ('V', 'T') or grapheme[c] == 'Prepend':
            return 0
        # Hangul leading consonants, even the default-ignorable filler, are wide;
        # letters that extend a grapheme (the halfwidth katakana sound marks)
        # are spacing and keep their cell.
        letter = (category[c] or '').startswith('L')
        invisible = c in ignorable or (c in extend and not letter)
        if invisible and hangul[c] != 'L':
            return 0
        return 2 if east_asian[c] in ('W', 'F') else 1

    return [width(c) for c in range(CODE_POINTS)]


def pack(widths):
    blocks, stage1 = {}, []
    for start in range(0, CODE_POINTS, BLOCK):
        quads = [widths[at:at + 4] for at in range(start, start + BLOCK, 4)]
        block = bytes(sum(width << 2 * i for i, width in enumerate(quad)) for quad in quads)
        stage1.append(blocks.setdefault(block, len(blocks)))
    if len(blocks) > 256:
        raise ValueError(f'{len(blocks)} distinct blocks do not fit one-byte numbers')
    return bytes(stage1) + b''.join(blocks)


if __name__ == '__main__':
    out = Path(sys.argv[2] if len(sys.argv) > 2 else Path(__file__).parents[1] / 'src/width.bin')
    out.write_bytes(pack(widths(sys.argv[1])))
