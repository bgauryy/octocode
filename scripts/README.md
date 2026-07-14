# Scripts

Root-level automation scripts for the Octocode monorepo. Prefer root `yarn …`
commands over package-local one-offs unless a package README says otherwise.

## Daily commands

| Need | Command | Notes |
|---|---|---|
| Build all | `yarn build` | Runs `yarn prepublish` first, then workspace builds. |
| Test/lint/typecheck all | `yarn test` · `yarn lint` · `yarn typecheck` | Routed through `workspace-health.mjs`. |
| Full verification | `yarn verify` | CI-grade workspace check. |
| Local dev resolutions | `yarn devScript && yarn install` | Uses `dev-setup.mjs`; keeps internal packages resolved from this checkout. |
| Publish prep/check | `yarn prepublish` | Runs `prepublish.mjs`, the shared publish guard, then README sync. |
| Publish prep/fix | `node ./scripts/prepublish.mjs --fix && yarn install` | Removes dev `workspace:*` resolutions and aligns versions. |

## Authoritative scripts

### `dev-setup.mjs`

Important dev-only entrypoint. Adds `workspace:*` root `resolutions` for internal
packages and engine platform packages so transitive consumers resolve to local
workspace builds instead of npm. Idempotent.

Undo before publish with `node ./scripts/prepublish.mjs --fix`, then run
`yarn install`.

### `prepublish.mjs`

Single publish-prep authority. It checks/fixes:

1. root `workspace:*` resolutions for managed packages,
2. publishable Octocode package versions against root `package.json`,
3. managed internal dependency versions.

Use check-only via `yarn prepublish` or direct fix via
`node ./scripts/prepublish.mjs --fix`. Do not re-add package-local version sync
scripts; extend this file instead.

### `workspace-health.mjs`

Discovers packages/skills, resolves dependency order, and runs package scripts.
Root `build`, `test`, `lint`, `typecheck`, and `verify` use it.

Useful direct modes: `yarn health:report`, `yarn health:check`, and
`node ./scripts/workspace-health.mjs run <script>`.

### `dedupe-deps.mjs`

Workspace dependency-version deduper for external dependencies.

Commands: `yarn deps:dedupe`, `yarn deps:dedupe:fix`.

### `sync-package-readmes.mjs`

Copies the root `README.md` into publishable packages.

Command: `yarn readme:sync`.

### `docs-verify.mjs`

Validates markdown links in `docs/` and workflow README references.

Command: `yarn docs:verify`.

## Package-owned publish guards

`packages/octocode/scripts/check-no-workspace-protocol.mjs` is the shared final
publish guard used by publishable packages. It checks no `workspace:` protocol
ships, publish package versions match root, internal deps are current, and engine
platform packages match `@octocodeai/octocode-engine`.

Engine-only version/binary scripts live under `packages/octocode-engine/scripts/`
and `packages/octocode-engine/npm/`; keep those package-specific checks there.
