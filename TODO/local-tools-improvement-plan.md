# Improve local tool correctness and efficiency

Status: Phases 0–3, 5, 7 done; Phase 4 gate passed and bounded continuation reuse shipped for localSearch (other walk measurements open); Phase 6 gate failed (global lock kept). Audit date: 2026-10-01; implementation 2026-10-02.

Make local tool evidence correspond to the submitted query, make cancellation stop background work, and reduce repeated scanning without weakening source identity or redaction. Keep the shared Rust runtime and generated contract pipeline. Improve the boundaries implicated in the audit before considering broader refactors.

The [local tools audit](../.octocode/octocode-dev/local-tools-2026-10-01.md) reproduced a cached-search correctness failure through a fresh MCP server. Source inspection also identified a cancellation defect and repeated-read costs. Matcher divergence, contention, and performance improvements remain measurement questions. Reproduce the baseline on matching artifacts before implementation because other sessions can change the source and builds.

Classification capture, sufficiency, and provider economics belong to the [clasify improvement plan](clasify-improvement-plan.md). This plan covers local execution and the shared primitives that other tools reuse.

## Delivery order

| Phase | Change | Why it helps | Completion gate |
| --- | --- | --- | --- |
| 0 | Freeze a reproducible baseline | Prevent stale artifacts or concurrent changes from becoming unnecessary fixes | Matching fingerprints and retained reproduction receipts |
| 1 | Validate cached search identity | Prevent plausible evidence for the wrong query | Changed expressions and scopes reject old cursors |
| 2 | Propagate live cancellation into workers | Stop work after cancellation or deadline expiry | Cancellation after worker launch reaches the active scan |
| 3 | Stream bounded security verification | Avoid retaining an entire prefix for one late-file hit | Redaction remains correct while verification memory stays within its budget |
| 4 | Measure and reduce repeated continuation work | Make multi-page research cheaper across a complete task | Measured benefit with unchanged evidence and coverage |
| 5 | Test search and rewrite parity | Establish which shared patterns have equivalent selection semantics | Supported cases agree or documented differences remain explicit |
| 6 | Evaluate narrower rewrite locks | Allow independent previews to proceed while preserving recovery coordination | Measured concurrency benefit and passing transaction tests |
| 7 | Verify the real CLI and MCP paths | Catch adapter, contract, pagination, and composition regressions | Rebuilt interfaces reproduce the acceptance cases |

Phases 1 and 2 are the first correctness milestone. Finish those before expanding caches or changing synchronization. Phase 3 precedes performance rollout because repeated work can also multiply security-verification costs. Phase 6 follows parity checks and a contention baseline.

## Phase 0: freeze the baseline

- [x] Record source revisions, local changes, core/native fingerprints, artifact versions, relevant configuration, and enabled tools without credentials.
- [x] Wait for ongoing builds to finish before evaluating their outputs. Resolve contract drift through regeneration and rebuilding.
- [x] Reproduce the cached-query failure in a fresh persistent MCP process with a two-file fixture.
- [x] Search only fixture source files so saved receipts cannot accidentally match the negative control expression.
- [x] Preserve exact requests, responses, continuations, source hashes, and control results.
- [x] Run the existing focused checks and record failures before changing implementation.

Why this helps: the audit first encountered fingerprint drift, then a temporarily missing CLI launcher during another build. Both conditions later cleared. A stable baseline separates these environment events from implementation defects.

Evidence: [fresh MCP reproduction](../.octocode/tmp/local-tools-audit-20261001/fresh-mcp-receipts.json), [CLI receipts](../.octocode/tmp/local-tools-audit-20261001/cli-receipts.json), and [source hashes](../.octocode/tmp/local-tools-audit-20261001/source-hashes.json).

### Results (2026-10-02)

- Baseline: HEAD 81530d26c, clean native tree; artifacts built 2026-10-01 23:15; `scheme` exit 0 (no drift); full lib suite green before changes except a load-sensitive GitHub credential-discovery timeout that passed on rerun.
- Re-verified on that build in a fresh persistent MCP (`node .octocode/tmp/local-tools-impl-20261001/repro-cache-identity.mjs …/phase0-before.json`): the `alpha` cursor reused with `searchText`, `include`, `defaultExcludes`, or `caseMode` changed still returned `other.ts:2 alpha(3)`; fresh control → empty. Receipts: `.octocode/tmp/local-tools-impl-20261001/phase0-before.json`.
- Re-verified source defects still open: importer scan `cancel.check().is_err()` captured once (`lsp_search/importers.rs`), prefix-retaining `read_leading_lines`, process-wide `APPLY_LOCK` on preview.

