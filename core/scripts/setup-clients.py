#!/usr/bin/env python3
"""Merge Memnest into detected client configs, with atomic backups and restore."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import shlex
import shutil


def executable(name):
    return shutil.which(name) is not None


def load_json(path):
    if not path.exists():
        return {}
    try:
        value = json.loads(path.read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read JSON config {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"expected a JSON object in {path}")
    return value


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic_write_bytes(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + f".memnest-{os.getpid()}.tmp")
    try:
        temp.write_bytes(data)
        os.chmod(temp, 0o600)
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def atomic_write(path, text):
    atomic_write_bytes(path, text.encode())


def json_merge(path, mutate, save, finish):
    before = path.read_bytes() if path.exists() else None
    value = load_json(path)
    mutate(value)
    after = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    if before == after:
        return False
    entry = save(path, before)
    atomic_write_bytes(path, after)
    finish(entry, after)
    return True


def configure(home, binary, url, force_all):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    backup_dir = home / ".memnest" / "setup-backups" / stamp
    manifest = backup_dir / "manifest.json"
    entries = []

    claude = force_all or executable("claude") or (home / ".claude").exists()
    cursor = force_all or executable("cursor") or (home / ".cursor").exists()
    codex = force_all or executable("codex") or (home / ".codex").exists()

    # Reject malformed existing files before changing any other client.
    if claude:
        load_json(home / ".claude.json")
        load_json(home / ".claude" / "settings.json")
    if cursor:
        load_json(home / ".cursor" / "mcp.json")
    if codex and (home / ".codex" / "config.toml").exists():
        try:
            (home / ".codex" / "config.toml").read_text()
        except (OSError, UnicodeError) as error:
            raise ValueError(f"could not read Codex config: {error}") from error

    def persist_manifest():
        doc = {"created": stamp, "home": str(home), "files": entries}
        atomic_write(manifest, json.dumps(doc, indent=2) + "\n")

    def save(path, before):
        backup_dir.mkdir(parents=True, exist_ok=True)
        os.chmod(backup_dir.parent, 0o700)
        os.chmod(backup_dir, 0o700)
        restore_copy = backup_dir / "setup-clients.py"
        if not restore_copy.exists():
            shutil.copyfile(pathlib.Path(__file__), restore_copy)
            os.chmod(restore_copy, 0o700)
        backup = backup_dir / f"{len(entries):02d}-{path.name}.bak"
        state = "absent" if before is None else "file"
        atomic_write_bytes(backup, b"ABSENT\n" if before is None else before)
        entry = {
            "path": str(path),
            "backup": str(backup),
            "state": state,
            "post_sha256": None,
        }
        entries.append(entry)
        # Persist and expose recovery information before mutating the client file.
        persist_manifest()
        if len(entries) == 1:
            print("Recovery manifest started: " + str(manifest), flush=True)
        return entry

    def finish(entry, after):
        entry["post_sha256"] = digest(after)
        persist_manifest()

    detected = []
    if claude:
        detected.append("Claude Code")
        json_merge(
            home / ".claude.json",
            lambda cfg: cfg.setdefault("mcpServers", {}).setdefault("memnest", {"url": url + "/mcp"}),
            save,
            finish,
        )

        def hook(cfg):
            hooks = cfg.setdefault("hooks", {}).setdefault("UserPromptSubmit", [])
            command = shlex.join([str(binary), "hook", "--url", url])
            expected = [{"type": "command", "command": command}]
            if not any(item.get("hooks") == expected for item in hooks if isinstance(item, dict)):
                hooks.append({"hooks": expected})

        json_merge(home / ".claude" / "settings.json", hook, save, finish)

    if cursor:
        detected.append("Cursor")
        json_merge(
            home / ".cursor" / "mcp.json",
            lambda cfg: cfg.setdefault("mcpServers", {}).setdefault("memnest", {"url": url + "/mcp"}),
            save,
            finish,
        )

    if codex:
        detected.append("Codex")
        path = home / ".codex" / "config.toml"
        before = path.read_bytes() if path.exists() else None
        text = before.decode() if before is not None else ""
        if "[mcp_servers.memnest]" not in text:
            suffix = "" if not text or text.endswith("\n") else "\n"
            after = (text + suffix + "\n[mcp_servers.memnest]\nurl = \"" + url + "/mcp\"\n").encode()
            entry = save(path, before)
            atomic_write_bytes(path, after)
            finish(entry, after)

    if executable("pi") or (home / ".pi").exists():
        detected.append("pi (use the pi-memnest package; config left unchanged)")

    print("Detected clients: " + (", ".join(detected) if detected else "none"))
    if entries:
        command = shlex.join(["python3", str(backup_dir / "setup-clients.py"), "--restore", str(manifest)])
        print("Config backup manifest: " + str(manifest))
        print("Restore command: " + command)
        return manifest
    print("Client configs already current; no files changed.")
    return None


def restore(manifest_path, force=False):
    try:
        doc = json.loads(manifest_path.read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read backup manifest {manifest_path}: {error}") from error
    if not isinstance(doc, dict) or not isinstance(doc.get("files"), list) or not doc.get("home"):
        raise ValueError(f"invalid backup manifest {manifest_path}")

    home = pathlib.Path(doc["home"]).resolve()
    allowed = {
        (home / ".claude.json").resolve(),
        (home / ".claude" / "settings.json").resolve(),
        (home / ".cursor" / "mcp.json").resolve(),
        (home / ".codex" / "config.toml").resolve(),
    }
    manifest_dir = manifest_path.parent.resolve()
    validated = []
    conflicts = []
    for entry in doc["files"]:
        if not isinstance(entry, dict):
            raise ValueError("invalid backup manifest entry")
        path = pathlib.Path(entry.get("path", "")).resolve()
        backup = pathlib.Path(entry.get("backup", "")).resolve()
        if path not in allowed or backup.parent != manifest_dir or not backup.is_file():
            raise ValueError(f"unsafe backup manifest entry for {path}")
        current = path.read_bytes() if path.is_file() else None
        expected = entry.get("post_sha256")
        if expected is None or current is None or digest(current) != expected:
            conflicts.append(str(path))
        validated.append((entry, path, backup))

    if conflicts and not force:
        joined = "\n  ".join(conflicts)
        raise ValueError(
            "client config changed after setup; refusing to overwrite:\n  "
            + joined
            + "\nReview the files, then rerun with --force-restore only if replacement is intended."
        )

    for entry, path, backup in reversed(validated):
        if entry.get("state") == "absent":
            if path.exists():
                path.unlink()
        elif entry.get("state") == "file":
            atomic_write_bytes(path, backup.read_bytes())
        else:
            raise ValueError(f"invalid backup state for {path}")
        print("restored " + str(path))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", type=pathlib.Path, default=pathlib.Path.home())
    parser.add_argument("--bin", type=pathlib.Path, default=pathlib.Path.home() / ".local/bin/memnest")
    parser.add_argument("--url", default="http://127.0.0.1:3111")
    parser.add_argument("--all", action="store_true", help="configure all known JSON/TOML clients (mainly for validation)")
    parser.add_argument("--restore", type=pathlib.Path)
    parser.add_argument("--force-restore", action="store_true", help="overwrite client changes made after setup")
    args = parser.parse_args()
    if args.force_restore and not args.restore:
        parser.error("--force-restore requires --restore")
    if args.restore:
        restore(args.restore.expanduser().resolve(), args.force_restore)
    else:
        configure(args.home.expanduser().resolve(), args.bin.expanduser().resolve(), args.url.rstrip("/"), args.all)


if __name__ == "__main__":
    main()
