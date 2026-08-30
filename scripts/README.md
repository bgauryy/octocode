# Scripts

Root automation for the Octocode monorepo. Prefer the root `yarn …` command over
running a script directly, unless you need `--fix`/flags.

| Script | Does | Run via |
|---|---|---|
| `workspace-health.mjs` | Discovers packages/skills, topo-sorts by internal deps, runs their scripts. Backs `build`/`test`/`lint`/`typecheck`/`verify`. | `yarn build` · `yarn test` · `yarn verify` · `yarn health:report` · `yarn health:check` |
| `prepublish.mjs` | Publish guard: checks/removes local `workspace:`, `file:`, `link:`, and `portal:` resolutions. | `yarn prepublish` (check) · `node ./scripts/prepublish.mjs --fix` |
| `dev-setup.mjs` | Dev-only: resolves workspace packages from this checkout and `octocode-core` from the sibling `octocode-mcp-host`. Supports `--dry-run`, `--install`, and `--reset`. | `yarn devScript && yarn install` |
| `dedupe-deps.mjs` | Enforces one version range per external dependency across all packages (replaces syncpack). | `yarn deps:dedupe` · `yarn deps:dedupe:fix` |
| `sync-package-readmes.mjs` | Copies root `README.md` into each public package at build/prepack time. | `yarn readme:sync` |
| `docs-verify.mjs` | Validates markdown links in `docs/` and workflow README references. | `yarn docs:verify` |

## Notes

- **`dev-setup.mjs` ↔ `prepublish.mjs`** are a pair: `devScript` adds local
  workspace resolutions plus the sibling core; `prepublish.mjs --fix` removes them
  versions before publishing. Always follow either with `yarn install`.
- **Don't re-add package-local version-sync scripts** — extend `prepublish.mjs`.
- **Final publish gate** lives in the package:
  `packages/octocode/scripts/check-no-workspace-protocol.mjs` (run from each
  package's `prepublishOnly`) blocks `workspace:` deps from shipping, enforces
  version match with root, and checks engine platform packages. Engine-specific
  version/binary checks stay under `packages/octocode-engine/`.
