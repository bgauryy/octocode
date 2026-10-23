# TODOs (2026-10-23 session hand-off)

Open work left after the 2026-10-23 alignment session. Evidence files are in
`.octocode/octocode-architect/audit-2026-10-23/` (gitignored). `path:line` references point at the tree as of
session end; re-check with `localSearch`/`lspSearch` before you edit, because file splits moved code all day.
Nothing is committed.

Legend: **P0** broken or wrong output · **P1** correctness drift or a duplicated rule · **P2** architecture or efficiency · **P3** hygiene.

## 0. Verify first (agents were mid-task at session end)

| # | Item | Why | Where / how |
|---|---|---|---|
| 0.3 | Re-run gates once Pi's in-flight benchmark edits land | Verified 21:40: native workspace tests all green (lib 2976/0, integration 342/0, engine 993/0, cli 144/0 + 142 + 95 + 56), clippy `--workspace --all-targets -D warnings` clean, contract regenerated (core descriptions changed), `$DEV verify` green except the file-length gate on 2 benchmark harness files a Pi session was editing at 21:48 (`local-ranking.mjs` 757→759, `read-tool-selection.mjs` 952→964) | `$DEV verify` (core `yarn vitest run` 944/0 at 21:55) |
| 0.4 | Release build + `/mcp` reconnect | `build:dev` is current; the release build (`$DEV build`) was not run because the tree was mid-edit | `$DEV build`, then `/mcp` reconnect |

