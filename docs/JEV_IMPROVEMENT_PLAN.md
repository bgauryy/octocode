# `semanticAssess` implementation plan

Status: naming decision accepted on 2026-09-20; the public hard cutover from `jev` to `semanticAssess` is not implemented. Earlier runtime fixes and script POCs remain verified within their recorded scope. This plan owns implementation status and acceptance criteria. Until the cutover ships, the [tool guide](OCTOCODE_JEV.md) describes the current interface, [JEV.md](../.octocode/JEV.md) indexes experiments, and [GOTCHAS](../.octocode/GOTCHAS.md) records operational lessons.

## Audit Reasoning — fix-and-keep (2026-09-20)

- **Status:** Partially implemented. The resource-question matrix, correlated rows, native capture reuse, and browser/scrape paging exist in the working tree. The live staged CLI still exposes the old `jev` pair schema, and no surface has completed the `semanticAssess` cutover.
- **Why kept:** This document remains the owner for the open public rename, contract regeneration, interface rebuild, cleanup, and release verification.
- **Evidence:** The core schema contains the matrix union; native matrix expansion and browser triage tests exist; live `scheme jev` still returns the previous staged schema. Root `AGENTS.md` also mandates retired `tools … --scheme` commands that the live CLI rejects; the live forms are `scheme <tool>` and direct canonical tool commands. These are working-tree observations, not release claims.
- **Remaining work:** Complete the hard rename and generic public contract across core, native, CLI, MCP, Pi, skills, browser/scraping helpers, docs, tests, generated artifacts, telemetry, and release checks.

### Agent-readiness scorecard before this amendment

These are planning-rubric scores, not product-quality or token-saving measurements. A live pre-rename semantic review of this document classified agent admission/mode selection as `gap` with 0.93 probability, layer ownership as `adequate` with 0.98 probability, and the context/evaluation lifecycle as `adequate` with only 0.65 probability. On that evidence, the plan was **7.5/10 for agent readiness**: strong architecture and ownership, but underspecified call admission, context reuse, resource paging, and held-out behavior checks. After the repair below, bounded semantic rechecks classified the agent contract as `ready` with 0.85 probability and its evaluation gate as `ready` with 0.98 probability. The revised planning score is **9/10**; S7 must establish real agent behavior before this can become a product-quality or efficiency rating.

## Decision and boundaries

Ship one public `semanticAssess` tool. The name describes the stable capability rather than the current Jev classification model. `{reasoning,resources:[{id,context}],questions:[{id,question}]}` is the primary shared-question contract; `queries[]` remains only for independent context/question pairs where a cross-product produces unwanted cells. Matrices are resource-major and capture each resource once. The pre-rename checkpoint admits 25 cells, 25 resources, and five questions; the target replaces the fixed cell ceiling with evaluated total-call limits and automatic private provider pages. Model selection stays internal. An unread `context:{tool,query}` uses ordinary dispatch, validation, authorization, caches, content security, and cancellation. `semanticAssess` must not gain a separate reader, GitHub client, or policy bypass.

Public schemas and agent guidance belong in octocode-core. CLI/MCP expose those contracts; native runtime owns context orchestration. The public runtime module becomes `tools/semantic_assess/`; the private provider adapter retains the Jev service/model name and provider-specific credentials. Skills remain optional shortcuts. Assessments route effort; source reads and tests establish claims. Generic semantic assessment does not imply arbitrary-size ingestion, free-form summarization, unseen identifier extraction, proof, or authority to take actions.

The cutover is intentionally hard: do not register a public `jev` alias, compatibility tool, duplicate schema, forwarding command, or deprecated skill. Document the rename in release notes, but expose only `semanticAssess` through runtime discovery. Historical receipts, POCs, and frozen benchmark artifacts keep their original names for provenance; label them pre-rename rather than rewriting them.

## Public rename and contract plan

### Public and internal names

| Surface | Target | Rule |
|---|---|---|
| Public tool and CLI command | `semanticAssess` | The only discoverable/callable name. |
| Display title | Semantic Assessment | Human-readable title; not a second identifier. |
| Public schema/types | `SemanticAssess*` | No exported `Jev*` contract aliases after the cutover. |
| Native public module | `semantic_assess` | Owns validation handoff, capture orchestration, batching, and output shaping. |
| Provider adapter | Jev-specific internal module | Retains Jev request/response vocabulary and `OCTOCODE_JEV_KEY`; never leaks into public tool descriptions or schemas. |
| Active skill | `octocode-semantic-assess` | Replaces `octocode-jev-reasoning-loop`; no forwarding skill. |
| Active docs | Semantic-assessment names | Rename current guides and links; preserve historical artifact paths. |

