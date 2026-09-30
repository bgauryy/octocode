# Scripts

Root automation for the Octocode monorepo. The root `package.json` no longer
wraps these; run tasks through `dev.mjs` (`node skills-dev/octocode-dev/scripts/dev.mjs --help`).
Extra arguments pass through to the underlying script.

| Script | Does | Run via |
|---|---|---|
| `dev.mjs` | Task runner: every repo-wide build/test/lint/verify/docs/deps/setup/publish task by name. | `node skills-dev/octocode-dev/scripts/dev.mjs <task>` |
| `tool-inventory.mjs` | Per-tool audit map (native module, evidence files, zero-hit and undescribed fields). | `node skills-dev/octocode-dev/scripts/tool-inventory.mjs [tool] [--json]` |
| `workspace-health.mjs` | Discovers packages/skills, topo-sorts by internal deps, runs their scripts. `verify` also checks dependency declarations before package verification. | `node skills-dev/octocode-dev/scripts/dev.mjs build` · `node skills-dev/octocode-dev/scripts/dev.mjs test` · `node skills-dev/octocode-dev/scripts/dev.mjs verify` · `node skills-dev/octocode-dev/scripts/dev.mjs health:report` · `node skills-dev/octocode-dev/scripts/dev.mjs health:check` |
| `prepublish.mjs` | Publish guard: checks/removes local `workspace:`, `file:`, `link:`, and `portal:` resolutions. | `node skills-dev/octocode-dev/scripts/dev.mjs prepublish` (check) · `node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix` |
| `dev-setup.mjs` | Dev-only: resolves workspace packages from this checkout and `octocode-core` from the sibling `octocode-mcp-host`. Supports `--dry-run`, `--install`, and `--reset`. | `node skills-dev/octocode-dev/scripts/dev.mjs setup && yarn install` |
| `dedupe-deps.mjs` | Enforces one version range per external dependency and rejects runtime dependencies repeated in `devDependencies` (replaces syncpack). | `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe` · `node skills-dev/octocode-dev/scripts/dev.mjs deps:dedupe --fix` |
| `esbuild-package.mjs` | Shared Node-package builder; emits each entry point and rejects external runtime imports missing from the package manifest. | Package `build` / `build:dev` scripts |
| `runtime-import-contract.mjs` | Normalizes bare import specifiers and implements the build-time dependency ownership check shared by package builders. | Imported by build scripts |
| `docs-verify.mjs` | Validates links, workflow references, the public tool catalog, configuration keys, and publishing contracts. | `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` |

## Notes

- **`dev-setup.mjs` ↔ `prepublish.mjs`** are a pair: `devScript` adds local
  workspace resolutions plus the sibling core; `prepublish --fix` removes them
  before publishing. Always follow either with `yarn install`.
- **Paths are root-relative.** Scripts resolve the repo root as `../../..` from
  this folder; moving the folder means updating `ROOT` in each script, the
  package builders that import `esbuild-package.mjs` /
  `runtime-import-contract.mjs`, and `.github/workflows/`.
- **Don't re-add package-local version-sync scripts.** Workspace packages version
  independently; native-package scripts own platform-package version checks.
- **Final publish gate** lives in the package:
  `packages/octocode/scripts/check-no-workspace-protocol.mjs` (run from each
  package's `prepublishOnly`) blocks local dependency protocols from shipping.
  Consolidated runtime/engine version and four-artifact checks live under
  `packages/octocode-native/`.
