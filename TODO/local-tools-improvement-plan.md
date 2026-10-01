# Improve local tool correctness and efficiency

Status: planned. Audit date: 2026-10-01.

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

- [ ] Record source revisions, local changes, core/native fingerprints, artifact versions, relevant configuration, and enabled tools without credentials.
- [ ] Wait for ongoing builds to finish before evaluating their outputs. Resolve contract drift through regeneration and rebuilding.
- [ ] Reproduce the cached-query failure in a fresh persistent MCP process with a two-file fixture.
- [ ] Search only fixture source files so saved receipts cannot accidentally match the negative control expression.
- [ ] Preserve exact requests, responses, continuations, source hashes, and control results.
- [ ] Run the existing focused checks and record failures before changing implementation.

Why this helps: the audit first encountered fingerprint drift, then a temporarily missing CLI launcher during another build. Both conditions later cleared. A stable baseline separates these environment events from implementation defects.

Evidence: [fresh MCP reproduction](../.octocode/tmp/local-tools-audit-20261001/fresh-mcp-receipts.json), [CLI receipts](../.octocode/tmp/local-tools-audit-20261001/cli-receipts.json), and [source hashes](../.octocode/tmp/local-tools-audit-20261001/source-hashes.json).

## Phase 1: bind cached results to the query

- [ ] Validate the submitted search identity on every cache hit before returning evidence.
- [ ] Define identity from the validated root and canonical query with defaults applied. Include every field that changes the collected evidence, including `defaultExcludes`, ignore behavior, filters, regex mode, and case mode.
- [ ] Explicitly separate permitted pagination changes from changes to search semantics. Preserve the supported continuation protocol.
- [ ] Bind cached scans to the applicable policy context and revalidate path access when serving them.
- [ ] Return a typed stale-cursor error and an executable restart when identity differs. Never combine old matches with a new query identity.
- [ ] Add behavioral checks for cache hits, cache misses, expiry, and eviction using the same requests.

Why this helps: the audited `noIgnore: true` branch retrieves a manifest by snapshot and bypasses the expected-identity comparison. Reusing an `alpha` cursor with a nonexistent expression returns `alpha` matches. Correct coordinates and a valid output schema do not make those matches relevant to the submitted query.

Acceptance checks:

- Replaying an unchanged continuation returns the expected next evidence without duplicates or omissions.
- Changing the expression, root, include/exclude filters, default exclusions, regex mode, or case mode with an old cursor rejects the request or starts an explicitly identified new search.
- The nonexistent-expression control produces zero matches regardless of cache state.
- A policy change cannot reuse a cursor to return source denied under the updated policy.
- Eviction or expiry produces a supported rescan or restart outcome, never evidence from another query.

Owners: [search execution and fingerprinting](../packages/octocode-native/crates/runtime/src/tools/local_search/executor.rs) and [search manifests](../packages/octocode-native/crates/runtime/src/tools/local_search/manifest.rs).

## Phase 2: propagate cancellation while work runs

- [ ] Carry a live cancellation/deadline handle into LSP importer discovery instead of capturing a boolean before worker launch.
- [ ] Reuse the runtime's existing cancellation machinery. Avoid introducing a second token or timeout system.
- [ ] Check cancellation during traversal, between source reads, and inside expensive verification loops.
- [ ] Preserve the distinction between cancelled work, provider failures, and empty evidence.
- [ ] Add a deterministic test that starts the worker, confirms scanning has begun, then cancels it. Add a separate deadline-expiry case.

Why this helps: the importer scan passes a callback that always returns its startup cancellation state. A later cancellation can prevent a successful response while the underlying scan still continues. Live propagation saves resources and improves interruption behavior during large repository research.

Acceptance checks:

- The active scan observes cancellation after launch and releases its worker resources.
- A deadline that expires during scanning produces the established timeout outcome.
- No extra importer recovery starts after cancellation.
- Normal references and caller recovery retain their existing results and coverage labels.
- The test observes worker termination, rather than only checking that the public response says cancelled.

Owners: [LSP importer discovery](../packages/octocode-native/crates/runtime/src/tools/lsp_search/importers.rs), [execution context](../packages/octocode-native/crates/runtime/src/runtime/engine.rs), and [ordinary search cancellation](../packages/octocode-native/crates/runtime/src/tools/local_search/executor.rs).

