#!/usr/bin/env python3
"""Regenerate core/THIRD_PARTY_NOTICES.md from Cargo.lock.

Resolves the crates that are actually linked into the memnest binary for a
target triple (normal and optional dependencies, no dev or build-only
dependencies), reads their license files out of the local cargo registry
checkout, and writes the attribution document.

Usage:
    python3 scripts/generate-third-party-notices.py [--check] [--target TRIPLE]

--check exits 1 when the committed file differs from a fresh render, so CI can
fail on a stale notices file instead of shipping one.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

CORE = Path(__file__).resolve().parent.parent
OUTPUT = CORE / "THIRD_PARTY_NOTICES.md"

LICENSE_FILE_PATTERN = re.compile(
    r"^(licen[cs]e|copying|notice|unlicense)([-_.].*)?$", re.IGNORECASE
)
# Only real holder lines: "Copyright (c) 2015 Somebody", "Copyright 2019 Somebody".
# Prose from the Apache-2.0 body and its "Copyright [yyyy]" placeholder are not
# attributions and must not end up in the table.
COPYRIGHT_PATTERN = re.compile(
    r"^\s*(?:#\s*)?copyright\s+(?:\(c\)|©|\d{4})", re.IGNORECASE
)

# Components that are not crates.io dependencies but still ship inside, or are
# pulled in by, a memnest install. Kept here because Cargo.lock cannot describe
# them.
EXTERNAL_COMPONENTS = """\
## Components that are not crates.io dependencies

### ONNX Runtime

