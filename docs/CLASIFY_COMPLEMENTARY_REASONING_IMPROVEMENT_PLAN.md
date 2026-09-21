# `clasify` complementary-reasoning improvement plan

Status: proposed follow-up plan  
Checked: 2026-09-21T15:51:54Z  
Primary migration: [`CLASIFY_RENAME_IMPLEMENTATION_PLAN.md`](CLASIFY_RENAME_IMPLEMENTATION_PLAN.md)

## Plan Context

- Goal: Ship `clasify` as a complementary typed-reasoning tool with deterministic proof, policy, and effect boundaries.
- Scope: Complete the hard cutover; correct instructions, descriptions, schema semantics, concurrency claims, and evaluation gates; defer public payload redesign.
- Constraints: Core owns public contracts, native owns execution, no compatibility alias, and no unmeasured performance claim.

### Goal

Ship `clasify` as a **complementary typed-reasoning tool**. It supplies bounded Noul, Choice, or Score judgments where semantic interpretation can change the next action, while code, ordinary tools, exact source, tests, and deterministic policy retain control of workflow, proof, permissions, and effects.

It is not a replacement for agent reasoning, source inspection, deterministic checks, or authorization:

```text
explicit state or unread bounded evidence
  → independent typed judgments
  → code or agent chooses the next bounded action
  → exact evidence or tests verify consequential claims
```

### Relationship to the rename plan

The linked rename plan owns the hard public cutover from `semanticAssess` to the intentionally spelled `clasify`. This plan does not reopen that owner decision or add an alias. It adds evidence-backed corrections for runtime consistency, instruction placement, descriptions, schema semantics, truthful concurrency, and evaluation.

### Scope

Included:

- Complete the native/interface cutover already defined by the rename plan.
- Make `clasify` conditional rather than a mandatory first step.
- Restore enabled-tool-aware MCP instructions while keeping a stable shared prefix.
- Keep selection rules in descriptions and exact mechanics in schemas.
- Clarify logical capture, paging, coverage, continuations, and usage ownership.
- Implement or remove the unproven five-request concurrency claim.
- Add held-out routing and equivalent-workflow evaluation gates.
- Evaluate resource defaults, question fan-out, and usage receipts only after cutover.

Excluded:

- No `semanticAssess` compatibility alias.
- No configuration, environment-variable, or provider-key rename.
- No permission grant based on a probabilistic answer.
- No productionization of `packages/octocode-jev-lab`; it remains a direct provider probe.
- No cross-call judgment cache or public output-shape redesign in the rename release.
- No claim of total token savings until an equivalent-workflow evaluation proves it.

## Checked current state

Every recommendation below traces to current source or an executed check. “Confirmed” applies only to the named revision and scope.