## Phase 1: bind cached results to the query

- [x] Validate the submitted search identity on every cache hit before returning evidence.
- [x] Define identity from the validated root and canonical query with defaults applied. Include every field that changes the collected evidence, including `defaultExcludes`, ignore behavior, filters, regex mode, and case mode.
- [x] Explicitly separate permitted pagination changes from changes to search semantics. Preserve the supported continuation protocol.
- [x] Bind cached scans to the applicable policy context and revalidate path access when serving them.
- [x] Return a typed stale-cursor error and an executable restart when identity differs. Never combine old matches with a new query identity.
- [x] Add behavioral checks for cache hits, cache misses, expiry, and eviction using the same requests.

Why this helps: the audited `noIgnore: true` branch retrieves a manifest by snapshot and bypasses the expected-identity comparison. Reusing an `alpha` cursor with a nonexistent expression returns `alpha` matches. Correct coordinates and a valid output schema do not make those matches relevant to the submitted query.

Acceptance checks:

- Replaying an unchanged continuation returns the expected next evidence without duplicates or omissions.
- Changing the expression, root, include/exclude filters, default exclusions, regex mode, or case mode with an old cursor rejects the request or starts an explicitly identified new search.
- The nonexistent-expression control produces zero matches regardless of cache state.
- A policy change cannot reuse a cursor to return source denied under the updated policy.
- Eviction or expiry produces a supported rescan or restart outcome, never evidence from another query.

Owners: [search execution and fingerprinting](../packages/octocode-native/crates/runtime/src/tools/local_search/executor.rs) and [search manifests](../packages/octocode-native/crates/runtime/src/tools/local_search/manifest.rs).

### Results (2026-10-02)

- `executor.rs`: the expected-snapshot comparison now runs on cache hits too (identity recomputed from the submitted query over the stored scan); identity uses the validated canonical root and adds `defaultExcludes`; doc comment lists the pagination-only fields (`page`, `matchPage`, `pageSize`, `maxMatchesPerFile`). Mismatch → `staleSnapshot` + `next.restart` (no snapshot, page 1).
- `manifest.rs`: entries keyed by snapshot **and** path-policy identity (allowed roots); served files are still revalidated with `validate_read`.
- Tests: `local_search::tests::frozen_snapshot_rejects_changed_search_semantics_on_cache_hit_and_miss` (searchText/include/defaultExcludes/regex/caseMode/hidden/wholeWord/path × hit and evicted; unchanged continuation identical; control empty) — failed before the fix; `local_search::manifest::tests::a_stored_scan_is_only_served_under_its_own_policy`. Expiry is the same miss path as eviction (TTL not injectable).
- Live (rebuilt, fresh MCP): every changed-semantics cursor → `errorCode: staleSnapshot`; unchanged continuation → `other.ts:2`; control → `status: empty` (`…/phase1-after.json`).

## Phase 2: propagate cancellation while work runs

- [x] Carry a live cancellation/deadline handle into LSP importer discovery instead of capturing a boolean before worker launch.
- [x] Reuse the runtime's existing cancellation machinery. Avoid introducing a second token or timeout system.
- [x] Check cancellation during traversal, between source reads, and inside expensive verification loops.
- [x] Preserve the distinction between cancelled work, provider failures, and empty evidence.
- [x] Add a deterministic test that starts the worker, confirms scanning has begun, then cancels it. Add a separate deadline-expiry case.

Why this helps: the importer scan passes a callback that always returns its startup cancellation state. A later cancellation can prevent a successful response while the underlying scan still continues. Live propagation saves resources and improves interruption behavior during large repository research.

Acceptance checks:

- The active scan observes cancellation after launch and releases its worker resources.
- A deadline that expires during scanning produces the established timeout outcome.
- No extra importer recovery starts after cancellation.
- Normal references and caller recovery retain their existing results and coverage labels.
- The test observes worker termination, rather than only checking that the public response says cancelled.

