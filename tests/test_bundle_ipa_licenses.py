"""ios/bundle-ipa.sh: a package's license is found by the package's name,
whatever its commands are called."""
import re
import unittest
from pathlib import Path

SCRIPT = (Path(__file__).resolve().parent.parent / "ios" / "bundle-ipa.sh").read_text()


class LicenseLoopTest(unittest.TestCase):
    def loop(self):
        return SCRIPT[SCRIPT.index("for package in target/ios-packages") :]

    def test_the_package_name_is_not_the_variable_embed_command_overwrites(self):
        function = SCRIPT[SCRIPT.index("embed_command()") : SCRIPT.index("for command in target/ios-commands")]
        assigned = set(re.findall(r"^\s*(\w+)=", function, re.M))
        loop = self.loop()
        package_name = re.search(r"(\w+)=\$\(basename \"\$package\"\)", loop).group(1)
        self.assertNotIn(package_name, assigned)

    def test_the_license_is_looked_up_under_the_package_name(self):
        loop = self.loop()
        self.assertIn('share/doc/$package_name', loop)
        self.assertNotIn('share/doc/$name', loop)

    def test_every_package_command_is_embedded_before_its_license_is_copied(self):
        loop = self.loop()
        self.assertLess(loop.index("embed_command"), loop.index("licenses/$package_name"))


if __name__ == "__main__":
    unittest.main()
