#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).with_name("setup-clients.py")
spec = importlib.util.spec_from_file_location("setup_clients", MODULE_PATH)
assert spec is not None and spec.loader is not None
setup_clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup_clients)


class SetupClientsTest(unittest.TestCase):
    def test_merge_is_idempotent_and_restore_is_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            original = '{"keep": {"user": true}}\n'
            (home / ".claude").mkdir()
            (home / ".claude.json").write_text(original)
            manifest = setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True)
            self.assertIsNotNone(manifest)
            claude = json.loads((home / ".claude.json").read_text())
            self.assertTrue(claude["keep"]["user"])
            self.assertEqual(claude["mcpServers"]["memnest"]["url"], "http://127.0.0.1:3111/mcp")
            first_manifest = manifest.read_text()
            self.assertIsNone(setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True))
            self.assertEqual(manifest.read_text(), first_manifest)
            setup_clients.restore(manifest)
            self.assertEqual((home / ".claude.json").read_text(), original)
            self.assertFalse((home / ".claude" / "settings.json").exists())
            self.assertFalse((home / ".cursor" / "mcp.json").exists())
            self.assertFalse((home / ".codex" / "config.toml").exists())

    def test_existing_memnest_entries_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            (home / ".claude").mkdir()
            (home / ".claude.json").write_text(json.dumps({"mcpServers": {"memnest": {"url": "http://custom/mcp"}}}))
            (home / ".codex").mkdir()
            (home / ".codex" / "config.toml").write_text('[mcp_servers.memnest]\nurl = "http://custom/mcp"\n')
            setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True)
            self.assertEqual(json.loads((home / ".claude.json").read_text())["mcpServers"]["memnest"]["url"], "http://custom/mcp")
            self.assertIn("http://custom/mcp", (home / ".codex" / "config.toml").read_text())


if __name__ == "__main__":
    unittest.main()
