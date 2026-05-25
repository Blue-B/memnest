#!/usr/bin/env python3
import json
import subprocess
import sys

ALLOWED = (
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "MPL-2.0",
    "CDLA-Permissive-2.0",
    "Unlicense",
)

DENIED_MARKERS = (
    "GPL",
    "AGPL",
    "LGPL",
    "SSPL",
    "BUSL",
)


def allowed_token(value: str) -> bool:
    cleaned = value.strip().strip("()")
    return any(token == cleaned for token in ALLOWED)


def normalize_expression(value: str) -> str:
    spaced = value.replace("(", " ( ").replace(")", " ) ").replace("/", " OR ")
    return " ".join(spaced.split())


def strip_wrapping_parens(value: str) -> str:
    candidate = value.strip()
    while candidate.startswith("(") and candidate.endswith(")"):
        depth = 0
        wraps = True
        for index, char in enumerate(candidate):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0 and index != len(candidate) - 1:
                    wraps = False
                    break
        if not wraps:
            break
        candidate = candidate[1:-1].strip()
    return candidate


def split_top_level(value: str, operator: str) -> list[str]:
    parts = []
    depth = 0
    start = 0
    tokens = value.split()
    offset = 0
    token_starts = []
    for token in tokens:
        token_starts.append(value.find(token, offset))
        offset = token_starts[-1] + len(token)

    for index, token in enumerate(tokens):
        if token == "(":
            depth += 1
        elif token == ")":
            depth -= 1
        elif depth == 0 and token.upper() == operator:
            token_start = token_starts[index]
            parts.append(value[start:token_start].strip())
            start = token_start + len(token)

    if parts:
        parts.append(value[start:].strip())
    return parts


def expression_allowed(license_value: str) -> bool:
    normalized = strip_wrapping_parens(normalize_expression(license_value))
    and_parts = split_top_level(normalized, "AND")
    if and_parts:
        return all(expression_allowed(part) for part in and_parts)
    or_parts = split_top_level(normalized, "OR")
    if or_parts:
        return any(expression_allowed(part) for part in or_parts)
    return allowed_token(normalized)


def denied_expression(license_value: str) -> bool:
    if expression_allowed(license_value):
        return False
    upper_license = normalize_expression(license_value).upper()
    return any(marker in upper_license for marker in DENIED_MARKERS)


def main() -> int:
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1"],
        text=True,
    )
    root = json.loads(metadata)
    packages = root.get("packages", [])

    failures = []
    for package in packages:
        name = package.get("name", "<unknown>")
        license_value = package.get("license") or ""
        license_file = package.get("license_file")
        if license_file:
            continue
        if not license_value:
            failures.append(f"{name}: missing license metadata")
            continue
        if denied_expression(license_value):
            failures.append(f"{name}: denied license expression {license_value}")
            continue
        if not expression_allowed(license_value):
            failures.append(f"{name}: review license expression {license_value}")

    if failures:
        print("license_check_failed", file=sys.stderr)
        for failure in failures:
            print(f" - {failure}", file=sys.stderr)
        return 1

    print("license_check_ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
