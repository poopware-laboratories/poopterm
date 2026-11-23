"""ios/merge_plist.py: the icon keys actool names go into the app's plist."""
import os
import plistlib
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "ios"))
import merge_plist  # noqa: E402


class MergeTest(unittest.TestCase):
    def test_partial_keys_are_added_and_the_rest_kept(self):
        base = {"CFBundleName": "Poopterm", "MinimumOSVersion": "17.0"}
        partial = {"CFBundleIcons": {"CFBundlePrimaryIcon": {"CFBundleIconName": "icon"}}}
        merged = merge_plist.merge(base, partial)
        self.assertEqual(merged["CFBundleName"], "Poopterm")
        self.assertEqual(merged["CFBundleIcons"]["CFBundlePrimaryIcon"]["CFBundleIconName"], "icon")
        self.assertEqual(len(merged), 3)

    def test_a_partial_key_replaces_the_bases(self):
        merged = merge_plist.merge({"CFBundleIcons": {"old": True}}, {"CFBundleIcons": {"new": True}})
        self.assertEqual(merged["CFBundleIcons"], {"new": True})

    def test_the_inputs_are_left_as_they_were(self):
        base, partial = {"a": 1}, {"b": 2}
        merge_plist.merge(base, partial)
        self.assertEqual((base, partial), ({"a": 1}, {"b": 2}))

    def test_files_round_trip_through_main(self):
        with tempfile.TemporaryDirectory() as folder:
            paths = [os.path.join(folder, name) for name in ("base", "partial", "out")]
            for path, data in zip(paths, ({"CFBundleName": "P"}, {"CFBundleIcons~ipad": {"x": 1}})):
                with open(path, "wb") as file:
                    plistlib.dump(data, file)
            merge_plist.main(*paths)
            with open(paths[2], "rb") as file:
                self.assertEqual(plistlib.load(file), {"CFBundleName": "P", "CFBundleIcons~ipad": {"x": 1}})


if __name__ == "__main__":
    unittest.main()