Owners: [LSP importer discovery](../packages/octocode-native/crates/runtime/src/tools/lsp_search/importers.rs), [execution context](../packages/octocode-native/crates/runtime/src/runtime/engine.rs), and [ordinary search cancellation](../packages/octocode-native/crates/runtime/src/tools/local_search/executor.rs).

### Results (2026-10-02)

- `lsp_search/mod.rs`: `blocking_cancellable` runs a blocking worker whose stop callback is an `AtomicBool` flipped when the existing `cancellable` poll (50 ms, `ExecutionContext::check` = cancel + deadline) fails or the request future is dropped — no second token/timeout system.
- `importers.rs`: `candidate_files` uses it and returns `Err(lsp.cancelled)` (`verified_anchors` propagates with `?`, so no further opens/identity checks start); a failed scan is now `importerScan: "failed"` → partial row with reason `importerScanFailed` + `next.textSearch` (`inferred_project.rs`; documented in `docs/OCTOCODE_TOOLS.md`) instead of a false `complete`. Cancellation checks already existed between candidate reads/identity loops.
- Tests (`lsp_search::importers::tests`): `a_running_worker_stops_when_the_request_is_cancelled_after_launch` (worker signals start, canceller flips, worker observes its stop callback), `a_deadline_expiring_during_the_scan_stops_the_worker_as_a_timeout`, `dropping_the_request_stops_the_worker`, `the_candidate_scan_reports_cancellation_after_launch` (real ripgrep scan; failed with the old captured boolean). Normal references unchanged: Phase 7 alias/identical-spelling LSP check passes.

## Phase 3: bound security-verification reads

- [x] Replace retained leading lines and a joined full prefix with a streaming pass that tracks private-key block state and stores only the neighborhoods needed for the selected matches.
- [x] Bound retained bytes and individual-line buffers, and check cancellation during the pass.
- [x] Preserve the information needed to detect key bodies whose boundary lies outside the displayed snippet.
- [x] Fail closed when verification cannot complete; return redacted placeholders and the established coverage/error information.
- [ ] Bind any reused verification state to source content and policy. Define how concurrent source changes invalidate verification before adding reuse.
- [ ] Measure whether sharing an existing source scan avoids duplicate reads without coupling unrelated tool policies.

Why this helps: every displayed non-list search page can reread the prefix through its latest hit, retain all those lines, and allocate another joined string. A small response can therefore consume memory proportional to a large prefix. Streaming reduces retained memory; reusing a valid scan can also reduce I/O. Streaming alone does not eliminate the need to inspect earlier key boundaries.

Acceptance checks:

- Small and large synthetic private-key blocks remain redacted for both interior body hits and unterminated blocks.
- Innocent base64 outside key blocks remains readable under the existing policy.
- Unreadable, replaced, or growing sources never expose unverified snippets.
- Late-file hits, giant lines, multiline snippets, Unicode, and cancellation obey explicit resource budgets.
- Original-source coordinates and redaction warnings remain accurate.
- Verification memory stays within the selected budget instead of increasing with the entire prefix length.

Owners: [search verification](../packages/octocode-native/crates/runtime/src/tools/local_search/executor.rs), [key-block detection](../packages/octocode-native/crates/runtime/src/security/content.rs), and [sanitized-view reuse](../packages/octocode-native/crates/runtime/src/security/scan.rs).

### Results (2026-10-02)

- `security/content.rs`: `KeyBlockTracker` (incremental `private_key_block_line_ranges`; the whole-text function now uses it).
- `executor.rs`: `guard_clipped_secrets` streams the prefix once (`read_verification_lines`/`read_bounded_line`): key-block state over every line, retains only the shown matches' neighborhoods within 16 MiB, buffers ≤64 KiB of other lines (longer ones consumed in chunks; one that mentions `PRIVATE KEY` makes the source unverifiable → fail closed), polls cancellation every 4096 lines / 1 MiB. New fail-closed cases: match line beyond EOF (source shrank/replaced — previously left unverified, test failed before the fix) and oversized neighborhood (`[REDACTED: source lines too large for secret check]`). Returns `Err` on cancellation (row → `cancelled`).
- Tests: `clipped_secret_guard_fails_closed_when_the_match_line_is_gone`, `late_key_bodies_in_large_sources_stay_redacted` (>10 MiB, terminated + unterminated, matchOnly + detailed, innocent base64 readable, exact line 560003), `verification_tests::{a_late_hit_retains_only_its_neighborhood_with_exact_coordinates, streamed_key_state_matches_the_whole_file_scan, an_unbufferable_line_mentioning_a_private_key_fails_closed, a_neighborhood_over_the_retained_budget_fails_closed, verification_stops_when_cancelled}`.
- Live (CLI, 315 MB file, hit at line 5,156,932): peak RSS 834 MB before → 65 MB after (`/usr/bin/time -l`, 2 runs 64.8/64.9 MB); latency unchanged (13.1 s debug build, dominated by the scan).
- Open: verification state is not reused across pages, and the ≤10 MiB whole-file key scan in the first redaction loop still reads matched files whose snippets look like key material. Reuse needs a source+policy-bound design; not measured.

