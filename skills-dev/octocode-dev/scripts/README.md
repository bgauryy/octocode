# Scripts

Root automation. Call a script directly only for a flag `dev.mjs` does not expose; otherwise `node skills-dev/octocode-dev/scripts/dev.mjs <task>` (`--help` lists tasks). `skills:check` and `test:ci` discover `*.test.mjs` under every `COLLECTIONS` entry in `skills/common/build.mjs`; an empty suite fails them. Tests (`<name>.test.mjs` beside each script): `node --test skills-dev/octocode-dev/scripts/*.test.mjs`.

- [dev.mjs](dev.mjs): task runner for every repo-wide build/test/lint/verify/docs/deps/setup/publish task.
- [tool-inventory.mjs](tool-inventory.mjs): per-tool audit map `[tool] [--json]`.
- [workspace-health.mjs](workspace-health.mjs): runs workspace scripts in build order (`build`, `test`, `verify`, `health:report`, `health:check`, `check-outputs`).
- [prepublish.mjs](prepublish.mjs): publish guard for local `workspace:`/`file:`/`link:`/`portal:` resolutions; `--fix` removes them, `--package <name>` also fails while a public workspace dependency is not on npm.
- [dev-setup.mjs](dev-setup.mjs): `dev setup` sets the relative sibling core resolution when available and removes other legacy local overrides; `--dry-run`, `--install`, `--reset`.
- [dev-resolution-contract.mjs](dev-resolution-contract.mjs): legacy local resolutions checked by prepublish.
- [dedupe-deps.mjs](dedupe-deps.mjs): `deps:dedupe [--fix]`, one version range per external dependency, no runtime dependency repeated in `devDependencies`.
- [oxlint-baseline.mjs](oxlint-baseline.mjs): shrink-only `.oxlintrc.baseline.json` check; `--write` removes stale entries.
- [core-lint-sync.mjs](core-lint-sync.mjs): fails when the sibling core's lint setup drifts from this repo's; skips if `../octocode-mcp-host` is absent.
- [core-build.mjs](core-build.mjs): builds the sibling core before `contracts:regen`.
- [contracts-regen.mjs](contracts-regen.mjs): `contracts:regen` under the machine-wide regeneration lock (one run at a time across sessions); beside a sibling core it refuses unless core typechecks, then builds the sibling, installs the configured core and generates the contract from that release.
- [gates.mjs](gates.mjs): `gates`, the release gate set as one run: `build:dev` first; then `check`, the contract check, config tests, `docs:verify`, `skills:check` and `test:rust` side by side (cargo work queues on the build slots); then the sibling core's tests and typecheck alone. One log per gate, one summary table, exit 1 on any failure ([plan test](gates.test.mjs)).
- [rust-lint.mjs](rust-lint.mjs): `lint:rust`, clippy ratchet against shrink-only `clippy-baseline.json`; `--write` lowers, `--adopt` adds a newly listed lint.
- [profile/profile.py](profile/profile.py): `dev profile`, instructions, peak footprint, samply flamegraph, top frames; `--heap`.
- [providers/record.mjs](providers/record.mjs): `answers:record [id,...]` re-records provider answers for the native mocks; cases in [recorded-plan.mjs](providers/recorded-plan.mjs).
- [package-build.mjs](package-build.mjs): helpers shared by the `octocode` and `octocode-mcp` builds.
- [docs-verify.mjs](docs-verify.mjs): `docs:verify`, links, workflow references, tool catalog, config keys, publishing contracts.
- [docs-pinned-links.mjs](docs-pinned-links.mjs): checks commit-pinned GitHub links against local history.
- [check-research-guidance.mjs](check-research-guidance.mjs): validates `octocode` examples in the [guidance files](lib/guidance-files.mjs) against live schemas (needs the built CLI; `--self-test`); last step of `verify`.
- [check-vocabulary.mjs](check-vocabulary.mjs): checks inline-code `parent.child` paths against the [contract vocabulary](lib/contract-vocabulary.mjs); exceptions in [vocabulary-exceptions.json](vocabulary-exceptions.json). Run by `docs:verify`.
- [artifact-freshness.mjs](artifact-freshness.mjs): checks version and contract fingerprint of staged native artifacts ([test](artifact-freshness.test.mjs)).
- [brew-formula.mjs](brew-formula.mjs): `brew:formula [version] [--out FILE]` ([test](brew-formula.test.mjs)).
- [lsp-lock.mjs](lsp-lock.mjs): `lsp:lock [--check | --bump]` generates `engine/src/lsp/managed.lock.json` from GitHub release digests.
- [rust-quality.mjs](rust-quality.mjs): opt-in tools from `packages/octocode-native/dev-tools.json`: `rust:tools`, `fuzz:smoke`, `fuzz`, `miri`, `llvm-lines`; never installs.
- Support: [lib/entrypoint.mjs](lib/entrypoint.mjs), [lib/github-token.mjs](lib/github-token.mjs), [lib/skill-tests.mjs](lib/skill-tests.mjs), [lib/workspaces.mjs](lib/workspaces.mjs), [lib/workspace-checks.mjs](lib/workspace-checks.mjs), [lib/workspace-run.mjs](lib/workspace-run.mjs).

## Notes

- Tests: [check-vocabulary](check-vocabulary.test.mjs), [core-build](core-build.test.mjs), [core-lint-sync](core-lint-sync.test.mjs), [docs-pinned-links](docs-pinned-links.test.mjs), [entry-symlink](entry-symlink.test.mjs), [github-token](github-token.test.mjs), [lsp-lock](lsp-lock.test.mjs), [oxlint-baseline](oxlint-baseline.test.mjs), [prepublish](prepublish.test.mjs), [profile](profile.test.mjs), [rust-lint](rust-lint.test.mjs), [rust-quality](rust-quality.test.mjs), [skill-tests](skill-tests.test.mjs), [workspace-health](workspace-health.test.mjs), [mock-registry-proxy](providers/mock-registry-proxy.test.mjs), [recorded-answers](providers/recorded-answers.test.mjs), [recorded-shape](providers/recorded-shape.test.mjs).
- Every task that compiles Rust holds a machine-wide build slot ([build-slot.cjs](../../../packages/octocode-native/scripts/build-slot.cjs)) and waits for one by itself; `slots` prints who holds them, by pid and `OCTOCODE_LANE`; a bare `cargo` takes none ([plan test](build-plans.test.mjs)). `test:dev` takes `--crate <name>`, `--lib` or a libtest filter after `--`.
- Scripts resolve the repo root as `../../..` from this folder (`../../../..` from `lib/`). If you move the folder, update `ROOT` in each script, the package builders that import `package-build.mjs`, and `.github/workflows/`.