| ID | Checked statement | Evidence | Result and implication |
|---|---|---|---|
| C1 | Canonical core exposes only `clasify`; `semanticAssess` and `jev` are rejected. | Sibling `packages/octocode-core/src/toolContract/names.ts:1-14`; `src/__tests__/clasify.test.ts:26-39`. | Confirmed. Native/interfaces must complete the same hard cutover; no alias is planned. |
| C2 | Core commit `a31ac0fd7c02e4a717e85343d4c5e595cfcf8339` is clean and verified. | `git rev-parse HEAD`; empty `git status --porcelain`; core lint/typecheck; test result: 22 files and 237 tests passed. | Confirmed. It is a valid generation source, subject to approved follow-up contract edits. |
| C3 | Generated native contracts reference clean core revision `a31ac0f…` and fingerprint `8a0b225c…`. | `packages/octocode-native/crates/runtime/src/contracts/generated/contract-provenance.json:1-6`; `yarn workspace @octocodeai/octocode-native contracts:check` returned OK. | Confirmed. Generated artifacts are ahead of runtime dispatch and cannot ship alone. |
| C4 | The built runtime still has fingerprint `81379c2f…`. | `node packages/octocode/out/octocode.js scheme clasify --view query --compact` exited 5 with drift against core `8a0b225c…`. | Confirmed. Drift bypass is not a release fix. |
| C5 | Native executable source still identifies the public tool as `semanticAssess`. | `localSearch` found it in 17 implementation/test files and found no non-generated `clasify` runtime implementation. | Confirmed. Runtime identity is the first implementation blocker. |
| C6 | `buildMcpInstructions(enabledToolNames, options)` ignores both arguments and returns one static prompt. | Sibling `toolContract/instructions.ts:21-31`; `src/__tests__/mcpInstructions.test.ts:9-17`. | Confirmed. Guidance can mention tools not exposed by the server. |
| C7 | Consumers and docs expect availability-scoped instructions. | `packages/octocode-mcp/src/native/index.ts:172`; `packages/octocode/src/cli/commands/scheme.ts:126-173`; sibling `mcp.service.ts:104`; sibling `docs/HTTP_MCP.md:105`. | Confirmed. Producer behavior has drifted from its consumers’ stated contract. |
| C8 | The system prompt says “Classify first.” | Sibling `toolContract/instructions.ts:9`. | Confirmed. It presents an optional judgment as a default prerequisite. |
| C9 | Existing whole-task evidence does not establish savings and includes regressions. | `.octocode/JEV.md:38-54`: host tokens increased 45.79% and 61.73%; 72 CLI calls versus 46; provider used 31,268 input/950 output tokens. | Confirmed for those frozen experiments only. Do not prescribe universal calls or quotas. |
| C10 | Input validation is already strict and bounded. | Sibling `validation/clasify.ts:184-295`, `limits.ts:1-12`, `clasify.test.ts:118-205`: unique IDs, 25 cells/query, 50 cells/call, five questions, 80,000 chars/resource. | Confirmed. Preserve initial shape; improve semantics and prose first. |
| C11 | A tool context is one logical capture, but native may follow multiple safe pages. | `runtime/jev_batch.rs:122-226`; current core description says “executed once” at `validation/clasify.ts:105-116`. | Confirmed. Wording must distinguish logical capture from physical reads/pages. |
| C12 | Provider assessment is serial across queries, resources, pages, and oversized fallbacks. | `runtime/jev_batch.rs:243-290` and `:293-408`; no matching concurrency test found. | Confirmed. Runtime does not currently implement its documented concurrency. |
| C13 | Native architecture claims a five-request concurrency bound. | `packages/octocode-native/ARCHITECTURE.md:40-44`. | Confirmed prose, contradicted by C12. Implement and test it or remove the claim. |
| C14 | Grouped usage is emitted on one answer while sibling answers omit it. | `runtime/jev_batch.rs:255-275`; live one-resource/three-question smoke test. | Confirmed. Missing usage means shared/unknown, not zero. |
| C15 | The Jev lab is not a public-contract implementation. | `packages/octocode-jev-lab/ARCHITECTURE.md:1-35`; direct live probe behavior. | Confirmed. Use native CLI/MCP integration for contract parity and the lab for provider behavior only. |
| C16 | TypeSafe describes System One as typed judgments inside code-owned workflows and supports independent-question fan-out. | Checked 2026-09-21: `https://docs.typesafe.ai/concepts/system-one.md`, `/how-to-build-with-system-one.md`, `/patterns/fan-out.md`. | Confirmed for current public docs. This supports complementary reasoning, not automatic proof or policy authority. |

### Immediate blocker

```text
core catalog:       clasify / next.clasify / fingerprint 8a0b…
native dispatch:    semanticAssess in active branches
built runtime:      old fingerprint 8137…
```

This is an in-progress checkpoint, not a runnable release state.

## Approach

Finish the public identity migration without behavioral changes, then correct canonical guidance and regenerate from a clean core revision. Verify real CLI/MCP paths before changing provider scheduling; benchmark optional context and output changes only after the coherent baseline passes.

## Settled design direction

`clasify` is a complementary reasoning primitive with explicit ownership:

