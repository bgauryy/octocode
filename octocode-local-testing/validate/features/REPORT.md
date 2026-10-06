# Octocode feature inventory and claims audit

Date: 2026-09-30. Every claim below was probed live, on the CLI and MCP. The full inventory tables are in `lanes/github.md`, `lanes/local.md` and `lanes/platform.md`. Raw probes are in `probes/`, and the MCP init capture is in `init.json`.

## Counts
| Lane | Rows | Verified | Partial | Fails | Untestable |
|---|--:|--:|--:|--:|--:|
| GitHub + artifact (including MCP parity) | 65 | 56 | 7 | 1 | 1 |
| Local tools | 90 | 70 | 14 | 6 | 0 |
| Platform (clasify, config, beta, skills, install, auth, errors, parity) | 58 | 39 | 14 | 5 | 0 |
| Spot probes C1–C4 | 4 | 2 | 0 | 2 | 0 |
| **Total** | **217** | **167** | **35** | **14** | **1** |

Spot probes:
- **C1 (FAILS):** README.md:456 says CUDA is supported, but there is no `.cu` grammar. The inventory lists 28 extensions, not 30.
- **C2 (FAILS, doc):** README.md:224 and :462 say `minify:"standard"` is the default. The effective default for localFetch and path-only ghGetFileContent is `none`.
- **C3 (VERIFIED):** MCP registers 12 tools, or 13 when a classification key is present.
- **C4 (VERIFIED):** output is minimal unless `debug:true`.

## Claims that hold up (the "better than raw tools" set)
- **Pinned SHAs:** `commitSha` is returned, continuations are pinned to it, and PRs carry `sourceSha` and `mergeCommitSha`.
- **Honest pagination:** `hasMore`, `countScope:"unknown"`, snapshots with a stale restart, `responsePagination`, and 16 KiB page budgets.
- **Minimal output:** 25–60% fewer bytes than debug output, and open pagination and `next` are never dropped.
- **Secret redaction:** AWS keys, `ghp_` tokens and PEM blocks, including mid-block windows and byte pages. `matchString` never matches a secret, GitHub content is covered, and emails can be redacted as an opt-in.
- **Sandbox:** paths outside the root, symlink escapes and `../` are denied, and sensitive files are denied even inside allowed roots.
- **Continuations:** each carries `followUp:true` and replays verbatim on CLI and MCP.
- **Batching:** 1–5 rows with a brief per row, bad-row isolation, and order preserved.
- **clasify:** locate hits the right window at 0.97–0.99, bodies don't leak, a matrix is capped at 25 cells, and it drops cleanly when no key is set.
- **Config precedence:** 5 layers verified. Beta gating verified. CLI stdout is byte-identical to MCP `structuredContent`.

## Top defects
1. **High: `WORKSPACE_ROOT` does not resolve relative paths.** They resolve against cwd instead (`runtime/src/policy/path.rs:416-425`). Repro: `cd /tmp; WORKSPACE_ROOT=$FX octocode localFetch '{"goal":"g","reasoning":"r","path":"src/a.ts","startLine":1,"endLine":1}'` gives `pathOutsideAllowedRoots`.
2. **Medium: home is not an allowed root,** although OCTOCODE_TOOLS.md:563, SECURITY.md:53 and README.md:444 say it is. The code is deliberate (`engine.rs:375`), so the docs are wrong.
3. **Medium: most of the documented GitHub response cache doesn't exist.** Only `ghGetFileContent` caches (`runtime/github.rs:519`).
4. **Medium: `invertMatch` with `resultView:"files"` lists files that DO contain the pattern.** `filesWithout` is correct.
5. **Medium: a hand-written `followUp:true` skips the brief rule** (`contracts/validate.rs:239-241`).
6. **Medium: minimal output hides signals agents are told to check.** `data.lsp.source` and the Rust `visibilityExact` hint appear only with debug.
7. **Medium: `auth status` reports an invalid token as `authenticated:true`,** while tool calls with that token fail with 401.
8. **Medium: `skill remove` with an isolated HOME/OCTOCODE_HOME targets the cwd repo's `.agents/skills`.**
9. **Medium: the documented structural-pattern auto-retry (`structural.query.rewritten`) doesn't exist.**
10. **Low: SHA pinning is incomplete.** Compare leaves `head` as a tag, and ghStructure has no SHA and its pages aren't pinned.
11. **Low: gate and config errors use a third output shape.** They return a bare `{error,errorCode}` with exit 5, and `--json-errors` doesn't change that.
12. **Low: redaction gaps.** An unlabeled 40-character AWS secret isn't redacted. A secret-shaped `searchText` returns a silent "No matches".
13. **Low: PR filter and match gaps.** `fileFilter` counts still cover the whole PR, and `matchString` without `content.patches` is dropped silently.
14. **Low: smaller issues:**
    - `hover.range` is 0-based while everything else is 1-based.
    - "14 lines" is reported for a 13-line file.
    - The structureSearch `files` count includes the root.
    - The graph `stale` hint drops `--workspace`.
    - Scout `next.read` ranges run past end of file.
    - `skill remove ../../etc` is refused with an empty reason.
    - MCP missing-goal errors are raw Zod text.
    - `schema` suppresses config warnings.
