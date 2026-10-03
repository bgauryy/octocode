# Fix and verify

Load before editing and before reporting done. Changes cross two repos and a generated embed; the wrong order ships a stale contract that tests still pass on.

## Before editing

- Baseline the affected package tests/lint and save the output to scratch; attribute every later failure against it.
- `git status` the files you will touch — another session may own in-flight edits there.

## Landing order

| Change in | Steps |
|---|---|
| Core (schema, description, instructions) | edit `CORE` → core `yarn test && yarn lint` → `yarn contracts:regen` at the monorepo root (refreshes the `file:` core **copy**, then regenerates `packages/octocode-config/contract/`) → declare any new field in `field-effect-coverage.json` → native rebuild (build.rs embeds `contract/` in place) |
| Native runtime / engine | edit → `cargo fmt` + clippy on touched crate → `yarn workspace @octocodeai/octocode-native test:rust` (or nextest) → `yarn workspace @octocodeai/octocode-native build:dev` |
| Config contract | edit `config-contract.json` → `yarn workspace @octocodeai/config generate:config-contract` → native rebuild (build.rs regenerates the struct) |
| CLI / MCP | `yarn workspace octocode build:dev` / `yarn workspace octocode-mcp build:dev`; MCP tests `test:contracts` |

Provenance records the core package version; release order is in `docs/RELEASE.md` (owned by the human).

## Verify through the real path

1. `yarn workspace @octocodeai/config check:tool-contract` — `contract/` matches core.
2. `$OCTO scheme <tool> --view query` shows the changed contract.
3. Re-run the reproducing call from the finding via CLI; it now passes.
4. Re-run via a restarted host server or a fresh stdio MCP session.
5. Execute any `next.*` pages and `hints.*` leads the change touches, pages to termination.
6. Re-run `scripts/tool-inventory.mjs <tool>`; the fixed field is no longer flagged.
7. Package tests + lint green vs baseline.

Record each check as passed / failed / skipped / unavailable with its command and scope.
