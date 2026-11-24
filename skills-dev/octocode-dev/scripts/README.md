# Scripts

[dev.mjs](dev.mjs) is the task runner (`--help` lists tasks); call a script directly only for a flag the task does not pass. Tests sit beside each script: `node --test skills-dev/octocode-dev/scripts/*.test.mjs skills-dev/octocode-dev/scripts/providers/*.test.mjs`. `skills:check` and `test:ci` run every `*.test.mjs` under the `COLLECTIONS` in `skills/common/build.mjs`; an empty suite fails them.

| Script | Task or use |
|---|---|
| [gates.mjs](gates.mjs) | `gates` |
| [contracts-regen.mjs](contracts-regen.mjs), [core-build.mjs](core-build.mjs) | `contracts:regen` under a machine-wide lock; builds the sibling core first |
| [core-lint-sync.mjs](core-lint-sync.mjs), [oxlint-baseline.mjs](oxlint-baseline.mjs) | part of `verify` and `check`; `oxlint-baseline.mjs --write` drops stale entries |
| [rust-lint.mjs](rust-lint.mjs) | `lint:rust` |
| [rust-quality.mjs](rust-quality.mjs) | `rust:tools`, `fuzz:smoke`, `fuzz`, `miri`, `llvm-lines` |
| [workspace-health.mjs](workspace-health.mjs) | runs a workspace script in build order (`build`, `test`, `verify`, `health:*`, `check-outputs`) |
| [docs-verify.mjs](docs-verify.mjs), [docs-pinned-links.mjs](docs-pinned-links.mjs), [check-vocabulary.mjs](check-vocabulary.mjs) | `docs:verify`; vocabulary exceptions in [vocabulary-exceptions.json](vocabulary-exceptions.json), and an entry that matches nothing fails |
| [check-research-guidance.mjs](check-research-guidance.mjs) | validates `octocode` examples in docs and skills against live schemas (needs the built CLI); last step of `verify` |
| [dev-setup.mjs](dev-setup.mjs), [dev-resolution-contract.mjs](dev-resolution-contract.mjs), [prepublish.mjs](prepublish.mjs) | `setup`, `prepublish` (`--package <name>` also fails while a public workspace dependency is not on npm) |
| [dedupe-deps.mjs](dedupe-deps.mjs) | `deps:dedupe` |
| [tool-inventory.mjs](tool-inventory.mjs) | `<tool> [--json]`: fields and their declared effects; `--schema-diff <old contract>` and `--sizes` ([tool audit](../docs/TOOL_QUALITY.md#audit-a-tool-end-to-end)) |
| [pin.mjs](pin.mjs) | `dev pin`: freeze the built binary for a round, print path and sha256 |
| [walk.mjs](walk.mjs) | `dev walk <tool> '<json>'`: follow every `next` of one call; exit 1 on a repeated page or a row that does not rebuild once |
| [artifact-freshness.mjs](artifact-freshness.mjs) | version and contract fingerprint of staged native artifacts |
| [brew-formula.mjs](brew-formula.mjs), [lsp-lock.mjs](lsp-lock.mjs) | `brew:formula`, `lsp:lock` |
| [package-build.mjs](package-build.mjs) | shared by the `octocode` and `octocode-mcp` builds: `node skills-dev/octocode-dev/scripts/package-build.mjs docs <outDir>` |
| [profile/profile.py](profile/profile.py) | `profile` |
| [providers/record.mjs](providers/record.mjs) | `answers:record`; cases in [recorded-plan.mjs](providers/recorded-plan.mjs) |

Tests: [artifact-freshness](artifact-freshness.test.mjs), [brew-formula](brew-formula.test.mjs), [build-plans](build-plans.test.mjs), [check-vocabulary](check-vocabulary.test.mjs), [core-build](core-build.test.mjs), [core-lint-sync](core-lint-sync.test.mjs), [docs-pinned-links](docs-pinned-links.test.mjs), [entry-symlink](entry-symlink.test.mjs), [gates](gates.test.mjs), [github-token](github-token.test.mjs), [lsp-lock](lsp-lock.test.mjs), [oxlint-baseline](oxlint-baseline.test.mjs), [pin](pin.test.mjs), [prepublish](prepublish.test.mjs), [profile](profile.test.mjs), [rust-lint](rust-lint.test.mjs), [rust-quality](rust-quality.test.mjs), [skill-tests](skill-tests.test.mjs), [tool-inventory](tool-inventory.test.mjs), [walk](walk.test.mjs), [workspace-health](workspace-health.test.mjs).

Scripts resolve the repo root as `../../..` from this folder (`../../../..` from `lib/`). Moving the folder means updating `ROOT` in each script, the package builders that import `package-build.mjs`, and `.github/workflows/`.