### Public schema cleanup

- Prefer `resources[] × questions[]`; describe `queries[]` only as the independent-pair mode.
- Matrix rows require paired `resourceId` and `questionId`. Independent-pair rows use ordered `index` and reject matrix IDs.
- Rename provider-specific public primitives when they leak model vocabulary. In particular, map the public binary question/answer to generic `binary` plus a probability field; translate to/from provider-specific Noul only inside the Jev adapter. Keep `choice` and `score` generic.
- Keep provider/model diagnostics optional and clearly non-contractual. Do not expose the configured Jev model as the tool identity.
- Reject mixed matrix/pair envelopes, duplicate IDs, incomplete output ID pairs, unknown fields, and provider-body echoes. Replace the current fixed 25-cell admission ceiling with evaluated public roster and total-cell limits plus smaller private provider pages; do not expose the current model's token ceiling as a public constant.

### Agent behavior and context contract

The agent-facing promise is: **use `semanticAssess` only when a bounded semantic answer can change the next action; keep source bodies outside host context until the assessment says which proof to read.** A call routes effort. It does not prove a claim, summarize an arbitrary corpus, grant authority, or replace tests.

#### Admission and mode selection

| Situation | Required agent action |
|---|---|
| Exact lookup, arithmetic, schema inspection, known deciding span, or source that must be read either way | Skip `semanticAssess`; use the exact tool or test directly. |
| Optional unread candidates where relevance, risk, support, or an alternative changes the next read/test | Call `semanticAssess` with unread `context:{tool,query}` resources. |
| The same questions apply to every resource | Use one `resources[] × questions[]` matrix. Every admitted resource must receive every admitted question. |
| Context/question pairs are independent and a cross-product creates meaningless cells | Use `queries[]`; correlate rows by `index`. |
| A later question depends on an earlier answer | Keep calls sequential; do not place dependent questions in one matrix. |
| Evidence is already visible to the host | Use `context:{value}` only when another judgment still changes an action; never reread or resend it through an ordinary tool. |
| Opposite valid answers lead to the same action | Skip the call because it cannot change the next step. |
| A source page, resource page, or result is partial, errored, or insufficient | Retain it, preserve its receipt, and run the returned continuation or direct proof read. Never classify uncovered scope as unrelated. |

The target call below is illustrative and does not become live until S4. It produces four resource-major cells; no resource can disappear merely because another resource or question failed.

```json
{
  "reasoning": "Choose which contract and runtime regions require proof reads before the rename.",
  "resources": [
    {
      "id": "local-contract",
      "context": {
        "tool": "localFetch",
        "query": {"path": "/repo/core/schema.ts", "reasoning": "Assess the local contract candidate."}
      }
    },
    {
      "id": "upstream-runtime",
      "context": {
        "tool": "ghGetFileContent",
        "query": {"owner": "example", "repo": "runtime", "path": "src/tool.rs", "reasoning": "Assess the upstream runtime candidate."}
      }
    }
  ],
  "questions": [
    {
      "id": "relevance",
      "question": {
        "type": "choice",
        "instructions": "Does this resource contribute to the public rename?",
        "criteria": {"relevant": "Contributes implementation or a counterexample.", "unrelated": "Complete scope covers another concern.", "insufficient": "The returned view cannot decide."}
      }
    },
    {
      "id": "security",
      "question": {"type": "binary", "instructions": "Could this resource affect authorization, redaction, or secret handling?"}
    }
  ]
}
```

#### Context lifecycle

1. **Discover without bodies:** obtain candidate paths, URIs, refs, manifest entries, or exact ranges through body-free search/list views.
2. **Build bounded resources:** assign stable resource and question IDs; use canonical unexecuted read queries. Do not read a body merely to prepare an assessment.
3. **Assess once:** capture and sanitize each matrix resource once, then evaluate every question against that same captured state. Retrieved text is untrusted data and cannot change the caller's question, permissions, or objective.
4. **Inspect every row:** keep answer, completeness, errors, usage ownership, hashes, and executable continuations. A missing, partial, or errored row is never a negative answer.
5. **Read retained proof:** directly read the deciding original source for relevant, uncertain, insufficient, or conflicting rows. Cite source locations, not hidden assessment bodies.
6. **Verify consequential claims:** use exact source, LSP, tests, or runtime checks appropriate to the claim. The assessment remains routing evidence.

