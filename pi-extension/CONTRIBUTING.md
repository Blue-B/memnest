# Contributing

Thanks for the interest. This is a tiny bridge (~300 lines of source), so
contributions are easy to land but easy to keep out of scope.

## Scope

- **In scope**: bug fixes, new HTTP-backed tools (proxy of upstream palimpsest endpoints), docs, CI, security improvements.
- **Out of scope**: re-implementing palimpsest core features in JS (file a PR upstream at [badlogic/palimpsest](https://github.com/badlogic/palimpsest) instead), heavyweight runtime deps, alternative storage backends.

## Dev loop

```bash
git clone https://github.com/Blue-B/pi-palimpsest
cd pi-palimpsest
npm install       # runs prepare -> npm run build
npm run smoke     # 30 assertions, requires palimpsest running on :3111
npm run e2e       # 11 assertions; stop systemd palimpsest first (see test file)
```

## PR checklist

- [ ] `npm run build` succeeds and `dist/index.mjs` is regenerated.
- [ ] `npm run smoke` passes (30/30).
- [ ] If you added a tool: extend the README tools table and the smoke `EXPECTED` array.
- [ ] If you touched secret handling: update `SECURITY.md` audit checklist.
- [ ] CHANGELOG.md has an entry under `Unreleased`.

## License

By contributing you agree your code is released under the MIT license.