## Phase 3: bound security-verification reads

- [ ] Replace retained leading lines and a joined full prefix with a streaming pass that tracks private-key block state and stores only the neighborhoods needed for the selected matches.
- [ ] Bound retained bytes and individual-line buffers, and check cancellation during the pass.
- [ ] Preserve the information needed to detect key bodies whose boundary lies outside the displayed snippet.
- [ ] Fail closed when verification cannot complete; return redacted placeholders and the established coverage/error information.
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

## Phase 4: reduce repeated work where measurement supports it

- [ ] Measure a full page walk for text search, file discovery, AST symbols, and topology rather than measuring the first response alone.
- [ ] Measure repeated bounded `localFetch` reads separately; selecting a few lines from a small-file source still reads the whole file on that path.
- [ ] Record traversal count, source bytes read, parse count, peak memory, cache bytes, and end-to-end latency.
- [ ] Compare cold and warm runs in persistent MCP and separate CLI processes. Process-local caches do not survive ordinary CLI invocation boundaries.
- [ ] After phase 1, evaluate bounded scan reuse independently of `noIgnore`. Keep ignore semantics about corpus selection.
- [ ] Reuse existing cache infrastructure where it fits, with an aggregate byte budget, expiry, eviction, and query/source/policy identity.
- [ ] Preserve actionable restart behavior when a snapshot becomes stale or unavailable.
- [ ] Compare topology's ordinary scan path with the existing `graph ingest` / `graph query` workflow before adding another persistent index.

Why this helps: page sizes primarily control visible output. Ordinary search pages can rescan, non-path discovery sorts rank a collected walk, and topology analyses can rebuild the graph. Reuse can lower complete-task cost, but broader caches introduce memory, invalidation, and stale-evidence risks. Measurements determine which changes justify that cost.

Proposed experiment gate: freeze representative corpora, query cases, and thresholds before implementation. Include small repositories, a large monorepo, ignored/build directories, late-file hits, many result pages, source edits, and policy changes. Use release builds for performance comparisons and keep correctness cases separate from timing cases.

Require at least a 20% improvement in median complete-walk latency for the selected repeated-work case. Limit p95 latency regression to 5%. Keep retained memory within the frozen budget. Require zero evidence or coverage regressions. Treat these thresholds as a proposal; the audit does not establish measured gains. Freeze the thresholds before collecting results. Repeat runs and report variation; treat noisy or inconclusive measurements as insufficient evidence to expand the change.

Frozen before collection (2026-10-02 00:20): case = complete localSearch page walk (`sort:"path"`, `pageSize:20`, `maxMatchesPerFile:1000`, page 1 → last via the page-1 snapshot) in one process, release profile, frozen-scan reuse (`noIgnore:true`, the existing manifest) vs per-page rescan (`noIgnore:false`) on corpora whose ignore rules hide nothing present (walked evidence must be identical), 7 alternating repetitions per mode. Gate: reuse median ≥20% faster, p95 ≤5% worse, manifest within its 1 MiB/64-entry budget, identical evidence. Pass → evaluate extending bounded reuse beyond `noIgnore`; fail or noise → no cache change.

Owners: [local search](../packages/octocode-native/crates/runtime/src/tools/local_search/), [local fetch](../packages/octocode-native/crates/runtime/src/tools/local_fetch/), [file discovery](../packages/octocode-native/crates/runtime/src/tools/structure_search/files.rs), [topology collection](../packages/octocode-native/crates/runtime/src/tools/ast_graph/graph.rs), and [shared cache infrastructure](../packages/octocode-native/crates/runtime/src/cache/). Measurement workflow: [evaluation skill](../skills/octocode-eval-benchmark/SKILL.md).

## Phase 5: establish search and rewrite compatibility

