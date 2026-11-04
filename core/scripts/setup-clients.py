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

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10, e.g. Ubuntu 22.04
    tomllib = None


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


def configure(home, binary, url, force_all, autocontext=False):
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
    if codex:
        load_json(home / ".codex" / "hooks.json")
    codex_unchecked = False
    if codex and (home / ".codex" / "config.toml").exists():
        path = home / ".codex" / "config.toml"
        if tomllib is None:
            # Without a TOML parser, leave an existing Codex config untouched.
            codex, codex_unchecked = False, True
        else:
            try:
                tomllib.loads(path.read_text())
            except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
                raise ValueError(f"could not read TOML config {path}: {error}") from error

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

    # Task cards need SessionStart and UserPromptSubmit hooks, so they are installed
    # by default in their cards-only form. Automatic recall stays opt-in.
    cards_command = shlex.join([str(binary), "hook", "--url", url, "--no-recall"])
    recall_command = shlex.join([str(binary), "hook", "--url", url])

    def ours(handler):
        try:
            argv = shlex.split(handler.get("command", ""))
        except ValueError:
            return False
        return len(argv) >= 2 and pathlib.Path(argv[0]).name == "memnest" and argv[1] == "hook"

    def add_hooks(cfg):
        events = cfg.setdefault("hooks", {})
        for event in ("SessionStart", "UserPromptSubmit"):
            groups = events.setdefault(event, [])
            recall = autocontext and event == "UserPromptSubmit"
            if recall:
                # Upgrade only our own cards-only entry; other commands are left alone.
                for group in groups:
                    if isinstance(group, dict) and isinstance(group.get("hooks"), list):
                        group["hooks"] = [h for h in group["hooks"] if not (isinstance(h, dict) and h.get("command") == cards_command)]
                groups[:] = [g for g in groups if not (isinstance(g, dict) and g.get("hooks") == [])]
            handlers = [h for g in groups if isinstance(g, dict) for h in g.get("hooks", []) if isinstance(h, dict)]
            if not any(ours(h) for h in handlers):
                groups.append({"hooks": [{"type": "command", "command": recall_command if recall else cards_command}]})

    detected = []
    if claude:
        detected.append("Claude Code")
        json_merge(
            home / ".claude.json",
            lambda cfg: cfg.setdefault("mcpServers", {}).setdefault("memnest", {"url": url + "/mcp"}),
            save,
            finish,
        )

        json_merge(home / ".claude" / "settings.json", add_hooks, save, finish)

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
        add = ""
        if "[mcp_servers.memnest]" not in text:
            add += "\n[mcp_servers.memnest]\nurl = \"" + url + "/mcp\"\n"
        # task_card only writes handoff notes; asking before each save would interrupt every task.
        if "[mcp_servers.memnest.tools.task_card]" not in text:
            add += '\n[mcp_servers.memnest.tools.task_card]\napproval_mode = "approve"\n'
        suffix = "" if not text or text.endswith("\n") else "\n"
        after = (text + suffix + add).encode()
        valid = True
        if add and tomllib is not None:
            try:
                tomllib.loads(after.decode())
            except tomllib.TOMLDecodeError:
                valid = False
                detected.append("Codex (config.toml not extended: an existing memnest entry has an unexpected shape)")
        if add and valid:
            entry = save(path, before)
            atomic_write_bytes(path, after)
            finish(entry, after)
        if json_merge(home / ".codex" / "hooks.json", add_hooks, save, finish):
            detected.append("Codex hooks added: run /hooks in Codex once to trust them")

    if codex_unchecked:
        detected.append(
            "Codex (config left unchanged: Python 3.11+ is needed to validate it; "
            'add [mcp_servers.memnest] url = "' + url + '/mcp" yourself)'
        )

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
        (home / ".codex" / "hooks.json").resolve(),
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
    parser.add_argument("--autocontext", action="store_true", help="opt in to Claude Code and Codex prompt-time memory recall (task card hooks are installed either way; other hooks are preserved)")
    parser.add_argument("--restore", type=pathlib.Path)
    parser.add_argument("--force-restore", action="store_true", help="overwrite client changes made after setup")
    args = parser.parse_args()
    if args.force_restore and not args.restore:
        parser.error("--force-restore requires --restore")
    if args.restore:
        restore(args.restore.expanduser().resolve(), args.force_restore)
    else:
        configure(args.home.expanduser().resolve(), args.bin.expanduser().resolve(), args.url.rstrip("/"), args.all, args.autocontext)


if __name__ == "__main__":
    main()
