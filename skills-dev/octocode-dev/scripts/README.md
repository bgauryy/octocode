# Scripts

Root automation for the Octocode monorepo. Call a script directly only for a flag that `dev.mjs` does not expose.

| Script | Does | Run via |
|---|---|---|
| `dev.mjs` | Task runner: every repo-wide build/test/lint/verify/docs/deps/setup/publish task by name. | `node skills-dev/octocode-dev/scripts/dev.mjs <task>` |
| `tool-inventory.mjs` | Per-tool audit map (native module, evidence files, zero-hit and undescribed fields). | `node skills-dev/octocode-dev/scripts/tool-inventory.mjs [tool] [--json]` |
| `workspace-health.mjs` | Discovers packages/skills, topo-sorts builds by internal deps, runs their scripts (`run <script>`, `verify`, `check`, `report`, `check-outputs`). `verify` checks dependencies/docs and every workspace-owned verification gate; build/lint/test is the fallback only when no verify script exists. | `node skills-dev/octocode-dev/scripts/dev.mjs build` · `node skills-dev/octocode-dev/scripts/dev.mjs test` · `node skills-dev/octocode-dev/scripts/dev.mjs verify` · `node skills-dev/octocode-dev/scripts/dev.mjs health:report` · `node skills-dev/octocode-dev/scripts/dev.mjs health:check` |
| `workspace-health.test.mjs` | Proves extra workspace gates execute and failures propagate. | Included in `dev.mjs verify`; `node --test skills-dev/octocode-dev/scripts/workspace-health.test.mjs` |
| `prepublish.mjs` | Publish guard: checks/removes local `workspace:`, `file:`, `link:`, and `portal:` resolutions. | `node skills-dev/octocode-dev/scripts/dev.mjs prepublish` (check) · `node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix` |
| `dev-setup.mjs` | Dev-only: resolves workspace packages from this checkout and `octocode-core` from the sibling `octocode-mcp-host`. Supports `--dry-run`, `--install`, and `--reset`. | `node skills-dev/octocode-dev/scripts/dev.mjs setup && yarn install` |
| `dev-resolution-contract.mjs` | The one list of local resolutions that `dev-setup.mjs` adds and `prepublish.mjs` removes. | Imported by both; not run by hand |
| `dedupe-deps.mjs` | Enforces one version range per external dependency and rejects runtime dependencies repeated in `devDependencies` (replaces syncpack). | `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe` · `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe --fix` |
| `esbuild-package.mjs` | Shared Node-package builder (used by the `octocode-mcp` build); emits each entry point and rejects external runtime imports missing from the package manifest. | Package `build` / `build:dev` scripts; not run by hand |
| `package-build.mjs` | Build helpers shared by the `octocode` and `octocode-mcp` builds: the bundle's external list (Node built-ins plus published runtime dependencies) and copying the repo `docs/` into the package output. | Imported by `packages/octocode/build.mjs` and `packages/octocode-mcp/buildConfig.mjs`; package `build` scripts run `package-build.mjs docs <outDir>` |
| `runtime-import-contract.mjs` | Normalizes bare import specifiers and implements the build-time dependency ownership check shared by package builders (imported by `packages/octocode/build.mjs`). | Imported by build scripts; not run by hand |
| `dev-unify-check.mjs` | Opt-in drift check for the native `dev-unify` feature: fails when the first `test:rust` selection compiles a crate that `build:dev` also builds, but with other features or profile settings (so every `cargo test` recompiles it). Uses nightly `cargo --unit-graph`, falls back to `cargo tree` feature sets (`--tree` forces it), skips without cargo. Not part of `verify` or CI. | `node skills-dev/octocode-dev/scripts/dev.mjs check:dev-unify [--json] [--tree]` · tests: `node --test skills-dev/octocode-dev/scripts/dev-unify-check.test.mjs` |
| `docs-verify.mjs` | Validates links, workflow references, the public tool catalog, configuration keys, and publishing contracts. | `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` |

## Notes

- Follow `dev-setup.mjs` or `prepublish.mjs --fix` with `yarn install`.
- Paths are root-relative. Scripts resolve the repo root as `../../..` from this folder. If you move the folder, update `ROOT` in each script, the package builders that import `esbuild-package.mjs` / `runtime-import-contract.mjs`, and `.github/workflows/`.
- Do not re-add package-local version-sync scripts. Workspace packages version independently; native-package scripts own platform-package version checks.
- Packages have no `prepublishOnly` gates. Runtime/engine version and artifact checks live under `packages/octocode-native/` (`version:check`, `platforms:check`) and run on demand.