15. **Harness:** `harness/mcp-client.mjs` hangs for 240 s when the server dies during init, because it has no child-exit handler.

## Docs ↔ behavior drift
**Recent changes:**
- **Minimal responses:** TOOL_DATA_CONTRACT documents them, but OCTOCODE_TOOLS still promises fields that are now debug-only.
- **Compact PR rows, `fileFilter`, `matchContext`:** described only in the schema.
- **`followUp`:** documented, but a hand-written value can bypass the brief rule.
- **`defaultExcludes`:** the docs overstate what `false` does.
- **ghSearch split:** clean.

**GitHub:**
- **G-1.** TOOL_DATA_CONTRACT.md:99 says the path-only default minify is `standard`; it is `none`.
- **G-2.** OCTOCODE_TOOLS.md:1536-1538 describe a shared cache with a `cache:1` marker; only `ghGetFileContent` caches.
- **G-3.** OCTOCODE_TOOLS.md:405-406 say ghGetHistoryItem caches; it doesn't.
- **G-4.** OCTOCODE_TOOLS.md:59 says batched rows share one goal; each row needs its own.
- **G-5.** OCTOCODE_TOOLS.md:366-373 describe object rows; they are now compact strings.
- **G-6.** `fileFilter` and `matchContext` are undocumented, and the OCTOCODE_TOOLS.md:529 cost table shows the old PR flow.
- **G-7.** OCTOCODE_TOOLS.md:513 promises an unsupported-capability error for PyPI keywords; the result is a generic `invalidInput`.
- **G-8.** The OCTOCODE_TOOLS.md:1506-1510 clone example has the wrong `complete` value and path format.
- **G-9.** OCTOCODE_MCP.md:68 lists ghCloneRepo without saying it is CLI-only.
- **G-10.** `--redact-emails` is undocumented.
- **G-11.** TOOL_DATA_CONTRACT.md:80 says request echoes are debug-only; minimal rows still echo owner/repo/path/type/ref.

**Local:**
- **L-D1.** OCTOCODE_TOOLS.md:563, SECURITY.md:53 and README.md:444 say home is allowed by default. It isn't.
- **L-D2.** OCTOCODE_TOOLS.md:562 and README.md:373/:444 say `WORKSPACE_ROOT` resolves relative paths. It doesn't; cwd does.
- **L-D3.** OCTOCODE_TOOLS.md:732-737 describe a structural retry. It isn't implemented.
- **L-D4.** OCTOCODE_TOOLS.md:1326-1331 say documentSymbols prefers the native path. The language server wins whenever one exists.
- **L-D5.** OCTOCODE_TOOLS.md:1318-1322 say `WORKSPACE_ROOT` is the LSP root. The nearest project marker wins.
- **L-D6.** OCTOCODE_TOOLS.md:662 describes `invertMatch` with the files view. The behavior is wrong (defect 4).
- **L-D7.** OCTOCODE_TOOLS.md:880 says the default chunk is 100 lines. It fills the 16 KiB budget instead (178–234 lines observed).
- **L-D8.** OCTOCODE_TOOLS.md:887/:889 say `sourceBytes` and `outOfRange` come on every read. They are debug-only.
- **L-D9.** OCTOCODE_TOOLS.md:1227 says every coordinate is 1-based. `hover.range` is 0-based.
- **L-D10.** TOOL_DATA_CONTRACT.md:101, LOCAL_RESEARCH_WORKFLOW.md:30 and OCTOCODE_TOOLS.md:1185-1200 tell agents to check `data.lsp`. It is absent in minimal output.
- **L-D11.** OCTOCODE_TOOLS.md:767 `visibilityExact` hint. Debug-only.
- **L-D12.** OCTOCODE_TOOLS.md:684 `capped`/`binaryQuit`. Debug-only.
- **L-D13.** OCTOCODE_TOOLS.md:671/:808 say `defaultExcludes:false` walks credential directories. `secrets/` is never walked, and `.gitignore` still applies without `noIgnore`.
- **L-D14.** OCTOCODE_TOOLS.md:565 omits the `structure.policy.outsideAllowedRoots` code.
- **L-D15.** SECURITY.md:60 says denied-path errors are relative. They are absolute.
- **L-D16.** README.md:377 and CONFIGURATION.md say `OCTOCODE_BETA` is the sole gate for astRewrite. It also gates astTopology, and the astTopology docs don't mention the gate.
- **L-D17.** The `recoveredImporter` source is undocumented.
- **L-D18.** OCTOCODE_TOOLS.md:576 says localFetch is pure Node.js. It is native.
- **L-D19.** Minimal `workspaceSymbol` rows still echo `type`.
- **L-D20.** LOCAL_RESEARCH_WORKFLOW.md:22 recommends `langType` for directories. Directory `symbols` rejects it.
- **L-D21.** SECURITY.md doesn't say that system directories are denied even when added to `ALLOWED_PATHS`.