| Layer | Owns | Must not own |
|---|---|---|
| Server instructions | When semantic judgment belongs in a cross-tool workflow; proof, trust, continuation, and permission boundaries. | Exact fields, limits, or mandatory call quotas. |
| Tool description | When to choose the tool, when not to, result kind, and next verification action. | Matrix mechanics or scheduler promises. |
| Input schema | Exact Noul/Choice/Score grammar, IDs, limits, matrix cross-product, supplied/read context, and continuation input. | Agent-wide policy. |
| Output schema | Page-local answer, confidence, coverage, model, usage, errors, and executable continuation semantics. | Hidden reduction or proof claims. |
| Native runtime | Admission, safe capture, paging, provider grouping, deadlines, ordering, sanitization, and response projection. | Public contract invention. |
| Calling agent/code | Decides whether a judgment changes an action and obtains proof where required. | Reinterpreting partial/error as negative or using probability as authorization. |

### Instruction text

Replace “Classify first” with one conditional rule:

> Use `clasify` only when a bounded Noul, Choice, or Score judgment over supplied state or unread candidates can change the next action. Skip it for exact lookups, deterministic checks, mandatory evidence, and settled decisions. Treat answers as probabilistic routing signals, not proof or permission. Retain uncertain, partial, and errored candidates; run query-level `next.clasify` unchanged, then verify selected claims against exact source or tests.

Reasoning:

1. “Can change the next action” makes usefulness observable rather than ceremonial.
2. Negative cases prevent screen-then-read duplication observed in C9.
3. “Not proof or permission” preserves deterministic security and effect boundaries.
4. Retention rules prevent uncertainty, partial coverage, and provider failure from becoming false exclusion.
5. Verification preserves the evidence contract.

`buildMcpInstructions` should use a stable shared prefix plus an enabled-tool overlay. `clasify` text appears only when the tool is enabled; grammar capability text appears only when supplied. This fixes C6/C7 while retaining a cache-friendly prefix.

### Tool description

Short:

> Bounded probabilistic Noul, Choice, or Score judgment over unread read-tool results or supplied work.

Full:

> Use for a bounded Noul, Choice, or Score judgment that changes which unread read-tool result to inspect, or to review supplied work. Not for exact lookup, deterministic checks, required proof, permissions, or free-form generation. Returns page-local probabilistic answers with explicit coverage; retain uncertain, partial, and errored results, run query-level `next.clasify` unchanged, then verify selected evidence. Requires a classification provider key.

This text owns selection without promising parallel execution or duplicating schema fields.

### Input-schema wording

No initial shape change is required. Update descriptions as follows:

- `context.tool`: “Read tool used to capture one logical resource under ordinary security and output policy. The runtime may follow safe same-resource continuations; assessed bodies remain hidden from the caller.”
- `context.query`: “One unexecuted bounded query using the selected read tool’s current canonical fields. Validate it against that tool’s live query schema before calling `clasify`.”
- context union: “Supply non-empty state already held by the caller, or one unread bounded read-tool query. Supplied values trigger no read; tool contexts retain ordinary authorization, redaction, pagination, and security policy.”
- `resources`: “Each resource is captured once logically, may contain ordered source pages, and is reused across every applicable question. Use one candidate/result per resource for comparison.”
- `questions`: “Every question applies independently to every resource page. Split the matrix when a question does not apply to all resources; issue dependent judgments only after prerequisites resolve. Provider calls may split by request-size headroom.”
- `maxChars`: “Maximum sanitized evidence characters captured for this logical resource; this is not a token budget. Low values can produce partial coverage without an executable continuation.”

Do not embed every downstream read-tool schema inside `ClasifyInputSchema`: native already validates nested queries against canonical contracts, and embedding them would duplicate large schemas in the tool definition.

### Output-schema wording

- Coverage: “Retrieval coverage for this resource-question cell, not confidence or correctness. `complete` means bounded source pages were assessed; `partial` and `error` are not negative judgments.”
- Pages: “One independent judgment per assessed source page. Review all pages together, but do not average, vote, or infer a global negative from page-local answers.”
- Usage: “Provider usage attributed once per provider call. Grouped sibling answers may omit usage; absence means shared or unavailable, never zero.”
- Nested source receipt: “Body-free source continuation metadata for diagnostics. Execute only the query-level `next.clasify` continuation.”