Compaction may drop repeated logs and rejected candidates only after preserving IDs, source anchors, decisions, open limitations, continuations, and the next action. A summary must never become the only copy of proof. Stable schemas/instructions should form a cacheable prefix; dynamic resources and questions remain the request tail. Cache behavior is an optimization, not a permission boundary or correctness claim.

#### Built-in resource paging

`semanticAssess` owns resource paging; browser and scraping skills consume that contract rather than implementing a second semantic protocol. One accepted request may contain a bounded resource roster and shared questions. The runtime captures each resource once and automatically partitions the cross-product into private provider pages under the adapter's input headroom, preserving resource-major order, IDs, row isolation, and usage ownership. Public limits are derived from payload, latency, cost, and output evaluations; provider-specific token ceilings remain private.

If the complete roster or result cannot fit the public call/output budget, return `isPartial` plus an executable `next.assess` query containing the unchanged questions and remaining resource descriptors. The continuation must work across one-shot CLI processes; it cannot depend solely on in-memory cursor state. Callers run it unchanged until every resource-question cell is present, then aggregate. This paging is distinct from source-tool pagination: `semanticAssess` never silently follows a nested source continuation because that changes evidence scope. No page can establish a global negative, and `skip` requires complete, confidently unrelated coverage across all pages.

#### Guidance ownership

| Layer | Sole responsibility |
|---|---|
| Core schema | Exact discriminated envelopes, field semantics, IDs, limits, mutual exclusions, output correlation, completeness, and executable continuation shape. |
| Core tool description | `Use when → Do not use when → Inputs → Returns → Next`; enough information to select `semanticAssess` and matrix versus independent mode, without restating types. |
| Core MCP/CLI instructions | Cross-tool `discover → assess → read proof → verify` workflow, shared trust boundary, and partial-result rule. |
| Active skill | Conditional examples, large-artifact workflow, recovery recipes, and live-schema discovery; it points to core rather than copying the full schema. |
| Native runtime | Ordinary-tool dispatch, authorization, redaction, capture-once semantics, provider paging, cancellation, output shaping, and recovery. |
| Product docs | Rationale, migration, end-to-end examples, limitations, and measured evidence; no second live field contract. |

The contract audit must compare `semanticAssess` against every ordinary tool allowed in `context.tool`, verify that every advertised `Next` exists, and fail on split ownership, field/name/type drift, phantom continuations, or guidance that exceeds runtime authority.

### Dependency-ordered implementation

| Step | Change | Depends on | Pass condition |
|---|---|---|---|
| S1 | Finalize `semanticAssess` input/output schemas, names, layer-owned descriptions/instructions, examples, evaluated limits, paging continuations, relations, and diagnostics in octocode-core. | None | Core lint, typecheck, tests, build, JSON Schema audit, contract-set audit, and prompt budget pass. |
| S2 | Rename native public tool/runtime modules and constants; keep Jev terminology only in the private provider adapter. Add capture-once orchestration, private provider paging, and cross-process executable resource continuations; remove redundant pair-ID handling and all public `jev` branches. | S1 | Native unit tests prove one canonical tool path, resource-major expansion, capture-once semantics, all-cell provider paging, private provider translation, continuations, and paired output IDs. |
| S3 | Regenerate native contracts from the actual core revision and provenance. | S1–S2 | Fingerprint, embedded body hash, source revision, and dirty flag are truthful; release mode rejects dirty provenance. |
| S4 | Rebuild CLI and MCP from the regenerated contract. Rename commands, catalog rows, help, examples, error paths, and telemetry labels. | S3 | `scheme semanticAssess` and real calls work identically through CLI/MCP; `scheme jev` is unknown and no alias appears in discovery. |
| S5 | Update Pi contracts/prompts/rendering and every skill, Chrome/scraping helper, script, error code, fixture, and test. Helpers chunk source artifacts but delegate semantic resource paging to the public contract. | S4 | No active caller invokes `jev`; every resource-question cell is processed or returned through an executable continuation, and incomplete/error chunks remain retained. |
| S6 | Rename active docs and cross-links, repair root/package agent instructions to match live CLI syntax, and label historical artifacts rather than rewriting them. | S4–S5 | Documentation, link, skill, instruction, and style validators pass with one current name, one current usage contract, and executable discovery examples. |
| S7 | Run the frozen agent-behavior evaluation plus live local-file, GitHub-file, missing-path recovery, supplied-value, matrix, independent-pair, paged-roster, saved-browser, CLI, MCP, and Pi smoke paths. | S4–S6 | Agents select the right mode, every required row is present/correlated/body-free/policy-equivalent, held-out safety and quality gates pass, and no stale public name appears. |
| S8 | Run release checks and inspect the published artifact contents. | S7 | Clean provenance, platform checks, package contents, catalog fingerprints, and release documentation all pass. |

