# Fix and verify

Load before editing and before reporting done. Changes cross two repos and a generated embed; the wrong order ships a stale contract that tests still pass on.

## Before editing

- Baseline the affected package tests/lint and save the output to scratch; attribute every later failure against it.
- `git status` the files you will touch — another session may own in-flight edits there.

## Landing order

```mermaid
flowchart TD
  K{Change in} -- Core: schema, description, instructions --> C1["edit CORE"]
  C1 --> C2["core: yarn test && yarn lint"]
  C2 --> C3["yarn contracts:regen at the monorepo root: refreshes the file: core copy, then regenerates packages/octocode-config/contract/"]
  C3 --> C4["declare any new field in field-effect-coverage.json"]
  C4 --> NB
  K -- Native runtime or engine --> N1["edit"]
  N1 --> N2["cargo fmt + clippy on the touched crate"]
  N2 --> N3["yarn workspace @octocodeai/octocode-native test:rust: nextest; bacon test while editing"]
  N3 --> NB
  K -- Config contract --> G1["edit config-contract.json"]
  G1 --> G2["yarn workspace @octocodeai/config generate:config-contract"]
  G2 --> NB["native rebuild: yarn workspace @octocodeai/octocode-native build:dev; build.rs embeds contract/ in place (config change: regenerates the struct)"]
  K -- CLI or MCP --> I1["yarn workspace octocode build:dev, or yarn workspace octocode-mcp build:dev; MCP tests: test"]
```

Every branch except CLI or MCP ends at the native rebuild. `<repo>/AGENTS.md` draws the core and native-change flows.

Provenance records the core package version; release order is in `docs/RELEASE.md` (owned by the human).

## Verify through the real path

1. `yarn workspace @octocodeai/config check:tool-contract` — `contract/` matches core.
2. `$OCTO schema <tool> --view query` shows the changed contract.
3. Re-run the reproducing call from the finding via CLI; it now passes.
4. Re-run via a restarted host server or a fresh stdio MCP session.
5. Execute required `next.*` and response continuations to termination. Check affected typed `related[]` calls as fresh requests; `hints[]` contains prose.
6. Re-run `scripts/tool-inventory.mjs <tool>`; the fixed field is no longer flagged.
7. Package tests + lint green vs baseline.

Claim–evidence: record each check as passed / failed / skipped / unavailable with its command and scope.
