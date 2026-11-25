"""Writes an app's Info.plist: the base one with the keys of a partial
plist (actool's, naming the app icon) added over it.

    python3 -I merge_plist.py <base> <partial> <output>
"""
import plistlib
import sys


def merge(base, partial):
    """`base` with each key of `partial` set over it."""
    merged = dict(base)
    merged.update(partial)
    return merged


def main(base_path, partial_path, output_path):
    with open(base_path, "rb") as base, open(partial_path, "rb") as partial:
        merged = merge(plistlib.load(base), plistlib.load(partial))
    with open(output_path, "wb") as output:
        plistlib.dump(merged, output)


if __name__ == "__main__":
    main(*sys.argv[1:4])
