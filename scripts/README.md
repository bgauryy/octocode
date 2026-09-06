# Scripts

Root automation for the Octocode monorepo. Prefer the root `yarn …` command over
running a script directly, unless you need `--fix`/flags.

| Script | Does | Run via |
|---|---|---|
| `workspace-health.mjs` | Discovers packages/skills, topo-sorts by internal deps, runs their scripts. `verify` also checks dependency declarations before package verification. | `yarn build` · `yarn test` · `yarn verify` · `yarn health:report` · `yarn health:check` |
| `prepublish.mjs` | Publish guard: checks/removes local `workspace:`, `file:`, `link:`, and `portal:` resolutions. | `yarn prepublish` (check) · `node ./scripts/prepublish.mjs --fix` |
| `dev-setup.mjs` | Dev-only: resolves workspace packages from this checkout and `octocode-core` from the sibling `octocode-mcp-host`. Supports `--dry-run`, `--install`, and `--reset`. | `yarn devScript && yarn install` |
| `dedupe-deps.mjs` | Enforces one version range per external dependency and rejects runtime dependencies repeated in `devDependencies` (replaces syncpack). | `yarn deps:dedupe` · `yarn deps:dedupe:fix` |
| `esbuild-package.mjs` | Shared tools-core/MCP builder; emits each entry point and rejects external runtime imports missing from the package manifest. | Package `build` / `build:dev` scripts |
| `runtime-import-contract.mjs` | Normalizes bare import specifiers and implements the build-time dependency ownership check shared by package builders. | Imported by build scripts |
| `sync-package-readmes.mjs` | Copies root `README.md` into public packages at build/prepack time. A package with `octocode.readmeSync: false` owns its README and is skipped. | `yarn readme:sync` |
| `docs-verify.mjs` | Validates links, workflow references, the public tool catalog, configuration keys, and publishing contracts. | `yarn docs:verify` |

## Notes

- **`dev-setup.mjs` ↔ `prepublish.mjs`** are a pair: `devScript` adds local
  workspace resolutions plus the sibling core; `prepublish.mjs --fix` removes them
  before publishing. Always follow either with `yarn install`.
- **Don't re-add package-local version-sync scripts.** Workspace packages version
  independently; engine-specific scripts own platform-package version checks.
- **Final publish gate** lives in the package:
  `packages/octocode/scripts/check-no-workspace-protocol.mjs` (run from each
  package's `prepublishOnly`) blocks local dependency protocols from shipping.
  Engine-specific version/binary checks stay under `packages/octocode-engine/`.