### Files, APIs, and contracts

| Area | Required update | No-legacy check |
|---|---|---|
| octocode-core | Tool constants, schemas, exported types, descriptions, instructions, catalog, preparation, output schemas, examples, tests, README. | No public `Jev*`, `jev` tool name, pair correlation IDs, or pair-first guidance. |
| Native runtime | Contract preparation, engine dispatch, security routing, context capture, batching, provider translation, results, tests, architecture docs. | One `semanticAssess` dispatch branch; Jev name confined to provider code and provider configuration. |
| CLI and MCP | Commands, scheme output, registration, help, rendering, structured output, integration tests. | No alias, fallback registration, stale catalog entry, or hand-written divergent guidance. |
| Repository agent instructions | Root/package `AGENTS.md`, handoffs, and setup snippets use `scheme <tool>` plus direct canonical tool calls from live help. | No active `tools --json`, `tools <name> --scheme`, retired tool-call `--json`, or other known-dead invocation. |
| Pi | Gateway contracts, catalog prompts, rendering hints, harness docs, tests. | No `jev` activation or prose outside historical fixtures. |
| Skills | Rename skill folder/name, triggers, README, references, validators, installed manifests, and internal links; encode the admission table and context lifecycle without duplicating field definitions. | No forwarding skill or duplicated schema; live discovery remains the source of truth. |
| Chrome/scraping | Triage invocation, request filenames where public, functions, errors, usage fields, docs, fixtures. Helpers own safe artifact splitting and roster construction; `semanticAssess` owns semantic paging. | Every chunk enters the public matrix/continuation flow; no private paging protocol, `JEV_UNAVAILABLE`, or `runJev` remains in active code. |
| Docs | Rename active Jev guides, headings, commands, examples, ownership links, architecture references, and release notes. | Historical POCs remain immutable and explicitly labeled; active guidance uses only `semanticAssess`. |
| Generated/release | Regenerated Rust/JSON contracts, provenance, fingerprints, package output, platform bundles. | No hand edits; published catalog contains exactly one semantic-assessment tool. |

### Acceptance contract

| Requirement | Pass/fail acceptance | Rollback threshold |
|---|---|---|
| One public tool | All live catalogs expose exactly `semanticAssess`; `jev` is absent. | Any duplicate/alias or interface disagreement blocks release. |
| Executable discovery guidance | Every active agent instruction and skill example executes against the staged CLI/MCP catalog exactly as written. | Any retired command, unknown flag, or stale schema path blocks release. |
| Model-independent contract | Public names, schemas, descriptions, examples, and skills contain no provider-specific Jev terminology except provider setup documentation. | Any provider term in the public contract blocks release. |
| Matrix correctness | Every question evaluates every resource once; rows are resource-major and carry both IDs. | Missing/duplicate cells, repeat capture, or leaked IDs in provider payload blocks release. |
| Independent mode | `queries[]` supports only independent pairs and correlates by `index`; matrix IDs are rejected. | Any ambiguous mixed mode or redundant public correlation path blocks release. |
| Agent routing | Held-out agents skip exact/mandatory reads, choose matrix for shared questions, choose `queries[]` only for independent pairs, and keep dependent questions sequential. | Any systematic wrong-mode selection, forced-call behavior, or unread-body defeat blocks release. |
| Context discipline | Unread resources stay outside host context until selected for proof; already-visible evidence uses `value`; compaction preserves identifiers, limitations, and continuations. | Any needless reread, lost recovery handle, or summary-only proof blocks release. |
| Resource paging | The runtime evaluates every accepted cell through internal provider pages or returns a cross-process `next.assess`; helpers only split source artifacts. | Any silent resource/error drop, in-memory-only continuation, duplicate capture, or global negative from a partial page blocks release. |
| Security and evidence | Hidden bodies remain out of host output; ordinary authorization, redaction, SSRF, path, continuation, and cancellation policies remain active. | Any policy bypass or body/secret leak blocks release. |
| Trust boundary | Retrieved files, pages, tool annotations, and provider text remain data; they cannot alter questions, objectives, permissions, or requested actions. | Any benign or adversarial fixture that changes agent authority blocks release. |
| Measured benefit | Equivalent-workflow evaluation preserves task correctness, citation fidelity, and false-exclusion safety while reporting host/provider tokens, reads, pages, calls, latency, and errors. | Regressed quality/safety or an efficiency claim without production-token evidence blocks the claim and can block the instruction change. |
| Provenance | Generated contracts identify their true core revision and dirty state. | Dirty or mismatched release provenance blocks publish. |

