# Doc and config hygiene

Load when reviewing inline comments, docs, or config files for verbosity, dead prose, redundant keys, or misplaced settings.

## Comments and JSDoc

| Keep | Remove |
|---|---|
| Non-obvious invariant or constraint | Syntax narration (what the next line does) |
| External contract or spec reference | Commented-out dead code or keys with no explanation |
| Known edge case with no obvious fix | TODO with no owner and no ticket |
| License header | Block copied verbatim from elsewhere in the file |
| Non-obvious parameter constraint; return invariant the type does not express | `@param x — the x value` (type restatement); `@returns the result` |
| `@throws` with a named error class; `@see` with a live reference | Docs for removed params; `@deprecated` with no migration path |

If reading the code answers the question, cut the comment. Do not add a comment that explains a removed comment. Do not move junk prose into the docs folder.

## Documentation files

- One concept per file; ≤ 300 lines. Do not pad a short doc to look thorough.
- README: what + install/use + one example; not an internal API reference.
- ARCHITECTURE.md: layer map + data flow + key constraints; not tutorials.
- API reference: one subsystem's public surface only.
- Cross-link; never duplicate. Duplicated prose diverges.

## Config files

A line count over the threshold is a signal to audit for redundancy, not a delete gate: `tsconfig.json` 60; `eslint.config.*` / `.eslintrc.*` 100; `package.json` (scripts + deps) 150; `vitest.config.*` / `jest.config.*` 80; CI/CD `*.yaml` per job file 200; `.env.example` 50.

| Redundancy signal | Action |
|---|---|
| Same key in base config and extending config | Remove from the extending file; rely on inheritance |
| `overrides` / `rules` restating a preset default | Delete |
| Duplicate `scripts` with different names, same command | Keep the canonical name; delete the alias |
| `paths` alias mirroring the real module path | Delete after confirming no import uses it |

Project-wide config lives at the repository root or a dedicated `config/` directory. Package-local config lives inside the package and must not reference paths outside it. Never store secrets in committed config; flag and stop if found.

Consent gate: changes that affect runtime behavior (env vars, aliases, compiler flags) need explicit user consent before the edit. Prose-only removals (comments, dead keys) may proceed within an approved batch.

Next: return to `references/cleanup-playbook.md` EXCISE.
