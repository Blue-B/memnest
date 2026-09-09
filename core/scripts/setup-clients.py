#!/usr/bin/env python3
"""Merge Memnest into detected client configs, with atomic backups and restore."""
import argparse
import datetime
import json
import os
import pathlib
import shutil


def executable(name):
    return shutil.which(name) is not None


def load_json(path):
    if not path.exists():
        return {}
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError(f"expected a JSON object in {path}")
    return value


def atomic_write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + f".memnest-{os.getpid()}.tmp")
    temp.write_text(text)
    os.chmod(temp, 0o600)
    os.replace(temp, path)


def json_merge(path, mutate, save):
    before = path.read_bytes() if path.exists() else None
    value = load_json(path)
    mutate(value)
    after = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    if before == after:
        return False
    save(path, before)
    atomic_write(path, after.decode())
    return True


def configure(home, binary, url, force_all):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    backup_dir = home / ".memnest" / "setup-backups" / stamp
    entries = []

    def save(path, before):
        backup_dir.mkdir(parents=True, exist_ok=True)
        backup = backup_dir / f"{len(entries):02d}-{path.name}.bak"
        if before is None:
            backup.write_text("ABSENT\n")
            state = "absent"
        else:
            backup.write_bytes(before)
            state = "file"
        entries.append({"path": str(path), "backup": str(backup), "state": state})

    detected = []
    claude = force_all or executable("claude") or (home / ".claude").exists()
    if claude:
        detected.append("Claude Code")
        json_merge(home / ".claude.json", lambda cfg: cfg.setdefault("mcpServers", {}).setdefault("memnest", {"url": url + "/mcp"}), save)
        def hook(cfg):
            hooks = cfg.setdefault("hooks", {}).setdefault("UserPromptSubmit", [])
            command = str(binary) + " hook --url " + url
            if not any(item.get("hooks") == [{"type": "command", "command": command}] for item in hooks if isinstance(item, dict)):
                hooks.append({"hooks": [{"type": "command", "command": command}]})
        json_merge(home / ".claude" / "settings.json", hook, save)

    cursor = force_all or executable("cursor") or (home / ".cursor").exists()
    if cursor:
        detected.append("Cursor")
        json_merge(home / ".cursor" / "mcp.json", lambda cfg: cfg.setdefault("mcpServers", {}).setdefault("memnest", {"url": url + "/mcp"}), save)

    codex = force_all or executable("codex") or (home / ".codex").exists()
    if codex:
        detected.append("Codex")
        path = home / ".codex" / "config.toml"
        before = path.read_bytes() if path.exists() else None
        text = before.decode() if before is not None else ""
        if "[mcp_servers.memnest]" not in text:
            save(path, before)
            suffix = "" if not text or text.endswith("\n") else "\n"
            atomic_write(path, text + suffix + "\n[mcp_servers.memnest]\nurl = \"" + url + "/mcp\"\n")

    if executable("pi") or (home / ".pi").exists():
        detected.append("pi (use the pi-memnest package; config left unchanged)")

    manifest = None
    if entries:
        manifest = backup_dir / "manifest.json"
        manifest.write_text(json.dumps({"created": stamp, "files": entries}, indent=2) + "\n")
        os.chmod(manifest, 0o600)
    print("Detected clients: " + (", ".join(detected) if detected else "none"))
    if manifest:
        print("Config backup manifest: " + str(manifest))
    else:
        print("Client configs already current; no files changed.")
    return manifest


def restore(manifest_path):
    doc = json.loads(manifest_path.read_text())
    for entry in reversed(doc["files"]):
        path, backup = pathlib.Path(entry["path"]), pathlib.Path(entry["backup"])
        if entry["state"] == "absent":
            if path.exists():
                path.unlink()
        else:
            atomic_write(path, backup.read_text())
        print("restored " + str(path))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", type=pathlib.Path, default=pathlib.Path.home())
    parser.add_argument("--bin", type=pathlib.Path, default=pathlib.Path.home() / ".local/bin/memnest")
    parser.add_argument("--url", default="http://127.0.0.1:3111")
    parser.add_argument("--all", action="store_true", help="configure all known JSON/TOML clients (mainly for validation)")
    parser.add_argument("--restore", type=pathlib.Path)
    args = parser.parse_args()
    if args.restore:
        restore(args.restore)
    else:
        configure(args.home.expanduser(), args.bin.expanduser().resolve(), args.url.rstrip("/"), args.all)


if __name__ == "__main__":
    main()