**Platform:**
- **P-1.** OCTOCODE_CLASIFY.md:185 says "no judgment cache". A process-local cache exists (`tools/clasify/cache.rs`).
- **P-2.** OCTOCODE_CLASIFY.md says the handoff carries the "five top-ranked files". It carries 3, plus an undocumented `prefilter`.
- **P-3.** OCTOCODE_CLASIFY.md:140 says MCP text is empty. It is about 2 KB of JSON.
- **P-4.** The OCTOCODE_CLASIFY.md:245-265 and :290-318 examples are rejected with exit 2.
- **P-5.** OCTOCODE_CLASIFY.md:134 omits the `sufficient` preset.
- **P-6.** README.md:328-329 say reasoning is optional. It is required.
- **P-7.** README.md:476 says "15 skills". There are 16.
- **P-8.** The skills/README.md:14 link is broken; the skill is in `skills-beta/`.
- **P-9.** CONFIGURATION.md:189 says every key is accepted from either `.env`. Home-only keys are skipped from the workspace `.env`.
- **P-10.** CONFIGURATION.md:360 says tokens are blocked in `.env`. They are accepted.
- **P-11.** CONFIGURATION.md:211/:236 promise config warnings. `schema` prints none.
- **P-12.** The CONFIGURATION.md:232 and :253 bullets are duplicates.
- **P-13.** The CONFIGURATION.md:277 `--ide` aliases aren't listed (claude → claude-desktop, vscode → vscode-cline).

## How the Octocode flow works (with file references)
1. **Contract.** Tools are authored in core Zod. Config generates `contract/tool-contract.json`, `tool_types.rs` and `toolTypes.generated.ts`. Native embeds them (`runtime/src/contracts/generated.rs`).
2. **MCP startup** (`packages/octocode-mcp/src/native/index.ts:308-458`): check the ABI and catalog, filter out CLI-only tools (ghCloneRepo, astRewrite), fail closed on a fingerprint mismatch, build the instructions scoped to the available tools, register the tools with row-isolating schemas, then call `runtime.executeMcp` through napi.
3. **CLI** (`packages/octocode/src/cli/native-delegate.ts` → native `crates/cli/src/cli/mod.rs:666` `run_tool`): only scheme and skill stay in Node. Exit codes:
   - invalid JSON → 2;
   - a failure kind → 3, 4, 5 or 7;
   - a rejected row → 2;
   - all rows empty → 1;
   - a continuation or `hasMore` → 6;
   - otherwise 0.
4. **Gate and validation** (`runtime/src/runtime/engine.rs:659-800`): CLI-only tools are refused on MCP. Availability is checked (beta, clasify key). An HMAC cursor resumes a previous query. `contracts::prepare_many_and_validate` (`contracts/mod.rs:156`) enforces 1–5 rows, unknown-field rejection and no clamping. The brief rule is `missing_brief` (`contracts/validate.rs:239`). Bad rows are isolated. The content security gate is `security/content.rs:308`.
5. **Dispatch** (`runtime/domain_dispatch.rs:34`) routes each tool:
   - GitHub → `runtime/github.rs`, with the file cache in `github_cache.rs`;
   - artifactSearch → its own route;
   - LSP → the LSP pool;
   - local tools → the path policy plus the engine: ripgrep, tree-sitter, minify;
   - clasify → `clasify_batch`, then `clasify_locate`, `clasify_context` and `clasify_output`.
6. **Row shaping** (`engine.rs:840-900`, `runtime/response.rs`):
   - `result_row`, diagnostics, the cache flag and the hint policy;
   - `minimize_row`, which is skipped under debug and falls back to the cheapest contract variant;
   - the `base` field with relative paths;
   - `finalize_output_fields` for redaction;
   - the `next.clasify` handoff;
   - filtering of unavailable cross-tool next steps.
7. **Response stage** (`runtime/response_stage.rs:34`):
   - `mark_follow_ups` swaps the goal/reasoning for `followUp:true`;
   - output-contract isolation;
   - continuation compaction;
   - `render_tool` (YAML or JSON, `render.rs:79`);
   - auto-pagination and `ResponsePager`;
   - a final `validate_output`.
   - clasify uses `finish_receipts` instead.
8. **Output:** MCP returns `{content, structuredContent, isError}`, and the CLI prints JSON plus an exit code.
9. **Config** (`runtime/src/config/{loader,resolver,dotenv}.rs`): precedence is env > workspace `.env` > global `.env` > workspace rc > global rc. Home-only and protected keys are ignored from workspace files. A misconfiguration never blocks a call.
