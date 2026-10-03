# Doc and config hygiene

Load for comments, docs, or config files with verbose or dead prose, redundant keys, or misplaced settings.

| Keep comment | Remove |
|---|---|
| Non-obvious invariant or constraint | Syntax narration |
| External contract or spec reference | Unexplained commented-out code, keys, or old implementation blocks |
| Known edge case with no obvious fix | TODO with no owner and no ticket, once obsolete or captured elsewhere (age proves nothing) |
| License header | Block copied from elsewhere in the file |
| Non-obvious parameter constraint; return invariant the type cannot express | `@param x — the x value`; `@returns the result` |
| `@throws` with a named error class; `@see` with a live reference | Docs for removed params; `@deprecated` with no migration path, once caller updates are complete |

If the code answers the question, cut the comment. Do not comment on a removed comment. Do not move junk prose into docs.

Docs: no padding. README = what + install/use + one example. ARCHITECTURE.md = layer map + data flow + key constraints. API reference = one subsystem's public surface.

More config audit counts: `eslint.config.*` / `.eslintrc.*` 100; `vitest.config.*` / `jest.config.*` 80; CI/CD `*.yaml` 200 per job file; `.env.example` 50.

| Redundancy | Act |
|---|---|
| Same key in base and extending config | Remove from the extending file |
| `overrides` / `rules` restating a preset default | Delete |
| Duplicate `scripts`, same command | Keep the canonical name |
| `paths` alias mirroring the real path | Delete after no import uses it |

- Project-wide config lives at the root or in `config/`. Package-local config stays inside the package and never references outside paths.

Next: run the batch with `references/cleanup-playbook.md`.
