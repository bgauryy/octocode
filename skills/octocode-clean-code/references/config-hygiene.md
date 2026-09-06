# Config Hygiene

Load when inspecting config files for length, redundant keys, or misplaced settings. Why: config files are the runtime contract; silent junk there is invisible until deployment.

## Length limits

| Config type | Audit threshold |
|-------------|-----------------|
| `tsconfig.json` | 60 lines |
| `eslint.config.*` / `.eslintrc.*` | 100 lines |
| `package.json` (scripts + deps only) | 150 lines |
| `vitest.config.*` / `jest.config.*` | 80 lines |
| `*.yaml` CI/CD pipeline (per job file) | 200 lines |
| `.env.example` | 50 lines |

Files exceeding the threshold are a signal to audit for redundancy — not a hard delete gate.

## Redundancy signals

| Signal | Action |
|--------|--------|
| Same key in base config and extending config | Remove from the extending file; rely on inheritance |
| Commented-out key with no explanation | Delete |
| `overrides` / `rules` restating a preset default | Delete |
| Duplicate `scripts` with different names, same command | Keep the canonical name; delete the alias |
| `paths` alias mirroring the real module path | Delete after confirming no import uses it |

## Config placement

- Project-wide config lives at the repository root or a dedicated `config/` directory.
- Package-local config lives inside the package; must not reference paths outside the package.
- Never store secrets or credentials in committed config; flag and stop if found.

## Consent gate

Changes affecting runtime behavior (env vars, aliases, compiler flags) require explicit user consent before edit. Prose-only removals (comments, dead keys) may proceed within an approved batch.

Next: step ends here; return to `references/cleanup-playbook.md` EXCISE phase.