## 1. Contract / output (finish what started)

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 1.3 | P2 | P4: page blocks for the gh history tools through `PageFacts`/`SpanFacts` builders (`SpanFacts::local`, `PageFacts::with_unit` don't exist yet) | Hand-built blocks bypass the shared page facts (`{"hasMore":more}` with no `currentPage`) | `gh_get_history_item/issue.rs` (~404), `patch/page.rs` (~335), `runtime/src/response/pages*.rs`; `paging.json` P4 |
| 1.5 | P2 | L8 rest: `ToolOutput::error_hint`/`category_hint` take the query (7 implementers) | lspSearch kept a second hint path because the hooks don't see the query | `tools/lsp_search/output.rs`, `response/rows.rs`, `response/hint_budget.rs`, `runtime/error.rs`; `.octocode/native-consolidation/leads-report.md` |
| 1.6 | P3 | Renamed-repo note on the ghGetHistoryItem missing-ref failure path | GH-4 moved recovery into the tool but deliberately left this behavior change out | `tools/gh_get_history_item/recovery.rs` |

## 2. Correctness drift found by audits (`duplication.json`, `efficiency.json`, `flow.json`)

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 2.5 | P2 | F13 rest: confirm `local_search/verify.rs` and `local_search/manifest.rs` reads go through `open_regular` (structureSearch line count/digest/secret guard were moved to it on 2026-10-23) | FIFO/special-file safety; the agent reported the structureSearch sites only | `local_search/verify.rs`, `local_search/manifest.rs` |
| 2.6 | P2 | D5: bounded read, 6 copies → `octocode_io` | One owner for read limits | `private_file::read_limited`, `cli management::read_bounded`, `selection/plan.rs:113`, … (`duplication.json` D5) |
| 2.7 | P2 | D4: 9 hand-written UTF-8 boundary loops → std `floor_char_boundary`/`ceil_char_boundary` | Engine already uses std; the fuzzer found 2 offset bugs in this class today | 5 runtime files (`duplication.json` D4) |
| 2.8 | P3 | D3, D6, D8–D13, D15, D16 small duplicates: line-start table, LSP SymbolKind codes ×3, glob metachar checks ×6, listing rank ×2 + `EntryKind` ×3, dirname ×5, page-count math ×4, receiver `Fields`, extension helpers, `fields_key`, match digest ×4 | Each has an owner already; move to it | `duplication.json` (exact `path:line` per item) |
| 2.9 | P3 | Duplication candidates D17–D24 (BudgetStop mapping ×5, localSearch page-size defaults hardcoded (core), extension aliases, two `ToolError` structs (rename = your call), redirect loops ×3, test page walkers, ghSearchHistory hand-picked sanitizing, lock-file setup) | Unverified: run the named distinguishing check first | `duplication.json` candidates |

## 3. Architecture flow (`flow.json`)

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 3.1 | P2 | Break the shared-module cycle judgment ↔ selection ↔ source_capture | A 7-line pure helper and one type import create the cycle; target DAG: selection→judgment, source_capture→judgment | `judgment/batch.rs:136` (`source_capture::assets::referenced`), `selection.rs:79`, `selection/run.rs:10` |
| 3.2 | P2 | Break response cycles pages↔result and pager↔row_pages | Exist only for one constant (`STALE_SNAPSHOT_ERROR`, `result/error.rs:80`) and one type (`RowSource`, `pager/cut.rs:1`); target one-way result→pages→pager | `response/**` |
| 3.3 | P2 | Replace glob re-exports and `pub(in super::super)` sprawl from the file splits with explicit module APIs | 261 `pub(in super::super)` items (~110 in lsp_search) and ~140 production `use x::*`. The localSearch 16-file cycle (`executor.rs` imports 14 siblings; `types.rs:170` imports the executor) is the worst | `flow.json` F3 |
| 3.4 | P2 | Teach `module_boundaries` about shared-module ordering and `tools/mod.rs` | The guard can't catch 3.1 and doesn't see `tools/mod.rs` helpers (`pruned_walk`, `display_name`, `outlines`) | `runtime/src/tools/module_boundaries*.rs` |
| 3.5 | P3 | One routing/row-shaping place per tool family (candidate) | Spread over `dispatch.rs` arms + post-processing (e.g. localSearch path join `dispatch.rs:81-87`), `github/tools.rs`, inline arms in `domain_dispatch.rs` | `flow.json` F4 (needs a distinguishing check) |
| 3.6 | P3 | ARCHITECTURE.md crate list and edges | Says "Six crates"; misses github/cli/runtime-napi → contract-types edges | `packages/octocode-native/ARCHITECTURE.md` |

## 4. Efficiency (`efficiency.json`)

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 4.1 | P2 | EFF-1: output secret gate scans every key and copies clean text | Measured: 711 detector calls / 706 µs per 16 KB localSearch page, about 40× serialization; keys are 42%. Skip or memoize contract keys, return unchanged text by reference, fixed bitset instead of the 308-entry vec. Expected −0.4–0.5 ms/page. 2026-10-23 attempt reverted unfinished (usage limit): design = build.rs emits declared output `properties` names; gate memoizes the pathless verdict per declared name (pure function of text, so user-data keys stay scanned); engine `redact_leaf` → `Option<String>`; `sanitize_json` closure → `Option<String>`; `[u64; PATTERN_COUNT.div_ceil(64)]` candidate bitset. Not landed, no timings taken | `engine/src/security/detector.rs:164-193`, `runtime/src/security/walk.rs:166`, `seal_output` |
| 4.2 | P2 | EFF-2: file reads scanned twice (whole file, then the page) | 1.6 ms + 0.34 ms per 75 KB read; the doc figure at `security/scan.rs:121` (~55 ms/176 KB) is stale | localFetch read path. Not a pure duplicate: the read scan uses `Scope::File(path)`, the gate `Scope::Leaf` (file-context patterns gate on content), and a window cut can create new `\b` matches, so skipping needs a proof the leaf is the unwindowed File-scope view; also fix the stale `scan.rs` doc figure (~55 ms/176 KB; measured ~1.6 ms/75 KB) |
| 4.3 | P2 | EFF-4: output validation deep-clones every tried union branch | 53 µs per localSearch envelope plus every row `data`; add a read-only validation mode when no defaults apply | `contracts/validate/union.rs:121,214`, `validate/schema.rs:97` |
| 4.4 | P2 | EFF-3: one global Mutex across the parallel walk, held during 3+ fs calls per dir | Contention on many-core walks; do I/O before locking or use per-thread buffers (like engine `WorkerRecs`) | `tools/local_search/walked.rs:201-204` |
| 4.5 | P3 | EFF-6: paged responses serialized/scanned 4–5×; `rows.clone()` | One streaming hash+count pass | `response/pager.rs:269`, `response/row_pages.rs:135,163` |
| 4.6 | P3 | EFF-5/7/8: unused `value` String per matched line; path clone to size; ghSearchCode dedupe clones+serializes rows | Small allocations on hot loops | `sink.rs:240`, `text_parser.rs:279`; `efficiency.json` |
| 4.7 | P3 | Fix misleading comment "files are hashed while searched" | Measured as a second read (within noise), the comment is wrong | `efficiency.json` refuted item |

## 5. Hard rules / tooling

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 5.1 | P1 | Finish the 400-line rule (31 baselined files left): (a) local_search: `layout` 1493, `executor` 1153, `manifest` 570, `enclosing` 561, `packed` 558, `regex_repair` 466, `cursor` 454, `rows` 454, `types` 430, `tests/stored` 1617, `tests/binaries` 419; (b) GitHub area: `runtime/tests/runtime_gh_structure.rs` 1213, `tests/tool_cache_contracts.rs` 421, `providers/artifact/{release_ref.rs 684, npm.rs 670, npm/tests.rs 619, http/tests.rs 586, types.rs 575}`, `gh_search_history/tests/{terms.rs 584, shaping.rs 435}`; (c) benchmark harness: `local/e2e/harness/{read-tool-selection 952, competitors 791, chain-check 772, local-ranking 757, tool-health/mock-server 561, mcp-client 525, tool-health 502, github 498, read-tool-selection/support 481, soak 476, sensors 406}.mjs` | ast_search and structure_search are done (the Pi session finished them); `local_search/tests/empty_pages.rs` split into `empty_pages/{walk_limits,skipped,leads}.rs` 21:40. local_search waits on the 3.3 module-API cleanup (the 16-file cycle); GitHub split plans in `.octocode/octocode-architect/audit-2026-10-23/split-plans-github/` | `node skills-dev/octocode-dev/scripts/check-file-length.mjs` |
| 5.2 | P1 | Regenerate core's oxlint baseline, then both again after 5.1 | Root `.oxlintrc.baseline.json` regenerated 21:40 (shrink-only check: 2048→1720 entries, none new or looser) and `file-length-baseline.json` rewritten; core's (`../octocode-mcp-host/packages/octocode-core/scripts/oxlint-baseline.mjs --write`) not run | as named |
| 5.3 | P2 | Stricter clippy: `too_many_lines` (clippy.toml 200 → 150 → 100), `allow_attributes`, `allow_attributes_without_reason`, `manual_let_else`, `unused_self`, `option_option`, `large_stack_arrays`, `mod_module_files` (migrate ~65 `mod.rs`); move engine's local lint attrs to the workspace table; cargo-deny `multiple-versions = "deny"` | Rust research (14/14 verified): about 70 hits at the 200-line threshold | `packages/octocode-native/Cargo.toml` `[workspace.lints]`, `clippy.toml`, `deny.toml`; research in `.octocode/octocode-brainstorming/tmp/build-lint-2026-10-23/` |
| 5.5 | P2 | chrome-devtools `strict: true` (~800 errors), then drop its oxlint override | The override turns off 2 type-aware rules that need `strictNullChecks` | `packages/octocode-chrome-devtools/tsconfig.json`, `.oxlintrc.json` (chrome-devtools override) |
| 5.6 | P3 | octocode-mcp declaration emit with strict on (drop rollup-plugin-dts if tsgo emits) | `tsconfig.build.json` still emits `.d.ts` with strict off | `packages/octocode-mcp` |
| 5.7 | P3 | Lint `skills/` and `skills-dev/` scripts with oxlint | Outside the old ESLint scope, so still unlinted | root `.oxlintrc.json` `ignorePatterns` |
| 5.8 | P3 | Pi max-len (200 cols) lost with ESLint | Prettier `printWidth` covers it once pi is formatted | `packages/octocode-pi-extension` |
| 5.9 | P3 | `agents-communication/tests/ire.test.ts` empty tracked file | Baselined under `unicorn/no-empty-file`; delete (needs your OK) | — |
| 5.10 | P3 | `$DEV clean:cache` + check `target/` size | target/ hit 421 GB today (25 GB disk left); stale incremental sessions were pruned by hand | `packages/octocode-native/target` |
| 5.11 | P3 | tsdown / oxfmt | Deferred by decision until oxfmt leaves beta and tsdown reaches 1.0 | `TOOLING-PLAN.md` |

## 6. Octocode tool defects found while dogfooding

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 6.1 | P1 | astSearch JSON (symbols on a directory) has no top-level "more pages" warning | Silent-gap risk: an agent collected 773 of 14,070 symbols unaware; text mode prints "49 more pages" | `duplication.json` T1 |
| 6.2–6.5 | P3 | Document the graph fixes | Code + tests landed and pass (ast_graph 153/0 incl. new `store/tests/rust_test_code.rs`; clippy clean): engine `testLines`, edge `test` bit, graph format v6, cfg(test) mount roles, Rust module-lineage skip in dir-cycle, unjudged-export notes, no-graph message. Docs not updated | `docs/OCTOCODE_CLI.md#graph--persisted-code-graph` (test edges, dir-cycle rule, format v6 re-ingest) |
| 6.6 | P3 | astSearch/lspSearch hints suggest fields the published MCP schema doesn't list (`defaultExcludes`, `rustContext`, `pageSize`) | Discoverability: the runtime accepts them but agents can't see them in the schema | `efficiency.json` TD-1 |
| 6.7 | P3 | Hint length: the broadened-search note can exceed the 120-char guidance with 4–5 variants | Trips only the test assert (`response/rows.rs:266`) today | leads batch report |

## 7. Product / MCP direction

| # | Pri | Item | Reasoning | References |
|---|---|---|---|---|
| 7.1 | P2 | Native MCP in the binary (`octocode mcp` on rmcp 3.x); octocode-mcp → thin launcher, drop the N-API addon | Research (40 verified claims): rmcp is Tier 1 and serves 2026-07-28 + 2025-11-25; removes the core/native drift class (today's outages). Spike first: serve pre-built JSON schemas to both client eras, byte-parity vs Node, cold start | `.octocode/octocode-brainstorming/tmp/mcp-exposure-2026-10-23/` |
| 7.2 | P3 | Migrate test clients off legacy `@modelcontextprotocol/sdk` v1 | v2 is `@modelcontextprotocol/server`/client; 5 packages use v1 as a test client | package.json devDeps |
| 7.3 | P3 | MCP published surface grew +3.8 KB today (26.5 KB) | Accepted for published filters/descriptions; revisit with the budget tests if hosts truncate | core `surfaceBudget.ts` |

## Done today (for context, not TODO)

MCP: JSON-only text, opaque page cursor, dual-era protocol, secret-leak fix (`security/cut.rs`). 12-tool rating fixes. Consolidation of
leads, local access, GitHub, and paging/source-admission/walk-errors. oxlint + tsgo + strict TS + CI typecheck/pins, with 13 redundant
scripts removed. 400-line splits for all TS and most Rust. Rust quality tools (`dev-tools.json`, fuzz/Miri/llvm-lines; 2 fuzz
bugs fixed). Windows CI filters fixed.


---

FIX ALL 

---
status: Done
confidence: medium
question: Do all 12 Octocode MCP tools work, how good are their descriptions and results, and how should agents use them better?
next: Fix the description/behaviour mismatches in ../octocode (paging, minify standard, lspSearch levels and workspaceSymbol, ghSearchCode tokens, compare pagination, artifactSearch pageSize); in octocodecode, merge the errorMessage/isRecord duplicates after the tool removal lands.
---

# Octocode MCP tools: deep check (2026-10-23)

Server: local build `octocode-mcp` 20.2.0 (`/Users/bgaryy/code/octocode/packages/octocode-mcp/dist/index.js`), after the native rebuild that fixed contract drift. Session: octocodecode, connected, 12 tools. Local tools are declared directly; GitHub and package tools load through `tool_search` (loaded in one call). About 60 queries across 25 calls. GitHub calls use the `gh` login of `bgauryy`.

Scores: Works = correctness on tested features; Desc = how well the tool description predicts the behaviour; Use = token efficiency and next-step guidance. Overall is a judgement, not an average.

| Tool | Works | Desc | Use | Overall |
|---|---|---|---|---|
| localFetch | 10 | 7 | 9 | 9 |
| localSearch | 9 | 8 | 9 | 9 |
| astSearch | 9 | 9 | 9 | 9 |
| structureSearch | 9 | 9 | 8 | 9 |
| lspSearch | 7 | 7 | 6 | 7 |
| ghSearchRepo | 9 | 9 | 9 | 9 |
| ghStructure | 9 | 9 | 9 | 9 |
| ghGetFileContent | 9 | 6 | 9 | 8 |
| ghSearchHistory | 9 | 9 | 9 | 9 |
| ghGetHistoryItem | 8 | 8 | 7 | 8 |
| ghSearchCode | 7 | 8 | 7 | 7 |
| artifactSearch | 9 | 8 | 7 | 8 |

## Per tool

### localSearch: 9
- Tested: Rust regex with context; `countMatches`; `files`; `fileHas`/`fileLacks`/`include`; an absent token; `clasifyContext`.
- Results: all correct. The absent token reported `filesScanned: 875`, that skipped vendor files hold no match, and binary files skipped by type. That is a trustworthy "no". `clasifyContext` ("is errorMessage defined here?") correctly selected 0 of 24 hits in `extensions/mcp` after the dedupe, and offered `next.unfiltered`.
- Description: good. The `clasifyContext` warning "selected chunks hold 0 of 24 hit rows" reads like a failure; it is the correct answer.
- Use better: use `countMatches` before reading hits for common names; use `fileHas`/`fileLacks` instead of a second search; treat the empty-result receipt (`filesScanned`, skipped paths) as the evidence for absence.

### localFetch: 9
- Tested: `minify: symbols`, `matchString` + `block`, a large file without range, a missing path, `ranges` + `minify: standard`.
- Results: the outline gives line spans per symbol (truncate.ts, 337 lines, in about 40 rows); `block` returned exactly `measureForTruncation` 103-132; missing path gave `pathNotFound` and two concrete recovery calls; an over-long range was clipped with a warning.
- Description mismatch: "files over 16 KiB ... return the first 200 lines". In practice the window is sized by `responseLength` (3000 chars returned 37 lines; a 5-query batch at 14000 returned 40), with `next.continueWindow`. The behaviour is good; the description is wrong.
- Use better: outline (`minify: symbols`) then `matchString` + `block` for the symbol; avoid paging whole files.

### structureSearch: 9
- Tested: `files` sorted by `lines`, `tree`, `time.modifiedWithin`, `nameRegex`, `empty`.
- Results: correct. `sort: lines` is a quick max-lines audit (agent-session.ts 577, then several at 395-400). `modifiedWithin: 2h` listed every file touched in this session with timestamps, useful for reviewing concurrent agents. The empty query returned a hint naming the filters to loosen.
- Use better: `sort: modified` + `modifiedWithin` to see what other agents changed; `sort: lines` for file-size rules.

### astSearch: 9
- Tested: pattern with `$VAR`/`$$$`, expression pattern, `rule` with `kind`/`not`/`has`, `syntaxTree`, `symbols` with kinds across a directory.
- Results: the strict pattern `function isRecord($V: unknown): $T { $$$ }` matched 0, and the diagnostic explained why (the real declarations are exported and spell the return type) and gave a rule that lists the kind. That is excellent failure guidance. The expression pattern `$E instanceof Error ? $E.message : String($E)` found **96 inline copies** of what `utils/guards.ts errorMessage` does, a duplication that jscpd cannot see.
- Use better: use AST patterns for semantic duplicates that text and token tools miss; start with a `rule` by kind when a pattern returns nothing.

### lspSearch: 7
- Tested: references (common and rare name), callers, callees, definition across packages, subtypes, hover, diagnostic, documentSymbols, workspaceSymbol.
- Good: `definition` crossed into `packages/ai` with context; `callers` of `measureForTruncation` complete; `callees` listed each call with its owner; `hover` gave signature and doc; references for a rare name complete and corrected the line hint (`lineDeviation: 1`).
- Weak: references for `isRecord` (94 candidate files) verified only 24 per page and labelled all hits `recoveredImporter`, so a full answer needs four calls. `subtypes` returned only direct subclasses (level 1); the two classes that extend `SummaryMessageComponent` were missing, with no depth option or hint. `workspaceSymbol` with only `workspaceRoot` returned nothing and said it covered "the typescript project of one file".
- Use better: for common names, run `localSearch` `countMatches` first and use LSP only to disambiguate; always pass `path` (not only `workspaceRoot`) for `workspaceSymbol`; repeat `subtypes` on each result for a full hierarchy.

### ghSearchRepo: 9
- Tested: keywords + language + stars + sort, topics + pushed, owner listing, missing repo.
- Results: correct, compact rows; provider caps are stated ("20030 results; first 1002 reachable"); the missing repo returned `notFound` with a recovery query.
- Use better: owner listing finds sibling repos (it showed `octocode-mcp-host`, which `../octocode` resolves `octocode-core` from).

### ghSearchCode: 7
- Tested: `filename` + `matchIn: path`, two ANDed keywords, `extensions` across an org, `clasifyContext`.
- Results: path search and org search worked with `commitSha` and a ready `readHits` call. Keywords `contracts` + `regen` returned README lines that contain neither word as written (GitHub's tokenizer matches "Regenerate" and other forms). The literal `contracts:regen` returned nothing because the index is the default branch, last committed 2026-08-25. `clasifyContext` kept 2 of 6 files, and its `relatedItems: 3` category is not explained, so a call `server.registerTool(` in `registerBasicTool.ts` dropped out of view without a reason.
- Use better: prefer one distinctive identifier over several words; confirm hits with `ghGetFileContent` `matchString` (the tool already offers that call); remember that the index trails unpushed and non-default-branch work, so use local tools for the working tree.

### ghStructure: 9
- Tested: refs, languages, tree with `include` + depth, missing path.
- Results: correct; each response names `commitSha`. Tags stop at 9.1.1 while packages are at 19-20, which is repository data, not a tool fault, but worth knowing before trusting tags as versions.

### ghGetFileContent: 8
- Tested: `minify: symbols`, `matchString` + `block`, `ranges` + `minify: standard`, a large file without range, a missing path.
- Results: correct and pinned to `commitSha`; `block` returned the whole `withOutputSanitization` function.
- Description mismatch: `minify: standard` is described as dropping comments and blank lines, but it also removes spaces inside code (`import{McpServer}from '...'`). Quotes from that view are not exact source. The same `responseLength` paging mismatch as localFetch applies (33 lines returned, not 200).
- Use better: read with `matchString` + `block`, or plain `ranges`, whenever you will quote; use `minify: standard` only to skim.

### ghSearchHistory: 9
- Tested: merged PRs, open issues, commits by path, PR keywords in another org.
- Results: correct and compact; the PR keyword search in `modelcontextprotocol/typescript-sdk` found three open stdio fixes (listener accumulation #3073, stderr drain #2794/#2921) that are directly relevant to this repo's MCP stdio client.

### ghGetHistoryItem: 8
- Tested: PR summary, PR `files`, PR `matchString` patches, compare with `include`, issue comments, and earlier a commit summary.
- Results: PR views are well layered: summary first, then files, then only matching hunks ("dropped 2 of 3 files"). A commit summary with no `include` listed about 200 files and was the most expensive call in the check. The compare row reported `commits: currentPage 0, hasMore true, totalItems 1`, an inconsistent pagination record.
- Use better: always pass `include` or `sections: ["files"]` for commits and compares; use `matchString` to fetch only the hunks you need.

### artifactSearch: 8
- Tested: npm range `^1.0.0`, PyPI, crates, npm keywords, missing package.
- Results: the range resolved to the highest match (1.32.1) with `verification: provenance`; PyPI and crates returned repo, source ref and dependencies; a missing package gave `notFound` plus a keyword fallback marked Approximate. The keyword search returned 20 rows with no page-size control and several unrelated packages (fuse.js, @codemirror/search).
- Use better: use exact package names; treat keyword results as leads, and rank them by `downloadsMonthly` and repository.

## First pass (before the deep check)

One call per tool, to prove each one answers after the native rebuild:

| Tool | Call | Result | First-pass score |
|---|---|---|---|
| localSearch | `canDiscoverTools` in `packages/coding-agent/src`, files view | 4 files, skipped vendor file named with a hint | 9 |
| localFetch | `packages/coding-agent/src/utils/guards.ts` | exact 11 lines | 10 |
| structureSearch | `core/tools`, files, depth 1 | 10 files with sizes (tool removal in progress) | 9 |
| astSearch | symbols `canDiscoverTools`, `activateDiscovery` | lines 23-25 and 28-36, exported, doc start | 9 |
| lspSearch | references of `isRecord` | partial: 24 of 94 candidate files per page | 6 |
| ghSearchRepo | `bgauryy/octocode` | metadata, 955 stars | 10 |
| artifactSearch | npm `octocode-mcp` | 19.1.0, deps, sourceRef `24ada10` | 9 |
| ghStructure | `packages/octocode-mcp` tree | listing at `c265e3f` | 9 |
| ghSearchCode | `contracts:regen` | empty (default-branch index trails); `registerTool` then found 6 hits | 7 |
| ghGetFileContent | `package.json` version at `24ada10` | `"version": "19.1.0"` | 9 |
| ghSearchHistory | latest commits | `c265e3f` #472, `af20f66` #471 | 9 |
| ghGetHistoryItem | commit `24ada10` | correct, but about 200 changed files in the summary | 7 |

The deep check raised localFetch and ghSearchRepo to 9 (description mismatch, nothing wrong in results), lspSearch to 7 (definition, callers, callees and hover are strong) and ghGetHistoryItem to 8 (PR views are well layered).

## Test inventory (deep check)

| Tool | Query | Outcome |
|---|---|---|
| localSearch | Rust regex `export function (isRecord\|errorMessage)\b`, contextLines 1 | 4 hits: `octocode/shared/util.ts:228,247`, `utils/guards.ts:4,9` |
| localSearch | `TOOL_SEARCH_TOOL_NAME`, countMatches | tool-search/tool.ts 6, mcp/tool-registry.ts 4 |
| localSearch | absent token | empty, filesScanned 875, skipped and binary files explained |
| localSearch | `sleepUnlessAborted` in `packages`, fileHas `signal`, fileLacks `test`, `*.ts` | 7 files (ai abort/retry/codex/device-code/sleep, coding-agent retry, llama client) |
| localSearch | `errorMessage` in extensions/mcp + clasifyContext "defined here?" | 0 of 24 selected (correct after dedupe), next.unfiltered offered |
| localFetch | truncate.ts `minify: symbols` | outline with spans, 337 lines |
| localFetch | `function measureForTruncation` + block | lines 103-132 exactly |
| localFetch | agent-session.ts, no range | 37-40 lines per window (responseLength), next.continueWindow |
| localFetch | missing path | `pathNotFound` + 2 structureSearch recovery calls |
| localFetch | output.ts ranges 8-20, minify standard | clipped to file end (10 lines) with a warning |
| structureSearch | core, files, sort lines, depth 3 | 250 files, largest agent-session.ts 577 lines, paged |
| structureSearch | octocode, tree depth 1 | 6 files, 20 dirs |
| structureSearch | modifiedWithin 2h, sort modified | 82 files touched this session with timestamps |
| structureSearch | nameRegex for the new helper files | guards.ts, output.ts, framed-view.ts, collapsible-message.ts |
| structureSearch | empty: true | none, with a loosen-filters hint |
| astSearch | pattern `function isRecord($V: unknown): $T { $$$ }` | 0, diagnostic explains exported/return-type mismatch and offers a kind rule |
| astSearch | pattern `$E instanceof Error ? $E.message : String($E)` | 96 matches in coding-agent src |
| astSearch | rule catch_clause without identifier, extensions/mcp | 5 bare catches in 4 files |
| astSearch | syntaxTree output.ts line 4 | tree from line 6 with a blank-line diagnostic |
| astSearch | symbols kind class `Message` in components | 10 classes incl. the new CollapsibleMessageComponent |
| lspSearch | callers `measureForTruncation` | truncateHead, truncateTail; complete |
| lspSearch | definition `sleepUnlessAborted` from retry.ts | `packages/ai/src/utils/abort.ts:54` with context |
| lspSearch | subtypes `CollapsibleMessageComponent` | SkillInvocation and Summary only (level 1) |
| lspSearch | hover `canDiscoverTools` | signature + doc |
| lspSearch | diagnostic framed-view.ts | no diagnostics |
| lspSearch | workspaceSymbol `withTogglePattern`, workspaceRoot only | empty, "covered the typescript project of one file" |
| lspSearch | references `withTogglePattern` | 3 sites in resource-settings.ts, complete, line hint corrected |
| lspSearch | callees `addToolExecution` | 6 callees with owners |
| lspSearch | documentSymbols package-manager/discovery.ts | full nested outline |
| ghSearchRepo | `mcp` + `code search`, TypeScript, stars >500, sort stars | zilliztech/claude-context, Mantic.sh |
| ghSearchRepo | topic model-context-protocol, pushed 30d | capped 1002 of 20030, paged |
| ghSearchRepo | owner bgauryy | octocodecode, octocode, octocode-code, octocode-mcp-host, octocode-workshop |
| ghSearchRepo | missing repo | notFound + keyword search hint |
| ghSearchCode | filename package.json, matchIn path, `packages` | 15 paths, paged |
| ghSearchCode | keywords `contracts` + `regen` | 4 README hits matching other word forms |
| ghSearchCode | org modelcontextprotocol, `StdioClientTransport`, ts | 52 hits, `packages/client/src/client/stdio.ts:107` |
| ghSearchCode | `registerTool` + clasifyContext "calls registerTool?" | 2 selected, 1 excluded, 3 "related" of 6 |
| ghStructure | refs / languages | default main at c265e3f, tags to 9.1.1; TS 5.0 MB, Rust 0.97 MB |
| ghStructure | octocode-mcp/src `**/*.ts` depth 2 | 11 files |
| ghStructure | missing path | notFound + tree hint pinned to the commit |
| ghGetFileContent | registerRemoteTool.ts symbols | outline, 108 lines |
| ghGetFileContent | secureServer.ts `registerTool` + block | `withOutputSanitization` 139-175 |
| ghGetFileContent | index.ts ranges 1-15 minify standard | spaces stripped inside imports |
| ghGetFileContent | README.md no range | 33 lines, continueWindow |
| ghGetFileContent | missing path | notFound + tree hint |
| ghSearchHistory | merged PRs / open issues | 395 merged (#472, #471, #470); 1 open issue #473 |
| ghSearchHistory | commits by path octocode-mcp/src/tools | 2 commits |
| ghSearchHistory | typescript-sdk PRs `stdio` + `stderr` | #3073 listener accumulation, #2794 and #2921 stderr drain |
| ghGetHistoryItem | PR #3073 summary / files / matchString `removeListener` | layered: summary, 3 files, only the matching hunk |
| ghGetHistoryItem | compare af20f66...c265e3f include octocode-mcp | 1 file shown; commit pagination `currentPage 0, hasMore true, totalItems 1` |
| ghGetHistoryItem | issue #473 comments | 1 comment |
| artifactSearch | npm `@modelcontextprotocol/sdk` `^1.0.0` | 1.32.1, provenance-verified |
| artifactSearch | pypi `mcp` / crates `tree-sitter` | 2.3.0 (tag), 0.27.1 (registryRef) |
| artifactSearch | npm keywords `mcp` + `code search` | 20 rows, several unrelated, capped 5020 of 2.1M |
| artifactSearch | missing package | notFound + Approximate keyword fallback |

Not tested: `ghStructure` `materialize`, `clasifyContext` on localFetch and ghGetFileContent, lspSearch `typeDefinition`, `implementation`, `supertypes`, and `references` paging to completion.

## Cross-cutting findings

1. Error and empty results are consistently good: an error code, the scope checked, and a runnable next call. This is the strongest part of the toolset.
2. Paging is by `responseLength` (characters), not by the line counts the descriptions state. Descriptions should say so.
3. Batching five queries shares one response budget, so each row gets a smaller window. Batch independent small queries; send large reads alone.
4. Every GitHub result names `commitSha`; cite it.
5. Local tools see the working tree including other agents' uncommitted edits; GitHub tools see the last pushed default branch (here 2026-08-25). Choose by which state you need.

## Findings about this repository from the check

- `octocode/shared/util.ts:228,247` still define `errorMessage` and `isRecord`, duplicates of the new `utils/guards.ts`.
- 96 inline `x instanceof Error ? x.message : String(x)` expressions in `packages/coding-agent/src` (astSearch) could use `errorMessage`.
- The local server (20.2.0) is ahead of npm (19.1.0) and of the pushed `main`.

Follow-up in octocodecode: merge the `util.ts` copies into `utils/guards.ts` and replace the inline expressions with `errorMessage` after the tool-removal change lands (it edits files in the same area).

Upstream reading worth doing before touching this repo's MCP stdio client: `modelcontextprotocol/typescript-sdk` #3073 (stdio listener accumulation) and #2794/#2921 (piped stderr drain deadlock). Use ghSearchHistory to check upstream for a fix before patching locally.

## Suggested fixes to the tool descriptions

The descriptions are owned by the sibling repository `/Users/bgaryy/code/octocode` (octocode-mcp and its tool contract), not by octocodecode.

| Tool | Change |
|---|---|
| localFetch, ghGetFileContent | Replace "return the first 200 lines" with "return a window sized by responseLength; continue with next.continueWindow" |
| localFetch, ghGetFileContent | `minify: standard` also removes whitespace; quote from raw or `block` reads |
| lspSearch | `subtypes` and `supertypes` return one level; state it, or add depth. `workspaceSymbol` needs a source `path` in the project |
| ghSearchCode | Keywords match GitHub's tokens, not literal substrings; explain `relatedItems` in `clasifyContext` selections |
| ghGetHistoryItem | Fix the compare commit pagination record; suggest `include` before a large commit summary |
| artifactSearch | Allow `pageSize` for keyword search |
