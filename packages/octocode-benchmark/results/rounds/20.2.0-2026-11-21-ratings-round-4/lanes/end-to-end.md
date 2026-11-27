# Round 4 — end-to-end + MCP surface lane

Binary: `packages/octocode-native/npm/darwin-arm64/octocode` 20.2.0+56f67471. Guidance used: `.octocode/tmp/rate4/served-instructions.md` (byte-identical to the instructions this lane's own `octocode mcp` initialize returned, `cmp`), each tool's `--help`, `schema`. Machine load average 31–40 during the run (another process): timings are indicative.
Raw: `log.jsonl` + `out-NNN.txt` (every CLI call), `mcp-run.txt`, `mcp-calls.json`, `mcp-tools-list.json`, `config-list.json`.

## Workflows

| Question | Calls (+help/schema) | Chars | Wall | Answer right? | Score | Route |
|---|---|---|---|---|---|---|
| Local: which tools `octocode mcp` serves by default, which are opt-in, where decided | 11 tool + 2 `config` (+1 help, +1 schema) | 22.9K | 117 s | yes | 8 | localSearch files → localSearch content → localFetch ×2 → lspSearch (empty) → localSearch → contract keyPath → `config list` |
| Public: first axios release containing the fetch adapter (PR #6371) | 7 (+2 help, +1 schema) | 5.2K | 78 s | yes | 9 | ghSearchHistory PR → ghGetHistoryItem PR → ghStructure refs glob → ghGetHistoryItem compare ×2 |
| Round 3's: serde_derive struct `Deserialize` visitor and the release that last changed it | 8 | 7.7K | 72 s | yes (1.0.227, unchanged through 1.0.229) | 9 | artifactSearch → ghStructure tree → ghGetFileContent symbols → ghSearchHistory path → refs glob → compare ×2 → ghGetFileContent matchString |
| Guidance | – | – | – | routing right in all 3 | 9 (8) | R3-F1 and R3-F2 fixed |

### Answers and checks (all claims verified, 11 of 11)

1. **Local.** `runtime-base/src/execution/availability.rs`: `resolve` (182–213) gates MCP by `tools.disabled`/`tools.enabled`, then `is_beta()` → `local.beta` (202), then `prerequisites` (151–159): `local.enabled` for local tools, `mcp_opt_in` (140–146: ghCloneRepo → `tools.enableClone`, securityCheck → `tools.enableSecurityCheck`), `required_setting` (130–135: clasify → `classification.api`). Beta tools are the contract's `beta: true`: graphBuild, graphQuery. Defaults (`config list --json`, empty home): local.enabled true, local.beta false, enableClone false, enableSecurityCheck false, classification.api unset → default MCP serves 12 of 17 tools. Verified: `rg` on availability.rs, `jq '.tools[]|select(.beta)'` on tool-contract.json (17 tools, 2 beta), and the live tools/list below (12).
2. **axios.** PR #6371 "feat(adapter): add fetch adapter" merged 2024-04-28T19:33:50Z, merge commit a3ff99b5. v1.6.8 does not contain it (compare: head behind by 2, ahead 0); **first release v1.7.0-beta.0** (8e4314b, ahead 1 behind 0); **first stable v1.7.0** (2024-05-19). Verified: `gh api .../compare/a3ff99b…{v1.6.8,v1.7.0-beta.0,v1.7.0}` = behind / ahead / ahead; npm registry times; `lib/adapters/fetch.js` 404 at v1.6.8, present at v1.7.0-beta.0.
3. **serde.** The struct visitor is emitted by `serde_derive/src/de/struct_.rs` `deserialize` (17–197), `impl … Visitor<#delife> for __Visitor` at 173 (in-place twin at 478), at v1.0.229 (7fc3b4c3). Last commit on the file: 8047570 (2025-09-25). First release with it: **v1.0.227** (v1.0.226 behind, v1.0.227 ahead). v1.0.227→v1.0.229 changes only enum_adjacently.rs/enum_externally.rs under `de/`. Verified: `gh api commits?path=…` newest = 8047570; `gh api compare` statuses; compare 227…229 file list has no struct_.rs; line 173 via contents API.

### Wrong turns and leads

| # | What | Cost | Lead quality |
|---|---|---|---|
| W1 | lspSearch definition of `is_beta`/`is_local` (generated `ToolId` impl) → empty, `partialReasons: buildScriptsOff`; the `related` lead was a localFetch of the *same* file, which only uses the symbols | 1 call | misleading: a workspace `localSearch` for `fn is_beta` found both at once |
| W2 | `contextLines: {after: 12}` refused: `before` is required too; the error's Example drops `contextLines` instead of showing `{before, after}` | 1 call | weak repair hint |
| W3 | `filters.createdAt: "a..b"` refused (`{from,to}`), then `"2023-06-01"` refused (RFC 3339 date-time); each error's Example again drops the filter | 2 calls | the second message is exact; the Example is no help |
| W4 | PR keyword search "fetch adapter" returned 5 newest Nov-2026 PRs of 79 (no relevance order); needed a remembered createdAt window | 1 call | instructions route "first release → its commit or PR" but give no way to find an old PR |
| W5 | `config --json` lists files, not values; `config list --json` has them | 1 call | — |
| H1 | `filters.state: "merged"` repaired with a warning naming `isMerged` | 0 | helpful |
| H2 | compare warning "head is behind base by 2: nothing ahead; related compares the other way" | 0 | helpful; containment (`behindBy 0`) is still the rater's inference — the instructions do not say how to read compare for containment |
| H3 | `ghStructure refs` with a glob ref (`v1.7.*`) listed exactly the candidate tags with SHAs | 0 | helpful (R3-F1 fixed: instructions now say `refs`) |
| H4 | keyPath on localSearch named `tools[0].name` for the `beta: true` hits | 0 | helpful |

Observations: cold `ghStructure tree` of serde_derive/src (depth 2) 5.7 s, the PR read 7.5 s (machine at load ~31, one observation each; R3-F4 still visible). All other calls ≤ 4.3 s.

## Default MCP surface (empty `OCTOCODE_HOME` = `.octocode/tmp/rate4-e2e/mcphome`)

`octocode mcp` over stdio (PID 95747, closed via stdin EOF, exit 0, empty stderr): initialize 0.05 s, serverInfo octocode-mcp 20.2.0+56f67471, instructions 3,233 chars identical to served-instructions.md. tools/list: **12 tools**, 51.3K chars, no nextCursor: ghSearchRepo, ghSearchCode, ghStructure, ghGetFileContent, ghSearchHistory, ghGetHistoryItem, artifactSearch, localSearch, structureSearch, astSearch, localFetch, lspSearch.

One real tools/call each: all 12 returned data, isError false (0.00–3.05 s; artifactSearch slowest). GitHub tools worked on the empty home (credential presumably from the `gh` fallback). The 5 unserved tools, called anyway: `isError: true`, `errorCode: toolUnavailable`, each naming its setting and variable (clasify → OCTOCODE_CLASSIFICATION_API, securityCheck → OCTOCODE_SECURITY_CHECKS, ghCloneRepo → OCTOCODE_ENABLE_CLONE, graphBuild/graphQuery → OCTOCODE_BETA).

Named but not served: the instructions name none. Tool descriptions name ghCloneRepo (ghSearchCode, lspSearch: "if listed") and securityCheck (artifactSearch: "when securityCheck is enabled … it is then in the tool list") — conditional, so not misleading. artifactSearch over MCP emitted no `related` securityCheck lead (the CLI emits one), so leads are gated. No tool is named unconditionally while unserved.

## Guidance rating: 9 (round 3: 8)

+ Routing right first time in all 3 questions; the new "first release with a change → its commit or PR, ghStructure refs, ghGetHistoryItem compare" line was exactly the route used (R3-F2 fixed); `refs` replaces the non-existent `tags` (R3-F1 fixed).
− Validation errors' `Example` strips the offending field rather than showing its valid shape (W2, W3): 3 extra round trips.
− lspSearch's empty-definition lead points at the same file (W1).
− No hint for finding an *old* PR/commit by keywords (W4) and no line on reading compare as containment (H2).