## Phase 4: reduce repeated work where measurement supports it

- [ ] Measure a full page walk for text search, file discovery, AST symbols, and topology rather than measuring the first response alone.
- [ ] Measure repeated bounded `localFetch` reads separately; selecting a few lines from a small-file source still reads the whole file on that path.
- [ ] Record traversal count, source bytes read, parse count, peak memory, cache bytes, and end-to-end latency.
- [ ] Compare cold and warm runs in persistent MCP and separate CLI processes. Process-local caches do not survive ordinary CLI invocation boundaries.
- [x] After phase 1, evaluate bounded scan reuse independently of `noIgnore`. Keep ignore semantics about corpus selection.
- [x] Reuse existing cache infrastructure where it fits, with an aggregate byte budget, expiry, eviction, and query/source/policy identity.
- [x] Preserve actionable restart behavior when a snapshot becomes stale or unavailable.
- [ ] Compare topology's ordinary scan path with the existing `graph ingest` / `graph query` workflow before adding another persistent index.

Why this helps: page sizes primarily control visible output. Ordinary search pages can rescan, non-path discovery sorts rank a collected walk, and topology analyses can rebuild the graph. Reuse can lower complete-task cost, but broader caches introduce memory, invalidation, and stale-evidence risks. Measurements determine which changes justify that cost.

Proposed experiment gate: freeze representative corpora, query cases, and thresholds before implementation. Include small repositories, a large monorepo, ignored/build directories, late-file hits, many result pages, source edits, and policy changes. Use release builds for performance comparisons and keep correctness cases separate from timing cases.

Require at least a 20% improvement in median complete-walk latency for the selected repeated-work case. Limit p95 latency regression to 5%. Keep retained memory within the frozen budget. Require zero evidence or coverage regressions. Treat these thresholds as a proposal; the audit does not establish measured gains. Freeze the thresholds before collecting results. Repeat runs and report variation; treat noisy or inconclusive measurements as insufficient evidence to expand the change.

Frozen before collection (2026-10-02 00:20): case = complete localSearch page walk (`sort:"path"`, `pageSize:20`, `maxMatchesPerFile:1000`, page 1 → last via the page-1 snapshot) in one process, release profile, frozen-scan reuse (`noIgnore:true`, the existing manifest) vs per-page rescan (`noIgnore:false`) on corpora whose ignore rules hide nothing present (walked evidence must be identical), 7 alternating repetitions per mode. Gate: reuse median ≥20% faster, p95 ≤5% worse, manifest within its 1 MiB/64-entry budget, identical evidence. Pass → evaluate extending bounded reuse beyond `noIgnore`; fail or noise → no cache change.

Owners: [local search](../packages/octocode-native/crates/runtime/src/tools/local_search/), [local fetch](../packages/octocode-native/crates/runtime/src/tools/local_fetch/), [file discovery](../packages/octocode-native/crates/runtime/src/tools/structure_search/files.rs), [topology collection](../packages/octocode-native/crates/runtime/src/tools/ast_graph/graph.rs), and [shared cache infrastructure](../packages/octocode-native/crates/runtime/src/cache/). Measurement workflow: [evaluation skill](../skills/octocode-eval-benchmark/SKILL.md).

### Results (2026-10-02)

