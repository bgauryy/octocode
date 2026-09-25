# Surface map

Load when locating any layer of a tool before auditing or editing it. Why: a fix in the wrong layer drifts from the source of truth.

Paths are relative to the monorepo root; `CORE` = `../octocode-mcp-host/packages/octocode-core`.

## Layers per tool

| Layer | Owner path | Audit question |
|---|---|---|
| Input schema (authored) | `CORE/src/toolContract/input/resources/tools/<tool>.ts` (+ `_toolkit.ts`, `global.ts`, `toolVariants.ts`) | Fields, defaults, bounds, variants |
| Cross-field validation | `CORE/src/toolContract/validation/<area>.ts`, `CORE/src/toolContract/nativeRules/` | Rules that native replays at prepare time |
| Descriptions | `CORE/src/toolContract/descriptions.ts`, `metadata.ts`, `catalog.ts` | Tool/field prose shown to agents |
| MCP + CLI instructions | `CORE/src/toolContract/instructions.ts` (`buildMcpInstructions`), `cliContext.ts` (`buildCliToolContext`), `CORE/src/systemPrompt.ts` | Shared workflow guidance |
| Output schemas / limits | `CORE/src/toolContract/outputSchemas.ts`, `limits.ts` | Internal validation of produced results |
| Contract hub (re-export only) | `packages/octocode-config/src/contracts/{schema,mcp}.ts` | Must stay a thin `export *` |
| Native embed (generated) | `packages/octocode-native/crates/runtime/src/contracts/generated/{tool-contract.json,contract-provenance.json,contract-fixtures.json}` | Never hand-edit; regen |
| Field-effect claims | `packages/octocode-native/crates/runtime/src/contracts/field-effect-coverage.json` + `tests/contract_field_effects.rs` | Hand-maintained labels: verify against code |
| Prepare / dispatch | `crates/runtime/src/contracts/{prepare,validate}.rs`, `src/runtime/{dispatch,domain_dispatch,engine}.rs` | Defaults, normalization, routing |
| Tool implementation | `crates/runtime/src/tools/<snake_tool>/` (`astTopology` → `ast_graph/`; `localFetch` also `tools/local_fetch.rs`; `clasify` also `runtime/clasify_*.rs`) | Business logic |
| Providers / API | `crates/runtime/src/providers/{github,artifact,classification}/`, `runtime/github.rs` | Request count, auth, errors |
| Engine primitives | `packages/octocode-native/crates/engine/` | ripgrep, AST, LSP, minify, secrets |
| Caching | `runtime/github_cache.rs`, `src/cache/`, `tools/gh_clone_repo/cache.rs`, `tests/tool_cache_contracts.rs` | Keys, TTL, invalidation |
| Output shaping | `src/response/mod.rs`, `runtime/{response,response_stage,render,continuations,cursor}.rs`, `tools/result.rs` | Rows, evidence, `next.*`, compact CLI |
| Security | `src/security/{content,walk,registry}.rs`, `src/policy/` | Redaction, path sandbox |
| MCP registration | `packages/octocode-mcp/src/native/index.ts` (instructions), `src/public.ts` | Thin forward, no logic |
| CLI | `packages/octocode/src/cli/{native-delegate,parser,options}.ts`, `commands/scheme.ts`; native CLI `crates/runtime/src/cli/` | Rendering, flags |
| Config | `packages/octocode-config/config-contract.json` → `packages/octocode-config/scripts/generate-config-contract.ts` → `src/config/contract.generated.ts`, `docs/generated/CONFIG_SETTINGS.md`; native struct from `crates/runtime/build.rs`; runtime `src/config/` | One declared knob, one resolver |
| Docs | `docs/OCTOCODE_TOOLS.md`, `docs/TOOL_DATA_CONTRACT.md`, `docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`, `docs/CONFIGURATION.md`, `docs/OCTOCODE_MCP.md`, `packages/octocode/docs/OCTOCODE_CLI.md` | Match live behavior |

## Tests to read for a tool

| Scope | Where |
|---|---|
| Core contract | `CORE/src/__tests__/*` (e.g. `localDirectSchemas`, `mcpInstructions`, `routingContracts`, `agentContextBudgets`) |
| Native integration | `crates/runtime/tests/runtime_{local,github,clasify,batch_response}.rs`, `cli_scheme.rs` |
| MCP all-tool contracts | `packages/octocode-mcp/tests/tools/all-tools.pagination-contract.test.ts` |

## Live views

```bash
OCTO='node packages/octocode/out/octocode.js'
$OCTO scheme --compact                 # catalog + availability
$OCTO scheme <tool> --view query       # full public query schema
$OCTO scheme <tool> --compact          # what agents see compactly
```

`scripts/tool-inventory.mjs` reads the generated embed, so it reflects the last regen — if core changed since, regen first or its output is stale.
