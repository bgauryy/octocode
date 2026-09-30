# Octocode harness and MCP usage audit (orangu v0.7.2)

Date: 2026-09-30. Tool: `node /Users/bgaryy/code/orangu/dist/orangu.js` (harness, repo, learn, checks, analyze, inspect, query). The audit was read-only; raw outputs are in `raw/`.

Population: 91 Claude Code sessions for this repo (2026-09-14 → 09-28). 33 of them called octocode-local: 1,692 calls, 227 errors (13.4%). Weekly calls went 1,422 → 220 → 30. Most of the error volume comes from four deliberate fuzz and audit sessions on 09-18 and 09-19 (`ffe7c050`, `27f72a37`, `73bb21bd`, `bd81680e`).

## Top fixes (ranked)
1. **The MCP server did not start.** The contract fingerprint didn't match (core 872ab3bb vs native 2a388c88) while round-3 core edits were in flight. Fix: regen plus rebuild. The override hint names `OCTOCODE_ALLOW_CONTRACT_DRIFT=1`, but the bundled dist also needs `NODE_ENV=development` (`packages/octocode-mcp/src/native/index.ts:367`); the message should say so.
2. **The MCP instructions are truncated by the host at about 2 KB.** The server serves 3,926 chars, but Claude Code records 2,216 bytes per injection and appends `… [truncated]`. About 48% is lost: the local tool map, batching, lean-context, stop rules and the grammar list. Keep instructions ≤ 2,000 chars, move the grammar list into the astSearch and structureSearch descriptions, and add a length cap test.
3. **clasify with ghSearchCode as a resource failed** with `classificationContextContractViolation` in session `80b9be40` (09-28), on all 4 resources and then the 1-resource retry. Re-check on the current build; a fix landed on 09-29.
4. **`structureSearch.operation` needs a default.** Its only allowed value is `tree`, yet it is required, so omitting it fails with `queries.0.operation: Value undefined is outside the allowed enum; allowed: tree`.
5. **The permission allowlist is incomplete** (`.claude/settings.local.json`). It allows only 4 of the 13 read-only tools and keeps stale entries (`jev`, `ghSearch`, `mcp__octocode__ghSearchRepos`). This caused 60 permission failures, and the mcp-rejections check fired in 21 of 90 sessions. **The owner decides this**, because it is a permission setting.
6. **Responses over the host output cap.** On 09-21, `ghGetFileContent fullContent:true` returned 74–82k chars and localSearch 61.9k, and the host rejected them. Keep the first page at about 40k with a continuation, and re-check on the current build.
7. **ghSearchHistory failed 39.7% of the time** (25 of 63): 10 own-output schema failures and 8 input errors. All were on 09-18, so re-check. Add one valid example per operation.
8. **lspSearch reliability.** Since 09-21, 4 of its 9 calls failed (`lsp.timeout`, "LSP connection closed"), and the slowest call took about 60 s.
9. **Schema and payload weight.** The 13 tool definitions total 77.6 KB (about 19k tokens). The largest are ghGetHistoryItem 10.4 KB, astSearch 9.1, lspSearch 8.8, clasify 8.6 and ghSearchHistory 8.1. ghGetHistoryItem responses average 9.6 KB (p95 29 KB).
10. **Skills and memory cleanup:**
    - Two skills were listed in 86 sessions and invoked 0 times: `octocode-orchestrator-local-worker` and `octocode-subagent`.
    - 11 skills are installed twice (in `~/.claude/skills` and in the repo's `.claude/skills`).
    - There are 3 dead links: `octocode-awareness` ×2 and `octocode-code-graph`.
    - `octocode-dev` and `rust-best-practices` are invisible to Claude Code; only Codex uses them.
    - MEMORY.md is 11.6 KB and injected every session.
    - AGENTS.md (14.7 KB) is not auto-loaded.

## Octocode tool usage (Claude Code, all history)
| tool | calls | errors | avg / p95 / max result bytes | since 09-21 (calls / errors) |
|---|---|---|---|---|
| ghGetFileContent | 700 | 67 (9.6%) | 4,812 / 13,827 / 46,362 | 74 / 5 |
| ghSearch (retired) | 367 | 51 (13.9%) | 1,780 / 6,955 / 24,783 | 55 / 6 |
| ghGetHistoryItem | 175 | 21 (12%) | 9,643 / 29,310 / 45,813 | 3 / 1 |
| localSearch | 127 | 15 (11.8%) | 3,531 / 13,301 / 30,865 | 38 / 3 |
| localFetch | 82 | 8 (9.8%) | 5,790 / 15,926 / 31,033 | 26 / 0 |
| ghSearchHistory | 64 | 25 (39%) | 1,621 / 7,974 / 14,997 | 4 / 0 |
| astSearch | 60 | 21 (35%) | 3,831 / 11,425 / 40,709 | 9 / 1 |
| lspSearch | 28 | 7 (25%) | 3,854 / 10,700 / 31,931 | 9 / 4 |
| artifactSearch | 25 | 3 | 1,985 / 8,287 / 14,519 | 5 / 0 |
| clasify | 18 | 3 | 3,974 / 37,535 / 37,535 | 17 / 3 |
| ghSearchRepo / ghSearchCode / structureSearch / ghStructure | 5 / 3 / 2 / 1 | 0 / 0 / 1 / 0 | small | all recent |

- **Coverage:** every current tool was called at least once. There were no identical-call loops.
- **Schema learning by failure:** retries with different fields — ghGetFileContent 13, ghSearchHistory 13, ghSearch 12, astSearch 8. Most of these are from the fuzz sessions.
- **Product-side input errors since 09-21:** the structureSearch operation (see fix 4); localSearch `regex: true`, where agents assume a boolean but the field is a mode enum; and astSearch given a bare object instead of an array.

## Harness issues
| # | Issue | Evidence | Class | Fix |
|---|---|---|---|---|
| H1 | MCP instructions truncated at about 2 KB | 3,926 served vs 2,216 bytes recorded | Product | #2 |
| H2 | MCP startup drift | fingerprint mismatch | Product/build | #1 |
| H3 | Allowlist is 4 of 13 tools, with 3 stale entries | 60 permission failures | Harness (owner) | #5 |
| H4 | Skills that never fire | 86 listings, 0 invocations | Harness | #10 |
| H5 | 11 skills installed twice | user and project links to the same targets | Harness | #10 |
| H6 | 3 dead skill links | `skill-link-dangling` ×3 | Harness | #10 |
| H7 | Repo skills invisible to Claude | Codex reads `octocode-dev` 14×, `rust-best-practices` 17× | Harness | #10 |
| H8 | AGENTS.md not auto-loaded | 14.7 KB, linked from a 152-byte CLAUDE.md | Harness | #10 |
| H9 | MEMORY.md large | 11.6 KB every session | Harness | #10 |
| H10 | Disabled Docs connector still announced | 8 sessions | Harness | account setting |
| H11 | Codex hooks point at missing scripts | `~/.codex/hooks.json` | Harness (Codex) | fix paths or remove the handlers |
| H12 | Stale Codex trust paths | 47 of 69 are temp or deleted directories | Harness (Codex) | prune |
| H13 | Unused plugin skill packs | 40 KiB skill listing per session | Harness / host | disable unused packs |
| H14 | Heavy tool schemas | 77.6 KB | Product | #9 |

## Other patterns (learn)
- **Unreliable tools:** ghSearchHistory 39.7% failures and astSearch 35.6%.
- **ghGetFileContent failures:** redirect-not-followed or not-found 19×, and "failed to execute; see the server logs" once.
- **Not octocode, but worth checking:** a private key appeared in the transcript in 7 sessions; `.env` files were read in 9; files were re-read in 37.

## orangu limitations hit
- "Sessions for a component" means announced, not used.
- Error text is hidden unless you pass `--include-text`.
- There is no date filter.
- Renamed tools are not linked.
- It does not detect truncated MCP instructions.
- It has no per-tool schema weight.
- It does not follow AGENTS.md imports.
- Octocode CLI outcomes are mostly invisible.
- Skill reads always show 0.
- Double-installed skills are flagged only for Codex.
- Very large outputs: `learn` returns about 809 KB.

## Method notes
- `--include-text` was used only on failing octocode rows.
- The one step outside orangu was a read-only `initialize` plus `tools/list` against `packages/octocode-mcp/dist/index.js`, to measure the instructions and schemas (with `NODE_ENV=development OCTOCODE_ALLOW_CONTRACT_DRIFT=1` on that one process).