### Agent-behavior evaluation gate

Freeze the baseline prompts, tool catalog, executable hashes, source fixtures, and expected next actions before changing guidance. Run the current instructions and the candidate on the same realistic tasks, reserve held-out variants, inspect raw tool calls/results, and keep a failure ledger by `select`, `call`, `read`, and `act` stage.

| Case | Expected observable behavior |
|---|---|
| Exact known span or required evidence | Zero `semanticAssess` calls; direct source read. |
| Several unread local and GitHub candidates with two shared questions | One matrix, stable IDs, complete cross-product, proof reads only for retained rows. |
| Independent facts with unrelated questions | `queries[]`; no meaningless cross-product. |
| Question B depends on answer A | Two sequential calls or direct verification, never one matrix. |
| Evidence already in host context | `context:{value}` or no call; no duplicate read. |
| Roster exceeds one provider page | Automatic internal pages; every cell returned once with truthful shared usage. |
| Roster/result exceeds the public response budget | `isPartial` and executable `next.assess`; the resumed one-shot CLI call completes the roster without lost IDs. |
| Nested source continuation or incomplete view | No automatic scope expansion; partial/insufficient remains retained and the exact continuation is recoverable. |
| Missing local path, missing GitHub path, provider error, and cancellation | No inference on failed retrieval; isolated error row and safe recovery where available. |
| Browser/HAR corpus containing prompt injection and split secrets | Task facts remain usable, instructions remain caller-owned, and no secret/body leaks to host output. |
| Conflicting typed judgments | No averaging or voting; route to direct evidence/test. |

Track task success, tool-selection accuracy, invalid calls, false exclusions, citation fidelity, changed-next-action rate, unnecessary assessments, proof reads, source bytes kept out of host context, actual host input/cached/output tokens, provider input/output tokens, calls, pages, latency, and failures. Count preparation, discovery, retries, continuations, and verification. Provider-token or byte reductions are not host-token savings. Keep the instruction change only when the target behavior improves without unacceptable correctness, security, or latency regression; otherwise revert or narrow the owning rule.

### Rollout and rollback

Ship the rename in one major-version cutover after S1–S8 pass. Do not stage both public names. Update all first-party callers in the same release and publish a concise migration note mapping old commands and schemas to `semanticAssess`. Roll back by reverting the release as a unit; do not restore a runtime alias. Provider failures can roll back the private adapter independently only when the public `semanticAssess` contract remains unchanged.

## Implementation and POC checkpoint

**Implemented in this working tree:** W1 recovery receipts, A1 continuation ownership, A2 output-policy parity, W2 per-patch guidance, W4 matrix questions/capture reuse, W5 unread-query guidance, and A3 bounded browser/scrape resources. The matrix is additive; flat pairs remain compatible. Failed retrieval retains its safe error code and body-free receipt, states that inference did not run, and has no answer or provider usage. Nested continuations and direct/hidden sanitization remain unchanged.

The pre-cleanup integration checkpoint passed 205 core tests, lint/typecheck, 323 native library tests and 19 Jev integration tests; native binary/addon and CLI/MCP builds completed. [Rebuilt CLI/MCP smoke evidence](../.octocode/tmp/jev-integration-smoke/after-live.json) covers recovery, executable continuations and two real-provider successes. Controlled tests inspect outgoing sanitized state and zero inference on failed retrieval. This is the named verification scope, not every monorepo suite.