A future `providerCalls[]` plus `inferenceId` output may improve usage ownership, but that is a separate public-contract decision after cutover.

## Acceptance Contract

| ID | Requirement | Pass/fail acceptance | Rollback/stop trigger |
|---|---|---|---|
| A1 | One executable public identity | Core, generated catalog, native `ToolId`, CLI, MCP, errors, stats, skills, and continuations use only `clasify`; direct Noul/Choice/Score and `next.clasify` pass without drift bypass. | Any advertised tool cannot dispatch, or any active `next.assess`/`semanticAssess` remains. |
| A2 | Complementary selection | Held-out exact lookup, deterministic test, mandatory proof, permission, and settled-decision cases make zero `clasify` calls; positive bounded-judgment cases select it. | Unnecessary calls increase or a permission decision depends on an answer. |
| A3 | Availability-aware instructions | GitHub-only instructions contain no local, mutation, LSP, or `clasify` guidance; full instructions contain `clasify` guidance exactly once; shared prefix stays byte-stable. | Instructions mention unavailable tools or diverge between CLI/MCP for the same enabled set. |
| A4 | Contract semantics | Tests establish logical capture wording, page-local coverage, non-negative partial/error, executable query-level continuation, and shared usage semantics. | Prose or fixtures imply one physical read, hidden reduction, proof, or zero usage. |
| A5 | Truthful concurrency | Either measured in-flight provider requests never exceed five and independent delayed groups overlap, or architecture prose states execution is serial. Output order remains deterministic. | Unbounded requests, reordered output, erased siblings, or prose ahead of implementation. |
| A6 | Equivalent-workflow quality | Against a frozen baseline, correctness/citation fidelity do not regress; critical false exclusions are zero; host/provider tokens, calls, proof reads, retries, and latency are reported separately. | Any critical false exclusion/security regression, or higher total work without a predeclared quality benefit. |
| A7 | Security boundary | Read policy runs before provider inference; no source body appears in public output; probabilistic output never authorizes mutation. | Secret/path-policy regression, body leak, or answer-controlled authorization. |

## Execution Questions

| ID | Question | Resolution/deferral | Evidence | Status |
|---|---|---|---|---|
| Q1 | Is the public spelling still open? | No. Owner requested intentional `clasify`; this plan evaluates behavior but does not create an alias. | Rename plan and current core. | Resolved |
| Q2 | Should contract prose be improved during cutover? | Yes, as a separate core commit before final regeneration; no input/output shape change. | C6-C11 and A2-A4. | Resolved |
| Q3 | Should concurrency be mixed into the identity rename? | No. First prove identity parity, then change scheduling in an isolated native commit. | C12/C13; smaller rollback radius. | Resolved |
| Q4 | Should the 80,000-character maximum/default change now? | Deferred to equivalent-workflow evaluation; separate maximum from default only if measured. | C9/C10. Owner: contract/runtime maintainers. Trigger: A1-A7 green. | Deferred |
| Q5 | Should five questions become 25? | Deferred. Test 1×25 against 5×5 after concurrency and usage accounting are trustworthy. | Current cell cap permits the shape in principle, but no local benchmark proves benefit. | Deferred |
| Q6 | Should usage move to explicit provider-call receipts? | Deferred as a public output redesign. Document current ownership first. | C14. Trigger: post-cutover RFC and compatibility review. | Deferred |
| Q7 | Should `clasify` choose or authorize arbitrary tools? | No. It may rank bounded candidates; deterministic code/agent chooses and policy authorizes. | Goal, C9/C16, A2/A7. | Resolved |

## Steps

### Phase 1 — Coherent identity

