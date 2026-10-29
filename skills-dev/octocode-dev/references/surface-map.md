Load when locating any layer of a tool before auditing or editing it. A fix in the wrong layer drifts from the source of truth.

# Surface map

Paths beginning `crates/` are relative to `packages/octocode-native/`; other paths are relative to the monorepo root (`<repo>`). `CORE` is the sibling `../octocode-mcp-host/packages/octocode-core`.

## Layers per tool

| Layer | Owner path | Audit question |
|---|---|---|
| Tool definition (authored) | `CORE/src/toolContract/tools/<tool>.ts` (one `{ name, title, description, schema }` per tool; `tools/<tool>/` sub-files when over 400 lines), shared builders in `CORE/src/toolContract/tools/_{fields,read,local,concepts}.ts` | Fields, defaults, bounds, variants, cross-field rules, description |
| Native rule catalog | `CORE/src/toolContract/nativeRules/` (opcode list per tool, parity fixtures) | Rules that native replays at prepare time |
| Descriptions | each tool file's `description` (`<when>…</when><workflow>…</workflow>`); field `.describe()` text; `CORE/src/toolContract/published{,Forms,Json}.ts` publishes the exact admitted forms | Tool/field prose shown to agents |
| MCP + CLI instructions | `CORE/src/toolContract/instructions.ts` (`MCP_INSTRUCTIONS`, one string); the native CLI appends its `<cli>` line (`crates/cli/src/cli/schema.rs`) | Shared workflow guidance |
| Output schemas / limits | `CORE/src/toolContract/{outputSchemas,outputEnvelope,outputResponse,limits}.ts` | Internal validation of produced results |
| Contract hub (re-export only) | `packages/octocode-config/src/contracts/{schema,mcp}.ts` | Must stay a thin `export *` |
| Tool contract (generated, embedded by native build.rs) | `packages/octocode-config/contract/{tool-contract.json,provenance.json,contract-fixtures.json,tool-types.schema.json,tool_types.rs}` | Never hand-edit; `yarn contracts:regen` |
| Field-effect review index | `crates/runtime/src/contracts/field-effect-coverage.json` + `crates/runtime/tests/contract_field_effects.rs` | Responsibilities and test routes; the index is not execution coverage |
| Prepare / dispatch | `crates/runtime/src/contracts/{prepare,validate}.rs`, `crates/runtime/src/runtime/{dispatch,domain_dispatch,engine}.rs` | Exact schema admission, defaults, private-state rejection, routing |
| Tool implementation | `crates/runtime/src/tools/<snake_tool>/` (`clasify` evaluates through `crates/runtime/src/tools/judgment/` and captures sources through `crates/runtime/src/tools/source_capture/`) | Business logic |
| Providers / API | `crates/runtime/src/providers/{github,artifact,classification}/`, `crates/runtime/src/runtime/github.rs` | Request count, auth, errors |
| Engine primitives | `packages/octocode-native/crates/engine/` | text search, AST, LSP, minify, secrets |
| Caching | `crates/runtime/src/runtime/github_cache.rs`, `crates/runtime/src/cache/`, `crates/runtime/src/tools/gh_clone_repo/cache.rs`, `crates/runtime/tests/tool_cache_contracts.rs` | Keys, TTL, invalidation |
| Output shaping | `crates/runtime/src/response/{pager,row_pages,rows,stage,continuations,pages,result}.rs`; `crates/runtime/src/runtime/engine/channel/` | Complete evidence pages; typed `next.*` and `related[]`; prose `hints[]` |
| Cursor storage and replay | `crates/runtime/src/response/cursor.rs`, `crates/runtime/src/runtime/engine/{cursors,page_replay}.rs` | Saved normalized request and progress, response pages, authority bindings |
| Security | `crates/runtime/src/security/{content,walk,scan}.rs`, `crates/runtime/src/policy/` | Redaction, path sandbox |
| MCP registration | `packages/octocode-mcp/src/index.ts` launches the native server in `crates/cli/src/cli/mcp_server/` | Launcher delegation; native protocol, listing and execution |
| CLI | `packages/octocode/src/cli/{index,native-delegate}.ts` delegates unchanged arguments; native CLI `crates/cli/src/cli/` owns parsing and rendering, with schema in `crates/cli/src/cli/schema.rs` | Delegation, rendering, flags |
| Config | `packages/octocode-config/config-contract.json` → `packages/octocode-config/scripts/generate-config-contract.ts` → `packages/octocode-config/src/config/contract.generated.ts`, `<repo>/docs/generated/CONFIG_SETTINGS.md`; native struct from `crates/runtime/build.rs`; runtime `crates/runtime/src/config/` | One declared knob, one resolver |
| Docs | `<repo>/docs/OCTOCODE_TOOLS.md`, `<repo>/docs/TOOL_DATA_CONTRACT.md`, `skills-dev/octocode-dev/docs/TOOL_QUALITY.md`, `<repo>/docs/CONFIGURATION.md`, `<repo>/docs/OCTOCODE_MCP.md`, `<repo>/docs/OCTOCODE_CLI.md` | Match live behavior |

## Tests to read for a tool

| Scope | Where |
|---|---|
| Core contract | `CORE/src/__tests__/*` (e.g. `publishedSchemaAlignment`, `toolContracts`, `routingContracts`, `publishedSurfaceBudget`) |
| Native integration | `crates/runtime/tests/runtime_{local,github,clasify,batch_response}.rs`; native CLI `crates/cli/tests/{cli_tool_contract,cli_catalog}.rs` |
| CLI/MCP execution and replay | `packages/octocode-benchmark/local/e2e/harness/{tool-health,transport}.mjs` (registered by MCP `test:acceptance`); classification capability and recovery in `packages/octocode-mcp/tests/integration/` |

## Live views

```bash
$OCTO schema                          # enabled tools and purposes
$OCTO schema <tool> --view query       # complete query-row schema
$OCTO schema <tool> --view variants    # required fields and runnable examples
$OCTO schema --view full              # full catalog, instructions and grammars
```

`skills-dev/octocode-dev/scripts/tool-inventory.mjs` reads the generated embed, so it reflects the last regen — if core changed since, regen first or its output is stale.