- [ ] Build a shared fixture corpus that runs equivalent supported patterns through `astSearch` and `astRewrite` preview.
- [ ] Compare selected files, source spans, captures, and invalid-input outcomes before comparing replacement text.
- [ ] Cover metavariables, repeated captures, sibling sequences, relational rules, constraints, punctuation, comments, partial syntax, and supported languages.
- [ ] Separate matcher differences from differences in scope, ignore rules, limits, or policy. Compare both engines over the same candidate files and declared overlap.
- [ ] Document supported differences in authored core guidance and the owning engine docs.
- [ ] Evaluate engine consolidation only after the corpus reveals the compatibility and performance tradeoffs.

Why this helps: structural search uses Octocode's matcher, while rewrite uses embedded ast-grep. Shared grammars do not establish equal selection semantics. Compatibility tests make an agent's search-to-preview workflow more dependable without assuming that replacing either engine is the right solution.

Acceptance checks:

- Cases in the declared shared subset select equivalent source spans and captures.
- Intended differences have explicit guidance and regression cases.
- Unsupported patterns fail with actionable errors rather than misleading empty results.
- Search results alone never authorize applying a rewrite; preview retains its snapshot, hash, syntax, and postcondition guards.

Owners: [structural search engine](../packages/octocode-native/crates/engine/src/structural/mod.rs), [rewrite engine](../packages/octocode-native/crates/engine/src/structural/rewrite.rs), and [public structural search](../packages/octocode-native/crates/runtime/src/tools/ast_search/matches.rs).

## Phase 6: evaluate lock scope for rewrite previews

- [ ] Measure two independent previews over different roots and compare them with sequential execution.
- [ ] Separate pure preview analysis from interrupted-transaction recovery before changing lock placement.
- [ ] Evaluate coordination per canonical root using existing lock infrastructure.
- [ ] Preserve exclusion for identical and overlapping roots, and preserve recovery/apply ordering across processes.
- [ ] Keep the global lock if narrower coordination has no material measured benefit or weakens transaction behavior.

Why this helps: previews acquire the process-wide rewrite lock before preparation, serializing independent roots. Narrower coordination can improve throughput, but preview can recover interrupted transactions. Removing locks without separating those effects risks races during recovery or apply.

Acceptance checks:

- Independent previews overlap only where the implementation establishes that their roots and effects are independent.
- Same-root and overlapping-root operations remain coordinated.
- Interrupted recovery, stale hashes, failed postconditions, cancellation, and rollback continue to satisfy their existing behavioral tests.
- Preview leaves an ordinary fixture unchanged; recovery effects remain documented.
- Measured contention reduction passes the frozen performance gate.

Frozen before collection (2026-10-02 00:25): one process, release profile, two `console.log($$$A)` → `console.debug($$$A)` previews over disjoint TypeScript roots, 5 repetitions: each alone, both sequentially, both concurrently under the current process lock. Gate: concurrent wall time ≥20% above the no-lock lower bound (max of the singles) with each preview ≥1 s; otherwise keep the global lock. Context: `astRewrite` is CLI-only, so the process lock can only contend within one multi-row CLI batch.

Owners: [rewrite orchestration](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/mod.rs), [root locks](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/lock.rs), and [transaction journals](../packages/octocode-native/crates/runtime/src/tools/ast_rewrite/journal.rs).

## Phase 7: verify complete workflows

- [ ] Keep field-effect inventory checks and add behavioral cases for the risky interactions: cursor/query identity, cancellation/worker launch, source changes/redaction, and selection/hash guards.
- [ ] Verify discovery → search → exact read → semantic references on a fixture with aliases and identical spellings for different bindings.
- [ ] Verify topology candidates → exact source → LSP references without treating syntactic reachability as safe-deletion proof.
- [ ] Verify structural search → rewrite preview → guarded apply on an isolated fixture whose edit is explicitly intended. Exercise stale-source and failed-postcondition cases before accepting the rewrite changes.
- [ ] Walk all result, match, source, and response-envelope continuations. Preserve explicit terminal limits and coverage gaps.
- [ ] Verify shared extraction through local reads and a fixture-backed GitHub read so changes to a local primitive do not break its remote consumer.
- [ ] Verify delegated local evidence through `clasify` without duplicating the provider-work improvements owned by its separate plan.

Why this helps: coverage labels and valid schemas prove structure, not behavior. Composition checks catch errors that single-tool unit tests miss, including wrong source identity, incorrect anchors, dropped pages, and changes to shared primitives.

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
