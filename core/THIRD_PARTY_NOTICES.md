# Third-party notices

Memnest is distributed as a Rust binary and includes third-party Rust dependencies.

## Release gate

Before cutting a release, run:

```bash
python3 scripts/check-licenses.py
```

The check fails on missing license metadata, copyleft markers that require legal review, or unknown license expressions. Passing the check does not replace legal review, but it prevents accidental releases with obvious license metadata problems.

## Current policy

Allowed license families for automatic release checks:

- MIT
- Apache-2.0
- BSD-2-Clause
- BSD-3-Clause
- ISC
- Unicode-3.0
- Zlib
- MPL-2.0
- CDLA-Permissive-2.0

Denied markers for automatic release checks:

- GPL
- AGPL
- LGPL
- SSPL
- BUSL

Any new dependency outside the allowed list must be reviewed before release.
