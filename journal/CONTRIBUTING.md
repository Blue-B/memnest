# Contributing

Thanks for the interest. Total code is under 900 lines including the
smoke harness — contributions are easy to land but should stay focused.

## Scope

- **In scope**: bug fixes, new backends (mem0/agentmemory/Letta/Zep adapters in `src/journal.mjs`), import-side resilience, docs, CI.
- **Out of scope**: heavyweight runtime deps (must stay zero-native-deps when possible), opinionated UI, replacing palimpsest core.

## Dev loop

```bash
git clone https://github.com/Blue-B/palimpsest
cd palimpsest-journal
npm run smoke         # 14 assertions, requires palimpsest running on :3111
npm run smoke:bun     # same, under bun (faster)
```

## PR checklist

- [ ] `npm run smoke` passes 3 times in a row (the test is sensitive to BM25 indexing race conditions; rerun to confirm).
- [ ] `npm pack --dry-run` shows the right `files` whitelist.
- [ ] CHANGELOG.md updated.
- [ ] If you added a new backend adapter: update the README capability table.

## License

By contributing you agree your code is released under the MIT license.