- [ ] S1. **Native hard cutover** — Depends on: C1-C5 — Produces: native source whose public identity is only `clasify`, while accurate private provider terms remain — Acceptance: A1/A7 — Verify: Rust unit/integration tests and source search — Owners: `packages/octocode-native/crates/runtime/src/{tools,runtime,cli,contracts}`.
- [ ] S2. **Native source verification** — Depends on: S1 — Produces: verified identity baseline against the current generated catalog — Acceptance: A1; old name fails and all answer kinds plus one continuation pass — Verify: `yarn workspace @octocodeai/octocode-native test:rust` plus focused direct calls.

### Phase 2 — Canonical behavioral contract

- [ ] S3. **Availability-aware instructions** — Depends on: S2 — Produces: stable base guidance plus enabled-tool and grammar overlays, with conditional `clasify` use — Acceptance: A2/A3 — Verify: core MCP-instruction tests and remote/full catalog snapshots — Owner: sibling core `toolContract/instructions.ts`.
- [ ] S4. **Description and schema prose** — Depends on: S3 — Produces: selection, capture, coverage, usage, continuation, and bounded-example wording without a payload-shape change — Acceptance: A2/A4/A7 — Verify: `clasify`, presentation, routing, schema-audit, and native-contract tests — Owners: sibling core `descriptions.ts`, `validation/clasify.ts`, `outputSchemas.ts`, `discovery/toolCommandPatternQueries.ts`.
- [ ] S5. **Commit and regenerate from clean core** — Depends on: S4 — Produces: clean committed core revision and regenerated native contracts with matching provenance/fingerprint — Acceptance: A1/A3/A4 — Verify: core lint/typecheck/tests, `yarn contracts:regen`, and `yarn workspace @octocodeai/octocode-native contracts:check`.

### Phase 3 — Consumers and end-to-end release path

- [ ] S6. **Interface/skill/document cutover** — Depends on: S5 — Produces: active MCP, CLI, skill, helper, benchmark, and documentation surfaces using `clasify`, with frozen history preserved — Acceptance: A1/A2 — Verify: exact source search plus package tests.
- [ ] S7. **Rebuild and real-path verification** — Depends on: S6 — Produces: compatible built native, CLI, MCP, and dependent Pi/Awareness artifacts — Acceptance: A1/A3/A7 — Verify: `config`, `scheme`, direct CLI, stdio MCP, and skill-path checks in the Test and Verification Plan.

### Phase 4 — Truthful provider concurrency

- [ ] S8. **Indexed provider work units** — Depends on: S7 — Produces: indexed query/resource/page provider work after unchanged sequential capture — Acceptance: A5/A7 — Verify: capture and snapshot tests retain existing behavior.
- [ ] S9. **One global five-request semaphore** — Depends on: S8 — Produces: bounded provider scheduling with shared deadline/cancellation, stable reconstruction, and preserved sibling outcomes — Acceptance: A5 — Verify: delayed-wire tests with an atomic max-in-flight counter, grouped and singleton fallback requests.
- [ ] S10. **Reconcile architecture prose** — Depends on: S9 — Produces: architecture text that states only measured scheduler behavior — Acceptance: A5 — Verify: review prose against the implementation and its in-flight-bound test anchor.

### Phase 5 — Evaluation and optional measured changes

- [ ] S11. **Held-out selection evaluation** — Depends on: S7 — Produces: frozen positive/negative results for the old baseline, old guidance, conditional guidance, and optional correctly spelled diagnostic control — Acceptance: A2/A6 — Verify: independent scoring of tool choice and unnecessary calls against the frozen case manifest.
- [ ] S12. **Equivalent-workflow benchmark** — Depends on: S9/S11 — Produces: separate correctness, false-exclusion, host/provider token, read, call, retry, latency, and monetary-cost results — Acceptance: A6/A7; no discard solely from partial/error/uncertain output — Verify: benchmark manifest, receipts, and keep/discard report.
- [ ] S13. **Evaluate, do not assume, context changes** — Depends on: S12 — Produces: comparison of 10K/20K/40K/80K defaults and 1×25 versus 5×5 question shapes — Acceptance: A6; adopt only a measured improvement and retain the 80K maximum absent separate evidence — Verify: equivalent frozen tasks and full workflow accounting.
- [ ] S14. **Separate usage-receipt RFC if needed** — Depends on: S12 — Produces: an explicit keep/defer decision or separate RFC for `providerCalls[]` plus `inferenceId` — Acceptance: compatibility, rollout, and rollback contract; not part of this plan’s initial release — Verify: public-schema review and consumer inventory.