- Harness: temporary ignored release test (`cargo test --release -p octocode-native --test zz_local_walk_measure -- --ignored`, archived at `.octocode/tmp/local-tools-impl-20261001/measure-harness.rs.txt`, removed from the tree). Corpus `octocode-local-testing/repos/typescript`, `searchText:"Debug.assert"`, 4 pages, 713 hits, evidence identical across modes and runs.
- Before (7 alternating reps): rescan median 6340 ms, p95 6729 ms; reuse median 1649 ms, p95 1774 ms → 74% faster median, p95 better. Gate passed. Other probes: `repos/javascript` `function` 3 pages 257 → 210 ms (1 rep); `repos/nextjs` `process.env` 71 pages, rescan walk 69.2 s, ineligible (noIgnore changes its corpus).
- Shipped: continuations reuse their page-1 scan whatever `noIgnore` says (`executor.rs`, `manifest.rs`): stored only when the response offers a continuation; ≤1 MiB per scan, ≤16 MiB and 64 scans total (oldest evicted), 60 s TTL; keyed by snapshot + policy; each matched file's size+mtime must be unchanged; the snapshot is still recomputed from the submitted query. A file created after the scan is not seen until expiry (pages stay consistent with page 1); expiry/eviction rescans and restarts a changed result. Tests: `continuations_reuse_their_scan_until_a_matched_file_changes`, `manifest::tests::stored_scans_stay_within_the_aggregate_budget`.
- After (default `noIgnore:false`, same case): median 1847 ms, p95 1941 ms (was 6340/6729).
- Open: file-discovery/AST-symbol/topology page walks, repeated bounded `localFetch`, traversal/bytes/parse counts, CLI-vs-MCP cold/warm (the reuse is process-local: no CLI benefit), and the `graph ingest` comparison were not measured; no change made for them.

## Phase 5: establish search and rewrite compatibility

- [x] Build a shared fixture corpus that runs equivalent supported patterns through `astSearch` and `astRewrite` preview.
- [x] Compare selected files, source spans, captures, and invalid-input outcomes before comparing replacement text.
- [x] Cover metavariables, repeated captures, sibling sequences, relational rules, constraints, punctuation, comments, partial syntax, and supported languages.
- [x] Separate matcher differences from differences in scope, ignore rules, limits, or policy. Compare both engines over the same candidate files and declared overlap.
- [x] Document supported differences in authored core guidance and the owning engine docs.
- [ ] Evaluate engine consolidation only after the corpus reveals the compatibility and performance tradeoffs.

Why this helps: structural search uses Octocode's matcher, while rewrite uses embedded ast-grep. Shared grammars do not establish equal selection semantics. Compatibility tests make an agent's search-to-preview workflow more dependable without assuming that replacing either engine is the right solution.

Acceptance checks:

- Cases in the declared shared subset select equivalent source spans and captures.
- Intended differences have explicit guidance and regression cases.
- Unsupported patterns fail with actionable errors rather than misleading empty results.
- Search results alone never authorize applying a rewrite; preview retains its snapshot, hash, syntax, and postcondition guards.

Owners: [structural search engine](../packages/octocode-native/crates/engine/src/structural/mod.rs), [rewrite engine](../packages/octocode-native/crates/engine/src/structural/rewrite.rs), and [public structural search](../packages/octocode-native/crates/runtime/src/tools/ast_search/matches.rs).

### Results (2026-10-02)

