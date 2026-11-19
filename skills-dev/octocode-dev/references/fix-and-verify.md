# Fix and verify

Load before editing and before reporting done; the wrong order ships a stale contract that tests still pass on.

## Before editing

- Baseline the affected package tests/lint into scratch to attribute later failures; `git status` the files you will touch (another session may own in-flight edits).

## Landing order

- Core (schema, description, instructions): edit CORE; core `yarn test && yarn lint`; `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen` at the monorepo root (refreshes the `file:` core copy, then regenerates `packages/octocode-config/contract/`); declare a new field in `field-effect-coverage.json` or confirm it inherits its ancestor's effect (the `field_effect_coverage` test gates it); native rebuild.
- Native runtime or engine: edit; `cargo fmt` + clippy on the touched crate; `yarn workspace @octocodeai/octocode-native test:rust` (nextest; `bacon test` while editing); native rebuild.
- Config contract: edit `scripts/config-contract/declaration/` (Zod); `yarn workspace @octocodeai/config generate:config-contract`; native rebuild.
- Native rebuild: `yarn workspace @octocodeai/octocode-native build:dev`; `build.rs` embeds `contract/` in place (a config change regenerates the struct).
- CLI or MCP: `yarn workspace octocode build:dev` or `yarn workspace octocode-mcp build:dev`; MCP tests: `test`.

Every branch except CLI or MCP ends at the native rebuild. `<repo>/AGENTS.md` draws the core and native-change flows.

## Verify through the real path

1. `yarn workspace @octocodeai/config check:tool-contract` (`contract/` matches core); `$OCTO schema <tool> --view query` shows the change.
2. Re-run the finding's reproducing call via CLI and via a restarted host server or fresh stdio MCP session.
3. Execute required `next.*` and response continuations to termination. Check affected typed `related[]` calls as fresh requests; `hints[]` is prose.
4. Re-run `scripts/tool-inventory.mjs <tool>`; the field is no longer flagged.
5. Package tests and lint green vs baseline.

Record each check as passed / failed / skipped / unavailable with command and scope.
