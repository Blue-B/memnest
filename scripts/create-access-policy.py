#!/usr/bin/env python3
"""Create a private starter policy and credentials without printing token values.

This does not install or restart a service. Review the files before enabling them.
"""

import argparse
import hashlib
import json
import os
import secrets
from pathlib import Path


def private_write(path, text):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        output.write(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--read", action="append", default=[])
    parser.add_argument("--write", action="append", default=[])
    args = parser.parse_args()
    if not set(args.write) <= set(args.read):
        parser.error("every --write project needs a matching --read grant")
    for project in args.read + args.write:
        if not project.strip() or project.strip() != project or project in {"all", "_trash", "_superseded"} or len(project.encode()) > 256 or any(ord(c) < 32 for c in project):
            parser.error("use explicit non-reserved project names")
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    tokens = {name: secrets.token_hex(32) for name in ["admin", "client"]}
    policy = {"principals": [
        {"id": "admin", "admin": True, "token_sha256": hashlib.sha256(tokens["admin"].encode()).hexdigest()},
        {"id": "client", "token_sha256": hashlib.sha256(tokens["client"].encode()).hexdigest(), "read_projects": args.read, "write_projects": args.write},
    ]}
    private_write(args.out / "policy.json", json.dumps(policy, ensure_ascii=False, indent=2) + "\n")
    private_write(args.out / "admin.env", f"MEMNEST_TOKEN={tokens['admin']}\nMEMNEST_ACCESS_POLICY={args.out.resolve() / 'policy.json'}\n")
    private_write(args.out / "client.env", f"MEMNEST_TOKEN={tokens['client']}\n")
    print(f"Created private policy and credential files in {args.out.resolve()}")
    print("Review policy.json; load the matching environment in the service/client. Token values were not printed.")


if __name__ == "__main__":
    main()
