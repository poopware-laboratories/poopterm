"""ios/Info.plist: the Documents folder shows in the Files app."""
import plistlib
import unittest
from pathlib import Path

PLIST = Path(__file__).resolve().parent.parent / "ios" / "Info.plist"


class FileSharingTest(unittest.TestCase):
    def setUp(self):
        self.info = plistlib.loads(PLIST.read_bytes())

    def test_documents_are_shared_with_the_files_app(self):
        self.assertIs(self.info.get("UIFileSharingEnabled"), True)

    def test_files_open_in_place_rather_than_as_copies(self):
        self.assertIs(self.info.get("LSSupportsOpeningDocumentsInPlace"), True)

    def test_the_ssh_server_may_listen_on_the_local_network(self):
        reason = self.info.get("NSLocalNetworkUsageDescription", "")
        self.assertIn("sshd start", reason)
        self.assertGreater(len(reason), 40)

    def test_the_other_keys_the_app_needs_are_still_there(self):
        self.assertEqual(self.info["CFBundleExecutable"], "Poopterm")
        self.assertIn("stikdebug", self.info["LSApplicationQueriesSchemes"])


if __name__ == "__main__":
    unittest.main()