- Upstream: [microsoft/onnxruntime](https://github.com/microsoft/onnxruntime)
- License: MIT

memnest embeds ONNX Runtime through the `ort` and `ort-sys` crates. `ort-sys`
does not vendor the library: its build script downloads a prebuilt ONNX Runtime
distribution for the host target from `cdn.pyke.io`, verifies it against a
SHA-256 pinned in the crate, and links it into `memnest`. That download happens
at build time, so a published memnest binary already contains ONNX Runtime and
does not fetch it later. Set `ORT_LIB_LOCATION` to link a runtime you built or
audited yourself instead.

### intfloat/multilingual-e5-base

- Upstream: [intfloat/multilingual-e5-base](https://huggingface.co/intfloat/multilingual-e5-base)
- License: MIT

The embedding model is not part of the binary or the release archive. The first
embedding operation, which is the first write, the first semantic search, or an
explicit `memnest --warmup-embedding`, downloads the ONNX weights and tokenizer
from Hugging Face into `<data-dir>/models/`. Nothing is downloaded when memnest
only starts. `MEMNEST_EMBED_MODEL` selects a different model, and those models
carry their own licenses, which this file does not track.
"""


def cargo_metadata(target: str) -> dict:
    raw = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            target,
            "--manifest-path",
            str(CORE / "Cargo.toml"),
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(raw)


def linked_packages(metadata: dict) -> list[dict]:
    """Packages reachable from the root through non-dev, non-build edges."""
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    packages = {package["id"]: package for package in metadata["packages"]}
    root = metadata["resolve"]["root"]

    seen: set[str] = set()
    stack = [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            kinds = {kind.get("kind") for kind in dep.get("dep_kinds", [])}
            if kinds and kinds <= {"dev", "build"}:
                continue
            stack.append(dep["pkg"])
    seen.discard(root)
    return sorted(
        (packages[pkg_id] for pkg_id in seen),
        key=lambda package: (package["name"].lower(), package["version"]),
    )


def license_files(package: dict) -> list[Path]:
    directory = Path(package["manifest_path"]).parent
    if not directory.is_dir():
        return []
    return sorted(
        path
        for path in directory.iterdir()
        if path.is_file() and LICENSE_FILE_PATTERN.match(path.name)
    )


def read_text(path: Path) -> str | None:
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None
    # Trailing whitespace only; the wording is reproduced verbatim. Upstream
    # license files are full of it and `git diff --check` complains otherwise.
    return "\n".join(line.rstrip() for line in raw.splitlines()).strip()


def copyright_lines(texts: list[str]) -> list[str]:
    found: list[str] = []
    for text in texts:
        for line in text.splitlines():
            if COPYRIGHT_PATTERN.match(line):
                cleaned = line.strip().replace("|", "/")
                if cleaned not in found:
                    found.append(cleaned)
    return found


def render(packages: list[dict], target: str) -> str:
    texts_by_hash: dict[str, str] = {}
    users_by_hash: dict[str, list[str]] = {}
    rows: list[str] = []
    missing: list[str] = []

    for package in packages:
        label = f"{package['name']} {package['version']}"
        license_expression = package.get("license") or "see repository"
        repository = package.get("repository") or ""
        texts = []
        for path in license_files(package):
            text = read_text(path)
            if text:
                texts.append(text)

        if not texts:
            missing.append(label)

        for text in texts:
            digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
            texts_by_hash.setdefault(digest, text)
            users_by_hash.setdefault(digest, [])
            if label not in users_by_hash[digest]:
                users_by_hash[digest].append(label)

        holders = copyright_lines(texts)
        holder = holders[0] if holders else ""
        link = f"[{package['name']}]({repository})" if repository else package["name"]
        rows.append(
            f"| {link} | {package['version']} | {license_expression} | {holder} |"
        )

    ordered = sorted(users_by_hash.items(), key=lambda item: item[1][0].lower())

    lines = [
        "# Third-party notices",
        "",
        "<!-- markdownlint-disable MD013 MD024 MD033 -->",
        "",
        "<!-- Generated by scripts/generate-third-party-notices.py. Do not edit by hand. -->",
        "",
        "memnest ships as a single Rust binary that statically links the crates listed",
        "below. Regenerate this file after any dependency change:",
        "",
        "```bash",
        "python3 scripts/generate-third-party-notices.py",
        "```",
        "",
        f"The list is resolved from `Cargo.lock` for `{target}` with default features",
        "(`viewer` and `mcp`), following normal and optional dependencies only. Dev and",
        "build-only dependencies are excluded because they are not linked into the",
        "shipped binary. Other Linux targets resolve to the same set.",
        "",
        EXTERNAL_COMPONENTS,
        "## Release gate",
        "",
        "Before cutting a release, run:",
        "",
        "```bash",
        "python3 scripts/check-licenses.py",
        "python3 scripts/generate-third-party-notices.py --check",
        "```",
        "",
        "The first check fails on missing license metadata, copyleft markers that require",
        "legal review, or unknown license expressions. The second fails when this file is",
        "stale. Passing both does not replace legal review, but it prevents accidental",
        "releases with obvious license metadata problems.",
        "",
        "## Current policy",
        "",
        "Allowed license families for automatic release checks:",
        "",
        "- MIT",
        "- Apache-2.0",
        "- BSD-2-Clause",
        "- BSD-3-Clause",
        "- ISC",
        "- Unicode-3.0",
        "- Zlib",
        "- MPL-2.0",
        "- CDLA-Permissive-2.0",
        "- Unlicense",
        "",
        "Denied markers for automatic release checks:",
        "",
        "- GPL",
        "- AGPL",
        "- LGPL",
        "- SSPL",
        "- BUSL",
        "",
        "Any new dependency outside the allowed list must be reviewed before release.",
        "",
        f"## Rust dependencies ({len(packages)} crates)",
        "",
        "| Crate | Version | License | Copyright |",
        "| --- | --- | --- | --- |",
        *rows,
        "",
    ]

    if missing:
        lines += [
            "### Crates without a license file in their published package",
            "",
            "These crates declare a license in their metadata but do not ship the text.",
            "The declared expression above governs them; the canonical text for each",
            "identifier appears in the section below.",
            "",
            *(f"- {label}" for label in missing),
            "",
        ]

    lines += [
        "## License texts",
        "",
        "Identical texts are listed once, with the crates that ship them.",
        "",
    ]

    for index, (digest, users) in enumerate(ordered, start=1):
        lines += [
            f"### {index}. {users[0]}"
            + (f" and {len(users) - 1} other crates" if len(users) > 1 else ""),
            "",
            "<details><summary>Used by</summary>",
            "",
            *(f"- {label}" for label in users),
            "",
            "</details>",
            "",
            "```text",
            texts_by_hash[digest],
            "```",
            "",
        ]

    return "\n".join(lines).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--target", default="x86_64-unknown-linux-gnu")
    args = parser.parse_args()

    metadata = cargo_metadata(args.target)
    rendered = render(linked_packages(metadata), args.target)

    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            print(
                f"{OUTPUT.relative_to(CORE)} is stale; "
                "run python3 scripts/generate-third-party-notices.py",
                file=sys.stderr,
            )
            return 1
        print("third_party_notices_ok")
        return 0

    OUTPUT.write_text(rendered, encoding="utf-8")
    print(f"wrote {OUTPUT.relative_to(CORE)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
