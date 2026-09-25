# Fix and verify

Load before editing and before reporting done. Why: changes cross two repos and a generated embed; the wrong order ships a stale contract that tests still pass on.

## Before editing

- Baseline the affected package tests/lint and save the output to scratch; attribute every later failure against it.
- `git status` the files you will touch — another session may own in-flight edits there. Never commit or stash.

## Landing order

| Change in | Steps |
|---|---|
| Core (schema, description, instructions) | edit `CORE` → core `yarn test && yarn lint` → `yarn install` in the monorepo (core resolves via `file:` as a **copy**, not a link) → `OCTOCODE_CORE_DIR=<abs CORE> yarn workspace @octocodeai/octocode-native contracts:regen` → update `field-effect-coverage.json` (fingerprint + new/removed fields) → native rebuild |
| Native runtime / engine | edit → `cargo fmt` + clippy on touched crate → `yarn workspace @octocodeai/octocode-native test:rust` (or nextest) → `yarn workspace @octocodeai/octocode-native build:dev` |
| Config contract | edit `config-contract.json` → `yarn workspace @octocodeai/config generate:config-contract` → native rebuild (build.rs regenerates the struct) |
| CLI / MCP | `yarn workspace octocode build:dev` / `yarn workspace octocode-mcp build:dev`; MCP tests `test:contracts` |

Regen provenance records `sourceDirty`; a dirty-core regen is fine for local verification but note it in the report — release needs a clean core commit + regen (owned by the human).

## Verify through the real path

1. `yarn workspace @octocodeai/octocode-native contracts:check` — embed matches core.
2. `$OCTO scheme <tool> --view query` shows the changed contract.
3. Re-run the reproducing call from the finding via CLI; it now passes.
4. Re-run via MCP. **A running MCP server keeps the old binary/contract until restarted** — restart the host's server (or spawn a fresh stdio session) before claiming MCP behavior.
5. Execute any `next.*` continuations the change touches, to termination.
6. Re-run `scripts/tool-inventory.mjs <tool>`; the fixed field is no longer flagged.
7. Package tests + lint green vs baseline; coverage floors unchanged or raised.

Record each check as passed / failed / skipped / unavailable with its command. Never report a surface as verified if it was not exercised.