Initial regeneration failed on dirty-core provenance and four outdated default-field expectations. The corrected tests preserve rejection of missing operation/URI anchors while accepting intentional defaults. Final regeneration used clean core provenance and passed; the guards were not weakened. Those results describe that checkpoint, not every later working-tree revision.

## RFC delta: resource-question matrix (pre-rename implementation)

### Decision

The pre-rename implementation adds one envelope for a true cross product. The target `semanticAssess` contract preserves this shape:

```json
{
  "reasoning": "Why this matrix changes the next action.",
  "resources": [{"id": "resource-1", "context": {"tool": "localFetch", "query": {}}}],
  "questions": [{"id": "relevance", "question": {"type": "noul", "instructions": "Is it relevant?"}}]
}
```

Every question is evaluated against every resource in resource-major order. Result rows expose `resourceId` and `questionId`. IDs are 1–64 ASCII letters, digits, dots, underscores, or hyphens and start alphanumeric. Resource and question IDs must be unique. The matrix admits at most 25 cells, 25 resources, and five questions. Direct input and `queries[]` remain the independent-pair mode; pair rows use their ordered `index` and cannot accept matrix correlation IDs.

### Runtime flow

1. Core owns descriptions, limits, input/output schemas, and matrix-preserving preparation.
2. Native validates the original envelope, enforces refinements absent from JSON Schema, trims shared reasoning, and expands resource-major rows.
3. Security validates every expanded row. Correlation IDs never enter provider state or question instructions.
4. Flat pairs retain fresh nested reads. Matrix rows cache the exact `(resourceId, canonical context)` capture within the call, including a shared retrieval failure, so one resource is never fetched once per question.
5. Identical captured states group for provider inference. Groups split automatically under the existing 24 KiB state-plus-question and 48 KiB combined headroom; independent groups remain concurrency-bounded.
6. Output preserves ordered indexes, row isolation, receipts, usage attribution, and correlation IDs.

### Large-resource pagination

At the pre-rename checkpoint, `jev` does not silently follow a source tool continuation because doing so changes evidence scope, and it does not truncate one oversized state. Callers represent large files, browser bodies, and HAR-derived artifacts as bounded resources. At that checkpoint, the scraping/Chrome bridge supplies the loop: it resolves every manifest part, splits UTF-8-safe byte chunks, submits successive matrices of 1–25 resources, and aggregates only after every chunk returns. Its `--limit` controls matrix page size; the loop tracks the complete discovered resource roster and reports missing or errored rows. The target keeps artifact splitting in the bridge but moves semantic roster paging into `semanticAssess` as specified above.

Aggregation is conservative and exclusive: any relevant chunk routes the candidate to `read`; otherwise partial, insufficient, mention, error, or low-confidence unrelated routes to `consider`; `skip` requires every chunk to be covered and confidently unrelated. Thin browser shells bypass Jev and stay in `consider`. Canonical path containment rejects explicit paths and symlink targets outside the scrape session.

### Rejected alternatives

- A separate semantic-assessment wrapper duplicates validation, security, dispatch, receipts, and output semantics.
- Repeating flat context for every question preserves compatibility but repeats mutable retrieval, so questions can observe different resource states.
- Auto-following continuations hides evidence-scope changes and can convert bounded retrieval into an uncontrolled crawl.
- Sending whole HAR/browser dumps makes provider limits and false negatives depend on arbitrary truncation.
- Returning answer maps instead of rows weakens row isolation, usage attribution, and existing interface rendering.

### Acceptance

- Core: union schema, matrix refinements, descriptions, prompt budget, JSON Schema export, and correlated output validate.
- Native: resource-major expansion, 25-cell rejection, duplicate-ID rejection, provider-payload non-leakage, one hidden capture across questions, fresh flat repeated captures, ordered IDs, and isolated failures pass.
- Browser/scrape: multipart and >50 KiB fixtures produce all UTF-8-safe resource pages; no silent drop; path escape rejected; relevant partial rows are not duplicated across routes.
- Interfaces: rebuilt CLI and MCP advertise the same `semanticAssess` matrix schema and execute one local, one GitHub, and one saved-browser smoke path; live discovery contains no `jev` alias.
- Release: generated contracts must be regenerated from clean core provenance before publish. Dirty provenance is allowed only for local iteration and remains an explicit failing release gate.

