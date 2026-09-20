# Jev improvement plan

Status: first runtime fixes and script POCs verified on 2026-09-20; remaining work is listed below. This plan owns implementation status and acceptance criteria. The [tool guide](OCTOCODE_JEV.md) owns current usage, [JEV.md](../.octocode/JEV.md) indexes experiments, and [GOTCHAS](../.octocode/GOTCHAS.md) records operational lessons.

## Decision and boundaries

Keep one pure `jev` tool: `{reasoning,context,question}`, one typed question per query, up to five independent queries per call, model selection internal. An unread `context:{tool,query}` uses the ordinary dispatcher, canonical validation, authorization, caches, content security and cancellation. Jev must not gain a separate file reader, GitHub client or policy bypass.

Public schemas and agent guidance belong in octocode-core. CLI/MCP expose those contracts; native runtime owns context orchestration, and `tools/jev/` owns provider adaptation. Skills remain optional shortcuts. Classifications route effort; source reads and tests establish claims. Generic evidence judgment does not imply arbitrary-size ingestion, free-form summarization, unseen identifier extraction or authority to take actions.

## Implementation and POC checkpoint

**Shipped and verified:** W1 recovery receipts, A1 continuation ownership, A2 output-policy parity, W2 per-patch unit guidance, and W4/W5 independent-question/unread-query guidance. The input shape is unchanged. Failed retrieval retains the safe underlying error code and body-free recovery receipt, explicitly states that inference did not run, and has no answer or provider usage. Nested continuation rationale/debug survives; ordinary continuation behavior is preserved. Direct and hidden reads share sanitization, including configured GitHub email redaction.

The pre-cleanup integration checkpoint passed 205 core tests, lint/typecheck, 323 native library tests and 19 Jev integration tests; native binary/addon and CLI/MCP builds completed. [Rebuilt CLI/MCP smoke evidence](../.octocode/tmp/jev-integration-smoke/after-live.json) covers recovery, executable continuations and two real-provider successes. Controlled tests inspect outgoing sanitized state and zero inference on failed retrieval. This is the named verification scope, not every monorepo suite.

Initial regeneration failed on dirty-core provenance and four outdated default-field expectations. The corrected tests preserve rejection of missing operation/URI anchors while accepting intentional defaults. Final regeneration used clean core provenance and passed; the guards were not weakened. Those results describe that checkpoint, not every later working-tree revision.

[POC v1](../.octocode/jev-poc-2026-09-20/REPORT.md) preserved a failed compound-question exact-label gate; uncertain candidates were retained. [Separately frozen v2](../.octocode/jev-poc-2026-09-20/v2/REPORT.md) passed six atomic classifications, retaining four relevant candidates and excluding two irrelevant windows from routed proof. Separate verifier reads count. Generated negatives could have been excluded by ordinary path filtering. These development probes do not establish blind adoption, actual host-token savings or the effect of core instructions built after the POCs. No new Octocode-only baseline ran.

## Review disposition

“Shipped” refers to the checkpoint above. Proposed fields and admission changes below are **not current contracts**.

| ID | Priority | Disposition | Remaining work |
|---|---|---|---|
| W1 | P0 | Shipped: hidden-read recovery | Preserve failure/continuation regressions when receipts evolve. |
| W2 | P0/P1 | Shipped: PR `charLength` is explicitly per file patch | Verify multiple patch/page axes and revision drift; any aggregate cap needs a separate ordinary-tool contract decision. |
| W3 | P1/P2 | Deferred: capture reuse | Measure capture cost before prototyping stable within-call reuse. Public handles remain gated. |
| W4 | P1 | Shipped: independent questions | One question per query; repeat context. Grouping does not promise capture reuse. |
| W5 | P1 | Shipped: unread-query instructions | Keep real CLI/MCP discovery examples valid; `value` is supplied evidence, not path loading. |
| W6 | P1 | Proposed: auditable scope receipts | Define bounded deterministic completion/count metadata without claiming global completeness. |
| W7 | P1 | Partly shipped: scalar wrapping and canonical nested-query guidance | Nonempty instructions and Choice minimum-two admission require an explicit compatibility decision. |
| W8 | P1/P2 | Shipped: independent-answer/conflict guidance | Reproduce the reported live disagreement; automated consistency requires a relation contract and evaluation. |
| A1 | P0 | Shipped: nested continuation ownership | Preserve outer/nested trace separation and ordinary-tool behavior. |
| A2 | P0 | Shipped: hidden/direct redaction parity | Keep output policy before provider transmission, hashing and reuse. |
| A3 | P1/P2 | Deferred: large structured artifacts | Use bounded existing artifact adapters; improve the shared reader only against representative unmet needs. |
| A4 | P1 | Pending: failure/packing accounting | Audit known versus unknown usage on failures and cancellation; measure actionable grouping diagnostics. |
| A5 | P1 | Unverified: stale discovery/error hints | Reproduce any remaining retired hints in current CLI help/errors, then repair the owning layer. |

