#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import stat
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).with_name("setup-clients.py")
spec = importlib.util.spec_from_file_location("setup_clients", MODULE_PATH)
assert spec is not None and spec.loader is not None
setup_clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup_clients)


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise AssertionError(f"invalid test JSON at {path}: {error}") from error


class SetupClientsTest(unittest.TestCase):
    def test_merge_is_idempotent_and_restore_is_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            original = '{"keep": {"user": true}}\n'
            (home / ".claude").mkdir()
            (home / ".claude.json").write_text(original)
            binary = pathlib.Path("/opt/Memnest Tools/memnest")
            manifest = setup_clients.configure(home, binary, "http://127.0.0.1:3111", True, autocontext=True)
            self.assertIsNotNone(manifest)
            claude = read_json(home / ".claude.json")
            self.assertTrue(claude["keep"]["user"])
            self.assertEqual(claude["mcpServers"]["memnest"]["url"], "http://127.0.0.1:3111/mcp")
            settings = read_json(home / ".claude" / "settings.json")
            hook_command = settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            self.assertEqual(hook_command, "'/opt/Memnest Tools/memnest' hook --url http://127.0.0.1:3111")
            manifest_doc = read_json(manifest)
            self.assertTrue(all(entry["post_sha256"] for entry in manifest_doc["files"]))
            self.assertEqual(stat.S_IMODE(manifest.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE(manifest.parent.stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE((manifest.parent / "setup-clients.py").stat().st_mode), 0o700)
            self.assertTrue(all(stat.S_IMODE(pathlib.Path(entry["backup"]).stat().st_mode) == 0o600 for entry in manifest_doc["files"]))
            first_manifest = manifest.read_text()
            self.assertIsNone(setup_clients.configure(home, binary, "http://127.0.0.1:3111", True, autocontext=True))
            self.assertEqual(manifest.read_text(), first_manifest)
            setup_clients.restore(manifest)
            self.assertEqual((home / ".claude.json").read_text(), original)
            self.assertFalse((home / ".claude" / "settings.json").exists())
            self.assertFalse((home / ".cursor" / "mcp.json").exists())
            self.assertFalse((home / ".codex" / "config.toml").exists())

    def test_default_has_tools_without_recall_and_preserves_opted_in_hooks(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            binary = pathlib.Path("/opt/memnest")
            url = "http://127.0.0.1:3111"
            setup_clients.configure(home, binary, url, True)
            settings = home / ".claude" / "settings.json"
            self.assertFalse(settings.exists(), "default setup must not install a prompt hook")
            self.assertIn("memnest", read_json(home / ".claude.json")["mcpServers"])
            self.assertTrue((home / ".codex" / "config.toml").exists())
            setup_clients.configure(home, binary, url, True, autocontext=True)
            before = settings.read_bytes()
            self.assertIn("UserPromptSubmit", read_json(settings)["hooks"])
            self.assertIsNone(setup_clients.configure(home, binary, url, True))
            self.assertEqual(settings.read_bytes(), before, "existing hooks are never removed by default setup")

    def test_restore_refuses_later_changes_without_force(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            manifest = setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True)
            (home / ".claude.json").write_text('{"changed_after_setup": true}\n')
            with self.assertRaisesRegex(ValueError, "changed after setup"):
                setup_clients.restore(manifest)
            self.assertTrue((home / ".cursor" / "mcp.json").exists())
            setup_clients.restore(manifest, force=True)
            self.assertFalse((home / ".claude.json").exists())
            self.assertFalse((home / ".cursor" / "mcp.json").exists())

    def test_malformed_config_is_rejected_before_any_write(self):
        for path, invalid, message in (
            (pathlib.Path(".claude/settings.json"), "not json", "could not read JSON config"),
            (pathlib.Path(".codex/config.toml"), 'model = "unterminated', "could not read TOML config"),
        ):
            with self.subTest(path=path), tempfile.TemporaryDirectory() as directory:
                home = pathlib.Path(directory)
                (home / ".claude").mkdir()
                original = '{"existing": true}\n'
                (home / ".claude.json").write_text(original)
                target = home / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(invalid)
                with self.assertRaisesRegex(ValueError, message):
                    setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True)
                self.assertEqual((home / ".claude.json").read_text(), original)
                self.assertFalse((home / ".memnest").exists())

    def test_existing_memnest_entries_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            (home / ".claude").mkdir()
            (home / ".claude.json").write_text(json.dumps({"mcpServers": {"memnest": {"url": "http://custom/mcp"}}}))
            (home / ".codex").mkdir()
            (home / ".codex" / "config.toml").write_text('[mcp_servers.memnest]\nurl = "http://custom/mcp"\n')
            setup_clients.configure(home, pathlib.Path("/opt/memnest"), "http://127.0.0.1:3111", True)
            self.assertEqual(read_json(home / ".claude.json")["mcpServers"]["memnest"]["url"], "http://custom/mcp")
            self.assertIn("http://custom/mcp", (home / ".codex" / "config.toml").read_text())


if __name__ == "__main__":
    unittest.main()