## Files, APIs, and Contracts

| Surface | Planned change | Compatibility |
|---|---|---|
| Core names/schema catalog | Already hard-renamed; improve prose/instructions and regenerate. | Intentional hard break; no alias. |
| `instructions.ts` | Stable base plus enabled overlays; conditional `clasify`. | Same API signature, corrected argument behavior; snapshots change. |
| `descriptions.ts` | Selection-focused text. | Presentation-only contract change. |
| `validation/clasify.ts` | Description corrections first; limits unchanged. | Shape-compatible. |
| `outputSchemas.ts` | Clarify coverage, pages, usage, receipts. | Shape-compatible initially. |
| Native tool/runtime branches | Public identity and continuation rename. | Hard break matching core. |
| Native provider scheduler | Serial to bounded five-way concurrency after cutover. | Intended output order/shape unchanged; timing and failure interleaving change. |
| CLI/MCP/Pi/skills | Consume the same core identity and instructions. | Must release atomically with matching native/core artifacts. |
| Jev lab | Documentation warning only if needed. | Remains provider-only and cannot report production contract metrics. |

## Test and Verification Plan

| Type | Scope | Command/check |
|---|---|---|
| Core deterministic | Schemas, names, descriptions, routing, instructions, native contract generation | In sibling repo: `yarn workspace @octocodeai/octocode-core lint`, `typecheck`, `test` |
| Contract parity | Clean provenance and matching fingerprints | `yarn contracts:regen`; `yarn workspace @octocodeai/octocode-native contracts:check` |
| Native unit/integration | Dispatch, nested validation, paging, continuation, errors, usage, ordering, cancellation | `yarn workspace @octocodeai/octocode-native test:rust` |
| Native build | Real addon, not source-only tests | `yarn workspace @octocodeai/octocode-native build:dev` |
| CLI | Availability, scheme, direct call, exact unknown-old-name failure | `yarn workspace octocode build:dev`; `$OCTO config --json`; `$OCTO scheme`; `$OCTO scheme clasify --compact`; direct fixtures |
| MCP | Advertised catalog, enabled instructions, direct result, continuation | Build `octocode-mcp`; stdio client integration tests |
| Concurrency | Bound, overlap, deterministic order, deadline, sibling failure isolation | Delayed Wiremock fixture and atomic in-flight count; do not infer from wall time alone |
| Security | Safe path, redaction, source-body non-disclosure, no answer-authorized mutation | Existing security tests plus malicious/nested-query fixtures |
| Behavioral evaluation | Selection, correctness, false exclusion, total workflow cost | Frozen held-out benchmark with predeclared keep/discard gate |

Required live classifications after rebuild:

1. Noul over one supplied resource.
2. Choice over two unread candidates with one shared question.
3. Score over one bounded unread resource.
4. Multi-page resource followed by unchanged `next.clasify`.
5. One source failure that preserves sibling results.
6. An exact lookup where the agent correctly skips `clasify`.
7. A mutation request where deterministic policy, not the answer, controls permission.

## Risk Mitigations