## Open work and acceptance

### Scope receipts and pagination — W2/W6

Keep existing `tool`, `resultHash`, `coverage`, `next` and limitations compatible. A receipt describes the requested view, not repository completeness. Add fields only where deterministic source metadata changes the next action; candidates include requested/returned ranges, resolved revision, source-reported counts and `scopeStatus:complete|partial|unknown`. An explicit evaluation discriminator is also a proposal, not a shipped field or a prerequisite for the existing recovery fix.

Never infer completion from missing `next`, hash equality, bounded coverage or confidence. Omit unknown counts instead of reporting zero. A completed comments surface says nothing about inaccessible reviews or inline threads. Exact counts already answering a question should come from ordinary tools without inference.

PR patch windows and file pages are separate axes. The review recorded 87,419 patch characters across 30 files with `charLength:4000`; that field limits each patch, not the aggregate. Prefer changed-file metadata, selected hidden patches, then deciding source at the captured revision. Pinning only the final source read does not prove the earlier mutable patch matched it.

**Acceptance:** complete/partial/empty/unknown/error views remain distinguishable; all valid continuation axes execute under normal policy; metadata has a bounded budget; oversized metadata produces an explicit limitation rather than dropped recovery. Cover selected/all/default patch modes, binary/unavailable patches, multiple active axes, secret redaction, real interface response caps and PR-head changes. Any aggregate cap belongs in the ordinary history tool and must preserve executable recovery. Do not silently redefine `charLength`, crawl pages or average page judgments.

### Admission and semantic agreement — W7/W8

Current shape compatibility remains unchanged. A future stricter contract could reject null/blank/empty instructions and require at least two Choice alternatives. This is an Octocode product decision: upstream allows null instruction entries. Preserve useful structured instructions and criteria; avoid keyword bans. Publish migration notes and regenerate contracts if tightening is approved.

Current guidance should use strings, objects or arrays for supplied state, wrapping scalars explicitly, for example `context:{value:{observed:false}}`. Do not silently transform caller values. Keep nested queries validated by their canonical tool schema rather than embedding every read-tool schema in Jev.

The newer review reports a **live semantic disagreement**: 496 input/79 output tokens, with Score assigning 0.96 probability to “no implementation evidence” while Noul/Choice and the function indicated otherwise. Its raw receipt was not located and the claim is not independently reproduced. A different, older Score contradiction was an intentionally malformed mock response whose live smoke passed. The mock result does not dismiss the newer report.

**Acceptance:** freeze a reproducible live case and source oracle before changing semantic behavior. Mathematical validation of labels, distributions, legends and Score expectation cannot establish agreement between differently worded questions. Preserve independent answers; resolve consequential conflicts through evidence/tests. Automated consistency needs explicitly declared relationships and held-out benefit. Do not silently vote, average, rewrite answers or add paid judge calls.

### Capture reuse and accounting — W3/A4

Measure capture, cache, sanitization and inference costs separately. Identical captured state can already share inference; repeated nested reads still execute independently. A matching request JSON or result hash is not a freshness proof or retrieval capability. The GitHub cache's default 32 MiB limit is a memory budget; disk retention is bounded by entry count, not that aggregate byte ceiling (defaults: 1,000 entries, 300-second TTL).

A within-call prototype must key eligibility on canonical selection, established source identity, effective availability/path policy, redaction policy, endpoint, credential/session partition and all output-affecting options. Preserve row-specific trace metadata, bound retained bytes, and release on completion/cancellation. Pinned immutable content is a candidate; local size/mtime, moving branches, live searches and LSP state are not immutable identities. Preserve the moving-branch regression unless an explicit snapshot contract is designed. Do not strip freshness metadata merely to force grouping.

Public context handles remain gated on demonstrated cross-call benefit and a lifecycle design: ownership, expiry/eviction, memory limits, policy revalidation, credential/endpoint partitioning, source-version checks, invalidation, restart and stale-handle errors. Keep content reuse separate from judgment caching. A [live design Choice](../.octocode/tmp/jev-improvement-plan/reuse-design.output.json) favored bounded internal reuse; it is decision input, not proof of safety or performance.

**Acceptance:** unchanged or better task quality and coverage, measured capture reduction, no mutable-state or policy cross-contamination, and accurate per-group usage. Audit failed-only/mixed groups, packing fallback, cancellation and duplicate accounting; distinguish unknown usage from zero. Existing completed-usage behavior is not a reason to claim every failure-accounting case is covered. Discard reuse if overhead or changed semantics outweigh its benefit.