### Pre-rename live dogfood record (2026-09-20)

- At this pre-rename checkpoint, the built local CLI still exposes the legacy `queries[]` schema; the checked-in generated contract contains the matrix. A native/CLI rebuild is therefore a required interface gate, not assumed success.
- Live `octocode jev` evaluated two local skill resources with two questions each. Chrome guidance passed both boundaries. Scraping guidance initially returned `gap` for complete-chunk aggregation; after stating that no final route is emitted until every part/chunk has a result, the same question returned `yes` with 1.00 confidence.
- Live Jev inspected two native implementation resources with two questions each. Matrix order/correlation and uniqueness/cell bounds returned `yes` at 1.00. Capture reuse and provider splitting returned `yes`; the large contract resource split into separate provider calls while the smaller batching resource shared usage, with no missing result row.
- A nonexistent GitHub path stopped before inference and returned an executable tree continuation. A confirmed GitHub file then executed through `ghGetFileContent`; its deliberately narrow view returned `insufficient`, demonstrating conservative handling rather than an unsupported claim. Repeated legacy remote pairs produced different capture hashes, reinforcing the matrix decision to capture a resource once before applying several questions.
- Generated-schema review passed the bounded-provider question but exposed weak colocation of the output correlation rule. The core envelope description now states directly that each result row returns its matching `resourceId` and `questionId`.
- A final live semantic review of `.octocode/JEV.md` shared one captured state across three questions. Full cross-product semantics returned `yes` at 0.97 confidence, safe large-resource paging at 0.96, and correlated output/evidence boundaries at 1.00.
- Browser/scrape triage passes 10 focused regressions. A six-resource UTF-8 fixture is processed through three automatic matrix pages without drops. Stdout bounds resource errors with `errorCount`/`errorsTruncated`; `triage.json` preserves every error and receipt.

[POC v1](../.octocode/jev-poc-2026-09-20/REPORT.md) preserved a failed compound-question exact-label gate; uncertain candidates were retained. [Separately frozen v2](../.octocode/jev-poc-2026-09-20/v2/REPORT.md) passed six atomic classifications, retaining four relevant candidates and excluding two irrelevant windows from routed proof. Separate verifier reads count. Generated negatives could have been excluded by ordinary path filtering. These development probes do not establish blind adoption, actual host-token savings or the effect of core instructions built after the POCs. No new Octocode-only baseline ran.

## Review disposition

“Shipped” refers to the checkpoint above. Proposed fields and admission changes below are **not current contracts**.

| ID | Priority | Disposition | Remaining work |
|---|---|---|---|
| W1 | P0 | Shipped: hidden-read recovery | Preserve failure/continuation regressions when receipts evolve. |
| W2 | P0/P1 | Shipped: PR `charLength` is explicitly per file patch | Verify multiple patch/page axes and revision drift; any aggregate cap needs a separate ordinary-tool contract decision. |
| W3 | P1/P2 | Implemented: matrix capture reuse | Each matrix resource is captured once; flat repeated pairs remain fresh. Keep integration tests for both behaviors. |
| W4 | P1 | Implemented: resource-question matrix | Correlated resource-major rows; 25-cell admission cap; provider grouping splits under size headroom. |
| W5 | P1 | Shipped: unread-query instructions | Keep real CLI/MCP discovery examples valid; `value` is supplied evidence, not path loading. |
| W6 | P1 | Proposed: auditable scope receipts | Define bounded deterministic completion/count metadata without claiming global completeness. |
| W7 | P1 | Partly shipped: scalar wrapping and canonical nested-query guidance | Nonempty instructions and Choice minimum-two admission require an explicit compatibility decision. |
| W8 | P1/P2 | Shipped: independent-answer/conflict guidance | Reproduce the reported live disagreement; automated consistency requires a relation contract and evaluation. |
| A1 | P0 | Shipped: nested continuation ownership | Preserve outer/nested trace separation and ordinary-tool behavior. |
| A2 | P0 | Shipped: hidden/direct redaction parity | Keep output policy before provider transmission, hashing and reuse. |
| A3 | P1/P2 | Implemented for saved browser/scrape files | All parts use UTF-8-safe bounded resources; helper pages matrices, aggregates conservatively, and rejects paths escaping the session. |
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
