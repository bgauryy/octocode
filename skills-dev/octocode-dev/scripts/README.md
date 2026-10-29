# Scripts

Root automation for the Octocode monorepo. Call a script directly only for a flag that `dev.mjs` does not expose.

`skills:check` and `test:ci` discover `*.test.mjs` files recursively under `skills/`, `skills-beta/`, and `skills-dev/`. An empty suite fails these tasks before Node can auto-discover other repository tests. The `skills/common` workspace and this dev skill run their own tests through their workspace gates. The ignored local `octocode-orchestrator-local-worker` skill is not part of these shipped roots; its model-backed eval is opt-in and does not run in CI.

| Script | Does | Run via |
|---|---|---|
| [dev.mjs](dev.mjs) | Task runner: every repo-wide build/test/lint/verify/docs/deps/setup/publish task by name. | `node skills-dev/octocode-dev/scripts/dev.mjs <task>` |
| [tool-inventory.mjs](tool-inventory.mjs) | Per-tool audit map (native module, evidence files, zero-hit and undescribed fields). | `node skills-dev/octocode-dev/scripts/tool-inventory.mjs [tool] [--json]` |
| [workspace-health.mjs](workspace-health.mjs) | Discovers packages/skills, topo-sorts builds by internal deps, runs their scripts (`run <script>`, `verify`, `check`, `report`, `check-outputs`). `verify` checks dependencies/docs and every workspace-owned verification gate; build/lint/test is the fallback only when no verify script exists. | `node skills-dev/octocode-dev/scripts/dev.mjs build` · `node skills-dev/octocode-dev/scripts/dev.mjs test` · `node skills-dev/octocode-dev/scripts/dev.mjs verify` · `node skills-dev/octocode-dev/scripts/dev.mjs health:report` · `node skills-dev/octocode-dev/scripts/dev.mjs health:check` |
| [workspace-health.test.mjs](workspace-health.test.mjs) | Proves extra workspace gates and the layering rules execute and failures propagate. | `dev.mjs verify` runs every `*.test.mjs` here; `node --test skills-dev/octocode-dev/scripts/*.test.mjs` |
| [prepublish.mjs](prepublish.mjs) | Publish guard: checks/removes local `workspace:`, `file:`, `link:`, and `portal:` resolutions. With `--package <name>`, also fails while a public workspace dependency of `<name>` is not on npm at its workspace version. | `node skills-dev/octocode-dev/scripts/dev.mjs prepublish` (check) · `node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix` · `node ./skills-dev/octocode-dev/scripts/prepublish.mjs --package @octocodeai/pi-extension` · tests: `node --test skills-dev/octocode-dev/scripts/prepublish.test.mjs` |
| [dev-setup.mjs](dev-setup.mjs) | Dev-only: resolves workspace packages from this checkout and `octocode-core` from the sibling `octocode-mcp-host`. Supports `--dry-run`, `--install`, and `--reset`. | `node skills-dev/octocode-dev/scripts/dev.mjs setup && yarn install` |
| [dev-resolution-contract.mjs](dev-resolution-contract.mjs) | The one list of local resolutions that `dev-setup.mjs` adds and `prepublish.mjs` removes. | Imported by both; not run by hand |
| [dedupe-deps.mjs](dedupe-deps.mjs) | Enforces one version range per external dependency and rejects runtime dependencies repeated in `devDependencies` (replaces syncpack). | `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe` · `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe --fix` |
| [check-file-length.mjs](check-file-length.mjs) | 400-physical-line limit for every `.ts`/`.js`/`.rs` source file against the shrink-only `file-length-baseline.json`; `--write` drops stale entries and lowers counts, never adds. | `dev.mjs lint`, `lint:ci`, `verify`; tests: [check-file-length.test.mjs](check-file-length.test.mjs) |
| [oxlint-baseline.mjs](oxlint-baseline.mjs) | Checks the shrink-only `.oxlintrc.baseline.json` (per-file overrides for violations that predate a rule) against an oxlint run without it; `--write` removes and lowers entries, never adds. | `dev.mjs lint`, `lint:ci`, `verify`; tests: [oxlint-baseline.test.mjs](oxlint-baseline.test.mjs) |
| [core-lint-sync.mjs](core-lint-sync.mjs) | Fails when the sibling core's copies drift from this repo's canonical lint setup: `scripts/oxlint-baseline.mjs` byte-identical, `.oxlintrc.json` with the same rules (layout keys and `packages/...` overrides excepted). Skips with a note when `../octocode-mcp-host` is absent. | `dev.mjs verify`; tests: [core-lint-sync.test.mjs](core-lint-sync.test.mjs) |
| [package-build.mjs](package-build.mjs) | Build helpers shared by the `octocode` and `octocode-mcp` builds: the bundle's external list (Node built-ins plus published runtime dependencies), the build-time check that every external runtime import is a declared dependency, and copying the repo `docs/` into the package output. | Imported by `packages/octocode/build.mjs` and `packages/octocode-mcp/build.mjs`; package `build` scripts run `package-build.mjs docs <outDir>` |
| [docs-verify.mjs](docs-verify.mjs) | Validates links, workflow references, the public tool catalog, configuration keys, and publishing contracts. | `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` |
| [check-research-guidance.mjs](check-research-guidance.mjs) | Validates every `octocode` example in `skills/octocode-research` (JSON `{tool,query}` blocks and CLI lines) against the live tool schemas; needs the built CLI. | Last step of `dev.mjs verify`; `node skills-dev/octocode-dev/scripts/check-research-guidance.mjs [--self-test]` |

| [artifact-freshness.mjs](artifact-freshness.mjs) | Checks the embedded version and contract fingerprint of staged native artifacts. | Imported by workspace health; tests: [artifact-freshness.test.mjs](artifact-freshness.test.mjs) |
| [brew-formula.mjs](brew-formula.mjs) | Renders the Homebrew formula from published npm platform payloads. | `node skills-dev/octocode-dev/scripts/dev.mjs brew:formula [version] [--out FILE]`; [smoke-query regression](brew-formula.test.mjs) executes the generated request against the built CLI during `dev.mjs verify` |
| [rust-quality.mjs](rust-quality.mjs) | Opt-in Rust quality tools from `packages/octocode-native/dev-tools.json`: pinned-version inventory, `pins <tool...>` (the `name@version` list every CI install reads), libFuzzer campaigns, the selected Miri tests, cargo-llvm-lines. Never installs anything. | `dev.mjs rust:tools`, `fuzz:smoke`, `fuzz`, `miri`, `llvm-lines`; tests: [rust-quality.test.mjs](rust-quality.test.mjs) |

Shared script support: [entry-point detection](lib/entrypoint.mjs) preserves invocation through installed skill symlinks.

[Standalone skill test discovery](lib/skill-tests.mjs) supplies exact test paths to `dev.mjs`; [its regression test](skill-tests.test.mjs) covers nested, beta, dev, hidden, and workspace-owned paths.

`workspace-health.mjs` parses its mode and delegates to [workspace discovery and build order](lib/workspaces.mjs), [contract checks](lib/workspace-checks.mjs) and [script runs](lib/workspace-run.mjs).

Related checks: [publish guard](prepublish.test.mjs), [symlinked entry points](entry-symlink.test.mjs).

## Notes

- Follow `dev-setup.mjs` or `prepublish.mjs --fix` with `yarn install`.
- Paths are root-relative. Scripts resolve the repo root as `../../..` from this folder (`../../../..` from `lib/`). If you move the folder, update `ROOT` in each script, the package builders that import `package-build.mjs`, and `.github/workflows/`.