### Artifact workflows and shared acquisition — A3

Use the [tool-by-tool guide](JEV_TOOL_RESEARCH_GUIDE.md) for supported read families; do not duplicate its matrix here. For RFCs, saved articles and browser artifacts, expose a small visible manifest, classify bounded unread sections, then read selected proof. Supplied alternatives can use `value`. Jev can select sections or assess a supplied summary; its primitives do not generate free-form summaries.

Use the existing [HAR capture guide](../skills/octocode-chrome-devtools/references/har-capture.md) and [ingestion bridge](../skills/octocode-chrome-devtools/scripts/har-ingest-to-scrape.mjs) to produce bounded records or selected response bodies. Skills produce artifacts; the pure CLI/MCP tool judges them through ordinary `localFetch`. Do not create a Jev-only acquisition path.

Current `localFetch` has distinct limits: 10 MiB source acquisition, 100 KiB raw full-content preflight and 50,000-byte complete-view cap. It acquires source before extraction; selecting a range does not bypass the source ceiling. Huge single-line HAR files can exceed scanner budgets. Use existing bounded artifact slicing first; shared streaming/structured acquisition needs evidence that those facilities are insufficient.

**Acceptance:** representative public/synthetic RFC and HAR tasks complete with cited proof; supported size/format limits are explicit; UTF-8, record boundaries, cancellation and applicable full scanning are preserved. Verify redaction of Authorization/Cookie/Set-Cookie, URLs/query parameters, post bodies, malformed/base64 payloads and split secrets using synthetic fixtures. A locally readable session file is not automatically safe to transmit.

## Ownership and delivery gates

| Area | Owning source |
|---|---|
| Input descriptions and validation | [Core Jev schema](../../octocode-mcp-host/packages/octocode-core/src/toolContract/validation/jev.ts) |
| Agent guidance and descriptions | [Core instructions](../../octocode-mcp-host/packages/octocode-core/src/toolContract/instructions.ts), [tool descriptions](../../octocode-mcp-host/packages/octocode-core/src/toolContract/descriptions.ts) |
| Output contract | [Core output schemas](../../octocode-mcp-host/packages/octocode-core/src/toolContract/outputSchemas.ts) |
| Capture, failure receipts and grouping | [Context execution](../packages/octocode-native/crates/runtime/src/runtime/jev_context.rs), [batch execution](../packages/octocode-native/crates/runtime/src/runtime/jev_batch.rs) |
| Shared output policy and continuation ownership | [Response finalization](../packages/octocode-native/crates/runtime/src/runtime/response.rs) |
| Ordinary retrieval and cache | [Dispatcher](../packages/octocode-native/crates/runtime/src/runtime/domain_dispatch.rs), [GitHub content provider](../packages/octocode-native/crates/runtime/src/providers/github/content.rs) |

Implement scoped contracts/recovery before measured reuse. Keep larger acquisition, handles and consistency extensions separately gated. For each production slice: change owning sources, regenerate contracts, rebuild native and interfaces, inspect real CLI schema/help and stdio MCP discovery, then exercise changed behavior. Do not hand-edit generated contracts or weaken provenance, security, lint or coverage guards. Fingerprint isolated executables before paid evaluations to avoid shared-build races.

## Measurement and completion

Keep the existing Octocode-only baseline. Do not launch another without a new user instruction. Candidate-only probes establish mechanics and routing; unmatched historical runs cannot establish causal savings.

Freeze natural tasks and source oracles covering optional large files, all-relevant controls, cheap exact lookups, repeated questions, mutable sources, recoverable/terminal failures, PR paging axes, RFC/HAR triage, supplied alternatives and conflicting judgments. Reserve unseen artifacts for adoption/quality validation. Keep failures and setup costs; never add a call quota or benchmark-only Jev recipe.

Measure actual host input/cached/output tokens separately from tool-wire proxies and provider usage. Include discovery, preparation, retries, hidden captures, proof reads and verification. Track task quality, citation fidelity, false exclusions, changed next actions, known/unknown cost and recovery. Reading every candidate after screening is not a read-saving success. Reasoning benefit is a separate outcome. Compare equivalent complete workflows against targeted ordinary tools, not only read-all.

Mocks establish contract, policy and scheduling properties; real provider/agent trials establish semantic quality and adoption. The plan is complete when each review item has a verified disposition and equivalent-workflow evidence supports every claimed benefit. The first shipped slice does not imply universal consistency, arbitrary-size ingestion or public context handles are complete.