| Risk | Reasoning | Prevention/detection |
|---|---|---|
| Mixed core/native release | C3-C5 already demonstrate the state. | Atomic compatible package set, fingerprint gate, real CLI/MCP smoke test; never publish generated contracts alone. |
| Overuse adds cost without changing behavior | C9 shows screening followed by proof reads can increase work. | Conditional instruction; zero-call negative cases; equivalent-workflow gate. |
| False exclusion | Page-local uncertainty can be mistaken for global absence. | Retain partial/error/uncertain candidates; inspect all pages; critical false-exclusion guardrail of zero. |
| Semantic authority creep | A confident model answer can be mistaken for proof or permission. | Repeat boundary in instructions, description, output prose, and security tests; code/policy owns effects. |
| Prompt drift across surfaces | Core, host, CLI, MCP, Pi, and copied skills consume related text. | One core owner for contracts; source search and interface snapshots after regeneration. |
| Concurrency races | Serial-to-parallel changes ordering, cancellation, and failure timing. | Indexed work, one semaphore, deterministic reconstruction, shared deadline, dedicated tests. |
| Miscounted usage | Current usage appears on one grouped answer only. | Document once-per-provider-call semantics; aggregate by observed receipts, never missing-as-zero. |
| Lower context default loses evidence | Smaller capture can reduce cost while increasing partial coverage. | Evaluate defaults against false exclusions and proof rereads; preserve executable continuations. |
| Name spelling harms tool selection | `clasify` is intentional but nonstandard. | Diagnostic held-out name control before release; report result even if product spelling remains fixed. |
| Lab metrics mislabeled as production | The lab skips native policy, acquisition, paging, validation, and projection. | Label lab results provider-only; use real CLI/MCP for production claims. |

## Rollout, Migration, and Rollback

1. Land native identity changes without publishing.
2. Land core instruction/description corrections and a clean core commit.
3. Regenerate once from that exact revision and record provenance.
4. Rebuild native, CLI, MCP, and dependent Pi/Awareness surfaces.
5. Run deterministic suites and real-path classifications.
6. Run held-out selection evaluation before publishing the hard cutover.
7. Publish only a compatible package set whose fingerprints match.
8. Add concurrency in a separate change after identity release is green, or remove the architecture claim.
9. Evaluate context/default/output redesigns independently.

Rollback triggers are A1-A7 failures. Before public release, revert the failing slice and regenerate from the last coherent core revision. After a hard-cutover release, rollback the compatible package set together; do not restore an undocumented alias in only one layer. Provider concurrency is independently reversible to serial execution because its initial contract requires unchanged order and shape.

## Statement-checking rules for implementation reviews

Every implementation PR under this plan must classify consequential prose as one of:

- **Fact:** cite current source/test/command and revision.
- **Measured result:** cite frozen inputs, full workflow accounting, and date.
- **Design decision:** cite owner/accepted plan and affected boundary.
- **Hypothesis:** state the deciding benchmark and reversal condition.

Reviewers must reject:

- “faster,” “cheaper,” or “smarter” without an equivalent baseline;
- provider-only token counts presented as total agent cost;
- architecture concurrency claims without an in-flight-bound test;
- `complete` coverage presented as correctness;
- confidence presented as permission or proof;
- lab behavior presented as native contract behavior;
- source-level success presented as a live CLI/MCP success;
- global absence inferred from partial coverage.

## Why this architecture fits the product thesis

The checked evidence supports a narrow but useful thesis:

1. Typed semantic judgment is composable because Noul, Choice, and Score have explicit criteria and machine-readable outputs.
2. It is complementary because deterministic control still selects effects, validates contracts, and obtains proof.
3. It can reduce unnecessary host reads when a judgment actually excludes work, but C9 shows that savings are not automatic.
4. Independent fan-out can reduce latency after S9, but C12 means that benefit is currently hypothetical in production.
5. Dynamic context is safer when the read query, byte/character bound, source receipt, coverage, and continuation are explicit.
6. Security-aware reasoning means classifying risk or relevance while deterministic policy retains authorization; it does not mean sending sensitive data to a model to ask whether transmission is safe.
7. The differentiated control plane is therefore explicit state + bounded context capture + typed judgment + provenance + deterministic effects, not another unconstrained agent loop.

## Completion definition

This plan is complete when A1-A7 pass, the hard cutover is executable through real CLI and MCP paths, the instruction/description/schema ownership is coherent, concurrency prose matches tested behavior, and every optional cost/context change is either accepted by its benchmark or explicitly rejected/deferred with retained evidence.