- Corpus: `crates/engine/src/structural/parity_tests.rs` (41 cases, same source + same rule through `structural::search` and `structural::rewrite`, so scope/ignore/limits/policy are out of the comparison; JSON rule documents feed both). Compares 0-based spans, UTF-16 columns, text, and captures (ast-grep's internal `secondary` relational capture filtered). Covers single/multi/ignored metavars, repeated captures, empty `$$$` beside punctuation, block bodies, `inside`/`has` (default and `stopBy: end`), `all`/`any`/`not`, `kind`/`regex`, trailing commas, comments, partial syntax, Unicode, TS/TSX/Python/Rust/Go/Java/C/C++/C#/Scala, invalid and unsupported inputs.
- Fixed Octo divergences (each failed in the first corpus run): `f(1, $$$M, 3)` missed `f(1, 3)` and `foo($$$A, x)` missed `foo(x)` (punctuation after `$$$` is now optional, as in ast-grep); a trailing `$$$B` (Python block) captured nothing (now takes the rest); a multi-node pattern (`a(); b()`) silently matched only files starting with that sequence (now an actionable error). `engine/src/structural/octo/pattern.rs`.
- Pinned intended differences: statement pattern ending in `;` (search 2 vs rewrite 1), `$K: $V` pair shorthand (2 vs 0), bare C call `foo($X)` (2 vs 0), `constraints`/`follows` rejected by search. Documented in `packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md` ("Search and rewrite matchers"); core guidance: astSearch `match: ast-grep syntax, own matcher`, astRewrite `Selection can differ from astSearch match; trust the preview.` (core tests 471/471, typecheck, build, `yarn contracts:regen`; budget snapshot updated, tools/list 32k test green).
- Open: engine consolidation not evaluated (corpus shows agreement on the shared subset; remaining gaps are search conveniences or ast-grep-only features; no performance comparison run). The bare-C-call rewrite miss is ast-grep behaviour and stays documented.

## Phase 6: evaluate lock scope for rewrite previews

- [x] Measure two independent previews over different roots and compare them with sequential execution.
- [ ] Separate pure preview analysis from interrupted-transaction recovery before changing lock placement.
- [ ] Evaluate coordination per canonical root using existing lock infrastructure.
- [ ] Preserve exclusion for identical and overlapping roots, and preserve recovery/apply ordering across processes.
- [x] Keep the global lock if narrower coordination has no material measured benefit or weakens transaction behavior.

Why this helps: previews acquire the process-wide rewrite lock before preparation, serializing independent roots. Narrower coordination can improve throughput, but preview can recover interrupted transactions. Removing locks without separating those effects risks races during recovery or apply.

Acceptance checks:

- Independent previews overlap only where the implementation establishes that their roots and effects are independent.
- Same-root and overlapping-root operations remain coordinated.
- Interrupted recovery, stale hashes, failed postconditions, cancellation, and rollback continue to satisfy their existing behavioral tests.
- Preview leaves an ordinary fixture unchanged; recovery effects remain documented.
- Measured contention reduction passes the frozen performance gate.

Frozen before collection (2026-10-02 00:25): one process, release profile, two `console.log($$$A)` → `console.debug($$$A)` previews over disjoint TypeScript roots, 5 repetitions: each alone, both sequentially, both concurrently under the current process lock. Gate: concurrent wall time ≥20% above the no-lock lower bound (max of the singles) with each preview ≥1 s; otherwise keep the global lock. Context: `astRewrite` is CLI-only, so the process lock can only contend within one multi-row CLI batch.

Owners: [rewrite orchestration](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/mod.rs), [root locks](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/lock.rs), and [transaction journals](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/journal.rs).

### Results (2026-10-02)

- Release, 5 reps, `huge-ts/src` (A) and `typescript` (B): A median 1345 ms, B 653 ms; sequential 1994 ms; concurrent under the global lock 1977 ms (fully serialized); no-lock lower bound 1345 ms. Corpora unchanged (`git status` empty before/after).
- Gate failed: B < 1 s, and the contention is unreachable in shipped interfaces — `astRewrite` is CLI-only and its batch rows run one at a time (`BatchBudget` width 1 for ordered tools, `runtime/engine.rs`). Global lock kept; recovery/lock separation not started.

## Phase 7: verify complete workflows

- [x] Keep field-effect inventory checks and add behavioral cases for the risky interactions: cursor/query identity, cancellation/worker launch, source changes/redaction, and selection/hash guards.
- [x] Verify discovery → search → exact read → semantic references on a fixture with aliases and identical spellings for different bindings.
- [x] Verify topology candidates → exact source → LSP references without treating syntactic reachability as safe-deletion proof.
- [x] Verify structural search → rewrite preview → guarded apply on an isolated fixture whose edit is explicitly intended. Exercise stale-source and failed-postcondition cases before accepting the rewrite changes.
- [x] Walk all result, match, source, and response-envelope continuations. Preserve explicit terminal limits and coverage gaps.
- [x] Verify shared extraction through local reads and a fixture-backed GitHub read so changes to a local primitive do not break its remote consumer.
- [x] Verify delegated local evidence through `clasify` without duplicating the provider-work improvements owned by its separate plan.

Why this helps: coverage labels and valid schemas prove structure, not behavior. Composition checks catch errors that single-tool unit tests miss, including wrong source identity, incorrect anchors, dropped pages, and changes to shared primitives.

### Results (2026-10-02)

- Behavioral cases added per risky interaction: cursor×query identity (Phase 1 test + live), cancellation×worker launch (Phase 2 tests), source change×redaction (Phase 3 shrunk-source/late-key tests), source change×continuation (`continuations_reuse_their_scan_until_a_matched_file_changes`), selection×hash guards (below). `contract_field_effects` unchanged and green.
- `node .octocode/tmp/local-tools-impl-20261001/phase7.mjs` with `P7_FIX=<scratchpad>/p7root/p7` (outside the repo so ignore rules don't hide it; `ALLOWED_PATHS` set) through a fresh MCP + CLI: 15/15 (`…/phase7-results.json`). Covers structureSearch → localSearch → localFetch → lspSearch references (aliased `renamed` import found; same-spelled local `alpha` in c.ts excluded); astTopology deadCode (`unused.ts`, `basis: syntactic`) → lspSearch (no refs, `exhaustive:false`); 30-file page walk (each file once); old cursor + changed `searchText` → `staleSnapshot`; clasify with a delegated `localFetch` resource; astRewrite preview → edit after preview → apply rejected, apply with failing `remainingMatches` postcondition rejected and file unchanged, fresh preview → apply committed.
- Shared extraction: CLI `ghGetFileContent` (prometheus@ea954809, `cmd/prometheus/main.go`, `matchString:"func main()"`, `contextLines:2`) and `localFetch` on the pinned clone return identical numbered content (`374`–`378`), `matchedLines:[376]`, `totalLines:2296`.
- Rebuilt interfaces (`dev.mjs build:dev` exit 0, 148.8 s): `config --json` 0, `scheme` 0. Harness (`OCTOCODE_BETA=true node octocode-local-testing/harness/run-all.mjs workflows,grammar,navigate,rewrite,large-files,perf`): 27/133/90/16/49/2 passed, 0 failed. `dev.mjs docs:verify` passed.
- Rust (2026-10-02, after all changes): `cargo test --locked -p octocode-native -p octocode-github -p octocode-cli --no-default-features` all green (runtime lib 1116 passed), `cargo test --locked -p octocode-engine --all-features` 877 passed, `cargo clippy --workspace --all-targets -- -D warnings` exit 0, rustfmt clean on touched files. Earlier full runs saw one load-sensitive `providers::github::auth::discovery` timeout that passed on rerun; `cargo test -p octocode-engine` without `--all-features` fails to link `graph_benchmark` (napi symbols), pre-existing and unrelated.

## Build and handoff gates

Follow the [development skill](../skills-dev/octocode-dev/SKILL.md) for the commands and the [tool-quality requirements](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md) for returned evidence.

| Change | Required verification |
| --- | --- |
| Native behavior | Focused regressions and `yarn workspace @octocodeai/octocode-native test:rust`; rebuild native and affected interfaces |
| Engine behavior | Engine and runtime regressions; rebuild native and affected interfaces; run real tool calls |
| Tool schema, limits, or guidance | Author in sibling `octocode-core`, build core, run `yarn contracts:regen`, immediately rebuild native, then rebuild affected consumers |
| Configuration | Follow the single config pipeline; verify defaults, environment behavior, and docs |
| Each completed phase | Run CLI `config --json`, `scheme`, and its affected cases; restart MCP and repeat relevant persistent-session cases |
| Change spanning packages | Run `node skills-dev/octocode-dev/scripts/dev.mjs verify` and `docs:verify` |

Use `build:dev` for ordinary local validation and release artifacts for performance experiments. Preserve coverage floors. Record commands, exit codes, source/artifact identity, and remaining limitations. Update this checklist from actual results; do not mark a phase complete because a registry or compilation check passed.

## First implementation batch

1. Freeze the fresh MCP cache reproduction and its negative control.
2. Fix query identity on cache hits and cover changed expressions and filters.
3. Replace captured cancellation with a live worker handle and prove interruption after launch.
4. Rebuild native and both interfaces, then repeat the cases in the CLI and a new persistent MCP server.

This batch addresses demonstrated wrong evidence and a source-confirmed cancellation defect with a limited change surface. Performance and synchronization changes follow their own measurement gates.
