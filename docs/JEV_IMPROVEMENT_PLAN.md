# `semanticAssess` implementation plan

Status: implemented and integration-verified as of 2026-09-21. The public contract, native runtime, CLI, MCP, Pi-facing docs, skills, and provider lab use the single `semanticAssess` surface. The canonical core is clean at `7b0ede1a`; native contracts report `sourceDirty:false` with fingerprint `99c6eab576c8…`. Clean-provenance and contract-sync checks pass. The [semantic assessment reference](OCTOCODE_SEMANTIC_ASSESS.md) describes the public contract, [JEV.md](../.octocode/JEV.md) indexes frozen provider-era experiments, and [GOTCHAS](../.octocode/GOTCHAS.md) records operational lessons.

## Audit Reasoning — fix-and-keep (2026-09-20)

- **Status:** Cutover implementation, end-to-end behavior verification, and clean-source contract provenance pass.
- **Why kept:** This document remains the owner for acceptance criteria, frozen evidence, and release verification.
- **Evidence:** The generated contract exposes `semanticAssess`, the direct/batch matrix union, correlated pages, and separate requested/resolved model fields. Interface, skill, and documentation changes are working-tree observations, not release claims until rebuild and end-to-end checks pass.
- **Remaining work:** No semantic-cutover work remains. Run the repository's normal final publish workflow at release time; do not bypass its provenance or package guards.

### Implemented verification snapshot — 2026-09-21

- Canonical core: 199/199 tests passed, including direct and batched SemanticQuery validation, primitive null boundaries, matrix limits, complete field descriptions, selector-deduplicated defaults, description budgets, schema budgets, and the shared 100-item security ceiling.
- Native: runtime, CLI integration, cache/local/semantic integration, and the 642-test engine suite passed (641 passed, one intentional ignore). Formatting, clean provenance, and contract sync passed.
- Built path: native runtime, both add-ons, CLI, and MCP development builds passed. CLI tests passed 123/123 and MCP tests passed 182/182. Full built stdio acceptance plus deterministic CLI/MCP parity passed 39/39.
- Availability: an isolated real MCP process passed 14/14 checks with 11 tools when the key was absent and omitted `semanticAssess`; with a resolved key it passed 15/15 checks with all 12 tools and included `semanticAssess`. The CLI kept the command discoverable, returned exit 5 without a key, and named `OCTOCODE_JEV_KEY` plus the setup URL. The removed `jev` command returned an unknown-subcommand error.
- Live provider matrix: two resources × three questions produced six correlated cells. Noul, Choice, and Score all returned native answer objects; the observed requested model was `jev-latest` and the resolved model was `jev-1.13.0`.
- Live batch: two independent SemanticQuery objects retained their query IDs and produced separate nested results.
- Live resource pagination: `maxChars` now charges sanitized evidence payload rather than repeated result-envelope and continuation metadata. A 78,377-character local document produced five explicit page-local answers, reached the deciding final-page marker at Noul 0.98, and returned `coverage:"complete"` with no continuation. A true 80,001-character payload returns a schema-valid `next.assess`; running it unchanged completes the remaining page without leaking the source body.
- Skills and docs: research, semantic assessment, RFC, Chrome, and scraping skills passed their self-tests and a five-skill review with zero errors or warnings. Documentation verification passed.
- Benchmark/dev tooling: the Jev lab passed 8/8 tests while preserving raw provider payloads and requested/resolved model provenance. The benchmark package passed 19/19 TypeScript, 67/67 Python, and 97/97 advanced-research tests; its bundled skill passes the zero-error structural gate (remaining notices are advisory cleanup for historical reference length/navigation).

### Public tool-contract optimization and scorecard — 2026-09-21

The final public catalog has 12 tools. Descriptions now state the positive trigger, negative boundary, and evidence handoff instead of repeating fields. `astRewrite` explicitly says it is not a research tool. Known/unknown path, ref, package, and symbol routing is consistent across tools. MCP instructions retain only cross-tool workflow, evidence, pagination, availability, and security rules; tool-local rules live in each description/schema.

Measured prompt surface after regeneration:

- Global instructions: **2,503 bytes**, down from 3,866.
- Full descriptions: **3,697 characters total**; every tool is below 400 characters.
- Default input schemas: **97,277 bytes**. Enabling default-off `astRewrite` raises the total to 119,391 bytes.
- Built MCP `tools/list`: **103,104 bytes** by default and 125,698 with rewrite enabled; neither MCP nor CLI advertises output schemas.
- `semanticAssess` input schema: **5,931 bytes**, reduced from 12,428 by reusable `$defs` without changing its accepted grammar.
- Every public input array is bounded by the native 100-item security ceiling. Contract tests ratchet description length, total default schema size, and array bounds.

The live provider evaluated the final descriptions as 24 complete cells: selection **3.085/4**, density **3.434/4**, combined **3.260/4 = 8.15/10**. This improved the earlier 7.85/10 pass. A separate schema-only semantic pass scored apparent simplicity at 2.021/4; that is retained as a friction signal, not treated as proof that precise discriminated unions should be weakened. The largest schemas are `astSearch` (23,724 bytes) and default-off `astRewrite` (22,114 bytes). They encode materially different operations and safety states; deleting those constraints would save tokens by making invalid calls easier. The next safe lever, if measurements justify it, is an operation-selected compact view—not a looser runtime contract or another public tool.

| Tool | Description | Schema | Implementation | Current judgment |
|---|---:|---:|---:|---|
| `semanticAssess` | 7.7 | 8.8 | 9.3 | Matrix, batch, typed primitives, raw answers, background acquisition, and recovery are live; description intentionally keeps the proof boundary. |
| `ghSearch` | 8.3 | 8.6 | 8.8 | Clear discovery boundary and bounded result modes; remote-index coverage remains inherently partial. |
| `ghGetFileContent` | 8.2 | 8.8 | 9.1 | Exact known-repo/ref/path reader with executable pagination and immutable-ref guidance. |
| `ghSearchHistory` | 8.2 | 8.5 | 8.8 | Discovery-only history surface; routes known identities to the item reader. |
| `ghGetHistoryItem` | 8.1 | 8.4 | 8.9 | Richest GitHub read schema; patch and independent expansion axes remain explicit instead of being flattened. |
| `artifactSearch` | 7.8 | 8.7 | 8.5 | Concise package/version/source discovery; registry facts are correctly separated from source proof. |
| `ghCloneRepo` | 8.1 | 9.0 | 8.7 | Smallest schema and a clear handoff to local tools; mutation/auth policy remains runtime-owned. |
| `localSearch` | 8.4 | 8.3 | 9.0 | Strong lexical discovery contract. Its two schema branches make `matchOnly` constraints executable rather than prose-only. |
| `astSearch` | 8.2 | 7.9 | 9.0 | Broadest read-only syntax/topology surface and largest default schema. Keep topology as candidate evidence and confirm identity with LSP. |
| `astRewrite` | 8.2 | 8.2 | 8.9 | Default-off, explicitly non-research, preview/apply states guarded by snapshot and hashes. Large schema is excluded from the default prompt budget. |
| `localFetch` | 8.3 | 9.0 | 9.2 | Small, exact, and recoverable; whole-file, range, match-window, minified, and continuation semantics are aligned. |
| `lspSearch` | 8.4 | 8.5 | 9.2 | Strongest routing description and precise anchor alternatives. TypeScript progress-request compatibility, cold-start budgets, cancellation cleanup, and CLI/MCP parity are regression-covered. |

Scores are ten-point engineering ratings. Description scores are the live semantic selection/density averages scaled to ten; schema and implementation scores combine contract inspection with the tests and real interface paths above. They are comparative maintenance signals, not release assertions by themselves.

### All-tool workflow execution audit — 2026-09-21

The final built-path audit exercised every registered tool through MCP and its CLI counterpart, including real GitHub, npm, clone, local, AST, rewrite-preview, LSP, and semantic-provider paths. The enhanced acceptance harness passed **52/52 checks** across all 12 tools in **35.0 seconds**, with zero transport errors or stderr output. It executes returned continuations instead of only inspecting their shape, accepts CLI exit code 6 as valid partial success, and ignores only opaque cursor bytes when comparing deterministic CLI/MCP payloads. `semanticAssess` is execution-verified rather than byte-compared because provider judgments are intentionally nondeterministic.

Observed MCP medians were 9.6 ms for `localSearch`, 56.6 ms for `localFetch`, 13.5 ms for `astSearch`, 13.1 ms for `astRewrite`, 311 ms for `ghGetFileContent`, 494 ms for `ghSearch`, 732 ms for `ghSearchHistory`, and 4.0 ms for the mixed `semanticAssess` sample (which includes local validation rejections; the live provider page took about 1.0 s). Warm-process `lspSearch` median was 44 ms; its 2.36 s maximum reflects cold language-server startup. One-shot CLI processes add roughly 260–420 ms of process/runtime overhead for local tools, while cold `lspSearch` took 2.63 s. These are single-machine diagnostic measurements, not SLOs.

The audit found one real correctness race: on cold TypeScript startup, definition lookup could stop on the import alias before the language server had resolved the next hop. Before repair, 1 of 8 isolated CLI trials returned the alias in `entry.ts`; a bounded 50 ms retry on only the first unresolved same-file hop produced 12/12 correct `math.ts` definitions. The retry is deliberately narrow so genuine cycles, later-hop failures, and cross-file results are not hidden.

Guidance and recovery stayed compact: 75 executable continuations were schema-valid and targeted registered tools; the one intentional empty-result recovery emitted exactly one 72-character hint; all 12 malformed calls produced actionable validation diagnostics under 2 KB and the server remained responsive. Maximum observed response size was 6.3 KB (`lspSearch`). The reproducible local receipt is [`.octocode/octocode-eval-benchmark/all-tools-live.json`](../.octocode/octocode-eval-benchmark/all-tools-live.json), and the evaluation report is [`.octocode/octocode-eval-benchmark/tool-workflow-audit.md`](../.octocode/octocode-eval-benchmark/tool-workflow-audit.md).

### Agent-readiness scorecard before this amendment

These are planning-rubric scores, not token-saving measurements. A live pre-rename semantic review of this document classified agent admission/mode selection as `gap` with 0.93 probability, layer ownership as `adequate` with 0.98 probability, and the context/evaluation lifecycle as `adequate` with only 0.65 probability. On that evidence, the plan was **7.5/10 for agent readiness**: strong architecture and ownership, but underspecified call admission, context reuse, resource paging, and held-out behavior checks. After the `SemanticQuery` and paging repair, bounded semantic rechecks classified the input/output grammar as `ready` with 0.97 probability, paging/recovery as `ready` with 0.99 probability, and agent guidance/evaluation as `ready` with 0.95 probability. The implemented design rates **9.8/10**: contracts, live behavior, and clean provenance pass. The remaining 0.2 reflects the deliberately narrow live semantic sample and single provider version, not a known contract or runtime defect.

## Decision and boundaries

Ship one public `semanticAssess` tool. Its unit of work is a `SemanticQuery`: `{id,reasoning,resources:[{id,context}],questions:[{id,question}]}`. The tool accepts either one unwrapped `SemanticQuery` or `{queries: SemanticQuery[]}` for a batch. Each semantic query defines one bounded decision domain; the runtime evaluates its resource-major `resources[] × questions[]` cross-product. Every question checks one atomic aspect. Add questions to assess several aspects of the same resources; add semantic queries only when the resource set, question set, reasoning, or paging policy differs. Batch elements stay isolated and correlate by `queryId`; `queries[]` never accepts the legacy flat `{context,question}` pair.

The query language is the product boundary: resources name evidence, questions define typed conditions, and the cross-product produces body-free assessment cells. Background context acquisition is part of language execution, not an optional helper. For `context:{tool,query}`, the runtime privately dispatches the ordinary read, applies its validation, authorization, caching, redaction, SSRF/path, timeout, and cancellation policies, and sends only the sanitized captured state to the private provider adapter. The host receives typed judgments, stable IDs, coverage, usage, and recovery—not the retrieved body. `context:{value}` remains an escape hatch for evidence already visible to the host; it is not the primary path.

The tool admits one to five semantic queries per call, up to 25 resources, five questions, and 25 logical cells per semantic query. The global call cap is 50 logical cells. One logical resource can assess at most 80,000 sanitized characters in a call. Safe ordinary-tool continuations become explicit page-local assessments; the runtime stops before a later page would exceed `maxChars`, and an oversized first page is a visibly partial bounded prefix rather than an unbounded provider request. Provider grouping also observes request-byte headroom. These are safety ceilings, not throughput promises. Model selection and tokenization stay internal. `semanticAssess` must not gain a separate reader, GitHub client, or policy bypass.

Public schemas and agent guidance belong in octocode-core. CLI/MCP expose those contracts; native runtime owns context orchestration. Native public dispatch uses the `SemanticAssess` identity, while resource paging lives in the runtime orchestration layer and the private `tools/jev/` adapter retains provider-specific request, model, credential, retry, and transport concerns. Skills remain optional shortcuts. Assessments route effort; source reads and tests establish claims. Generic semantic assessment does not imply arbitrary-size ingestion, free-form summarization, unseen identifier extraction, proof, or authority to take actions.

The cutover is intentionally hard: do not register a public `jev` alias, compatibility tool, duplicate schema, forwarding command, or deprecated skill. Document the rename in release notes, but expose only `semanticAssess` through runtime discovery. Historical receipts, POCs, and frozen benchmark artifacts keep their original names for provenance; label them pre-rename rather than rewriting them.

## Public rename and contract plan

### Public and internal names

| Surface | Target | Rule |
|---|---|---|
| Public tool and CLI command | `semanticAssess` | The only discoverable/callable name. |
| Display title | Semantic Assessment | Human-readable title; not a second identifier. |
| Public schema/types | `SemanticAssess*` | No exported `Jev*` contract aliases after the cutover. |
| Native public dispatch | `SemanticAssess` / `semanticAssess` | Owns public identity and validation handoff; runtime orchestration owns capture, paging, batching, and output shaping. |
| Provider adapter | Jev-specific internal module | Retains endpoint, model, credential, retry, and transport concerns. Native Noul/Choice/Score primitives remain public because changing them alters answer semantics. |
| Active skill | `octocode-semantic-assess` | Replaces `octocode-jev-reasoning-loop`; no forwarding skill. |
| Active docs | Semantic-assessment names | Rename current guides and links; preserve historical artifact paths. |

### Public schema cleanup

- Export one `SemanticQuerySchema` and derive both accepted envelopes from it: direct `SemanticQuery` or `{queries: SemanticQuery[]}`. Do not retain a second pair schema.
- Require unique `queryId`, `resourceId`, and `questionId` values. A result cell carries all three; a resource page retains the same triple plus its zero-based `pageIndex`.
- Define `queries[]` as a batch of complete semantic queries. Reject legacy pair elements, mixed envelopes, duplicate IDs, empty arrays, unknown fields, and any batch whose logical-cell total exceeds the global cap.
- Keep every question atomic. Several questions over the same resources belong in one semantic query; dependent questions require later calls because provider answers cannot change the active matrix.
- Require nonblank string `reasoning`. Question `instructions` accept a nonempty string, object, or array; recommend a string first and structure only when labels or supporting data improve the boundary. Noul criteria are optional; when supplied they define both `true` and `false`, whose descriptions may be `null`. Choice requires 2–255 declared alternatives whose descriptions may be `null`. Score requires 2–10 independently described ordered levels, and each level must be a non-null, non-empty string, object, or array. Reject blank/null instructions, one-option choices, null/empty Score levels, and undeclared answer labels.
- Allow `context.value` only for a string, nonempty object, or nonempty array. Wrap scalar facts, such as `{observed:false}` or `{count:3}`; do not silently transform them.
- Keep the native primitive names and answer shapes: `noul`, `choice`, and `score`. `semanticAssess` is the generic tool identity; renaming Noul or replacing its answer creates semantic drift at the adapter boundary.
- Keep provider/model diagnostics separate from tool identity. Every successful page reports both required `requestedModel` and `resolvedModel`; error pages may omit them when resolution never occurred.
- Use one output shape for direct and batch inputs: `queries:[{queryId,results:[...]}]`. Each logical cell has `{resourceId,questionId,coverage,pages:[...]}`; each successful page has `{pageIndex,answer,context,requestedModel,resolvedModel,usage?}` where `answer` is the unmodified native primitive answer object. Never threshold, relabel, average, round, flatten, or synthesize answer fields. Never echo source bodies.

### Native primitive contract

| Primitive | Short tool-description text | Response-description text |
|---|---|---|
| [Noul](https://docs.typesafe.ai/primitives/noul) | One yes/no proposition. Returns `P(yes)` from 0 to 1; a value near 0.5 is uncertainty, not medium intensity. | `{type:"noul",noul}` where `noul` is the complete yes/no probability distribution represented as `P(yes)`; there is no separate confidence field. |
| [Choice](https://docs.typesafe.ai/primitives/choice) | One selection from declared, unordered alternatives. Include an `other`/`none` option when the set may be incomplete. | `{type:"choice",choice,probabilities,confidence}`; `choice` is the highest-probability label, probabilities sum to 1, and confidence describes distribution concentration—not correctness. |
| [Score](https://docs.typesafe.ai/primitives/score) | One ordered dimension with 2–10 independently described levels from low to high. | `{type:"score",score,probabilities,legend,confidence}`; `score` is the probability-weighted zero-based level and may be fractional. Read the distribution and confidence with it. |

[Structured instructions and criteria](https://docs.typesafe.ai/primitives/advanced) use the provider's JSON entry vocabulary for instructions, Choice option descriptions, Score level descriptions, and Noul `true`/`false` criteria. Structure clarifies labels and carries supporting data; it does not create a fourth primitive or permit compound questions.

### Agent behavior and context contract

The agent-facing promise is: **use `semanticAssess` only when a bounded semantic answer can change the next action; keep source bodies outside host context until the assessment says which proof to read.** A call routes effort. It does not prove a claim, summarize an arbitrary corpus, grant authority, or replace tests.

#### Admission and mode selection

| Situation | Required agent action |
|---|---|
| Exact lookup, arithmetic, schema inspection, known deciding span, or source that must be read either way | Skip `semanticAssess`; use the exact tool or test directly. |
| Optional unread candidates where relevance, risk, support, or an alternative changes the next read/test | Call `semanticAssess` with unread `context:{tool,query}` resources. |
| The same questions apply to every resource | Use one `SemanticQuery`. Every admitted resource receives every admitted question. |
| The same resources need several independent aspects checked | Add atomic entries to `questions[]`; do not repeat the resources in separate queries. |
| Resource sets, question sets, reasoning, or paging policies differ | Submit `queries:[SemanticQuery,...]`; correlate by `queryId`, `resourceId`, and `questionId`. |
| A later question depends on an earlier answer | Keep calls sequential; do not encode control flow inside one semantic query or batch. |
| Evidence is already visible to the host | Use `context:{value}` only when another judgment still changes an action; never reread or resend it through an ordinary tool. |
| Opposite valid answers lead to the same action | Skip the call because it cannot change the next step. |
| A logical resource spans provider pages | Keep the same question object and cell IDs on every page; inspect every page result before routing the resource. |
| A source page, resource page, or result is partial, errored, or insufficient | Retain it, preserve its receipt, and run the returned continuation or direct proof read. Never classify uncovered scope as unrelated. |

The target single-query call below does not become live until S4. It produces four resource-major cells; no resource can disappear merely because another resource or question fails. To batch different decision domains, wrap complete objects of this shape in `{"queries":[...]}`. Do not place legacy `{context,question}` pairs in that array.

```json
{
  "id": "rename-impact",
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
      "question": {"type": "noul", "instructions": "Could this resource affect authorization, redaction, or secret handling?"}
    }
  ]
}
```

The public type contract is:

```ts
type SemanticAssessInput = SemanticQuery | { queries: SemanticQuery[] };
type SemanticQuery = {
  id: SemanticId;
  reasoning: string;
  resources: SemanticResource[];
  questions: SemanticQuestion[];
};
type SemanticResource = {
  id: SemanticId;
  context: { tool: ReadTool; query: ReadQuery } | { value: JsonValue };
  maxChars?: number; // Default and maximum: 80_000.
};
```

#### Conditional query semantics

| Element | Meaning |
|---|---|
| `SemanticQuery` | One decision domain with one reason the result changes the next action. |
| `resources[]` | Evidence operands. A resource is a supplied value or an executable background read descriptor, never an already-read body disguised as a path. |
| `questions[]` | Atomic typed conditions. `noul` tests one proposition as `P(yes)`, `choice` selects among declared unordered conditions, and `score` evaluates one ordered dimension. |
| Cross-product | For each resource in order, evaluate every question in order. A batch never crosses resources or questions between semantic queries. |
| `queries[]` | Independent semantic tables that share transport only. Each table keeps its own reasoning, resources, questions, failures, usage, and continuation. |
| Result cell | The stable `(queryId,resourceId,questionId)` condition result, with one or more ordered page judgments and explicit coverage. |

This is a conditional query language, not a workflow/action language. Typed answers act as predicates for the host agent's next read, test, or decision. They cannot invoke tools, mutate state, grant permission, rewrite later questions, or let retrieved content define new instructions. `reasoning` records why the condition matters; it does not enter provider evidence.

Use one output shape for direct and batch inputs:

```ts
type SemanticAssessOutput = { queries: SemanticQueryResult[] };
type SemanticQueryResult = {
  queryId: SemanticId;
  results: SemanticCellResult[];
  next?: { assess: SemanticAssessInput };
};
type SemanticCellResult = {
  resourceId: SemanticId;
  questionId: SemanticId;
  coverage: "complete" | "partial" | "error";
  pages: SemanticPageResult[];
};
type SemanticPageResult = {
  pageIndex: number;
  context: BodyFreeReceipt;
  usage?: UsageAttribution;
} & ({ status: "success"; requestedModel: string; resolvedModel: string; answer: TypedAnswer } |
     { status: "error"; error: SafeError });

type TypedAnswer =
  | { type: "noul"; noul: number }
  | { type: "choice"; choice: string; probabilities: Record<string, number>; confidence: number }
  | { type: "score"; score: number; probabilities: Record<string, number>; legend: Record<string, EntryType>; confidence: number };
```

Order query results by input query, cells by resource then question, and pages by `pageIndex`. Validate native answer invariants but preserve every provider answer field and value. Provider model and usage are adjacent call metadata, never folded into or inferred from the answer. Omit unknown usage rather than reporting zero. Shared provider calls assign usage once and identify the owner. Errors stay isolated at the smallest valid level.

#### Context lifecycle

1. **Discover without bodies:** obtain candidate paths, URIs, refs, manifest entries, or exact ranges through body-free search/list views.
2. **Build semantic queries:** assign stable query, resource, and question IDs; use canonical unexecuted read queries. Put every atomic aspect that shares the resource set in `questions[]`. Do not read a body merely to prepare an assessment.
3. **Acquire in the background:** capture and sanitize each logical resource once. If a same-resource continuation is needed, freeze source identity and collect ordered pages without exposing bodies to the host.
4. **Assess every cell:** apply every question to every resource. For a paged resource, apply the exact same question object to each provider page and retain the stable `(queryId,resourceId,questionId)` identity.
5. **Inspect every result:** keep page answers, completeness, errors, usage ownership, hashes, and executable continuations. A missing, partial, or errored page is never a negative answer. Do not average page probabilities or manufacture one answer when the question has no declared reducer.
6. **Read retained proof:** directly read the deciding original source for relevant, uncertain, insufficient, or conflicting results. Cite source locations, not background assessment bodies.
7. **Verify consequential claims:** use exact source, LSP, tests, or runtime checks appropriate to the claim. The assessment remains routing evidence.

Compaction can drop repeated logs and rejected candidates only after preserving IDs, source anchors, decisions, open limitations, continuations, and the next action. A summary must never become the only copy of proof. Stable schemas/instructions should form a cacheable prefix; dynamic resources and questions remain the request tail. Cache behavior is an optimization, not a permission boundary or correctness claim.

#### Background resource pagination

`semanticAssess` owns semantic paging; browser and scraping skills only create safe artifact resources. A logical resource can contain up to 80,000 characters. The runtime keeps its body in background execution and separates two paging axes:

1. **Acquisition pages:** Follow only an executable continuation that the ordinary tool marks `continuationKind:"sameResource"`. Add this discriminator to the shared continuation contract. Never auto-follow collection pages, optional-content menus, related links, search expansion, or any continuation that changes evidence scope.
2. **Provider pages:** Split the frozen logical resource into UTF-8-safe pages whose estimated input stays below 20,000 tokens after adding the exact question, wrapper, and provider overhead. Use the configured model's tokenizer when available. Otherwise use a conservative byte ceiling and bisect an oversize page; never assume four characters equal one token.

The first acquisition page freezes identity. GitHub files resolve to an immutable commit SHA before another page. Local files carry a content hash through every continuation; mutation stops the cell with a stale-resource error instead of mixing versions. Browser and scrape manifests pin their artifact hashes and canonical paths. Each acquisition page executes once per logical resource, and every question sees the same ordered page set.

For every provider page, send the same immutable question object and retain the same `(queryId,resourceId,questionId)` cell identity. Return one logical cell with ordered `pages[]`, where each page contains `pageIndex`, its typed `answer` or isolated `error`, a body-free receipt, and usage attribution. Set cell `coverage` to `complete` only after every admitted source/provider page returns. Do not average page probabilities or choose a global answer without an explicit reducer contract. Agent guidance conservatively retains a resource when any page is relevant, uncertain, insufficient, partial, or errored; exclusion requires complete confidently unrelated coverage across all pages.

If deadline, cost, output, or call limits stop execution, return `isPartial` plus an executable `next.assess` containing the stable query ID, unchanged question objects, unresolved resource descriptors, frozen source identity, and exact continuation. It must resume in a one-shot CLI process without in-memory state or replaying completed pages. A slow semantic query cannot suppress completed sibling queries: the batch returns completed query results and an isolated continuation/error for unfinished queries. Bound concurrency, honor cancellation between capture and provider pages, and record known usage before returning.

#### Guidance ownership

| Layer | Sole responsibility |
|---|---|
| Core schema | Exact discriminated envelopes, field semantics, IDs, limits, mutual exclusions, output correlation, completeness, and executable continuation shape. |
| Core tool description | `Use when → Do not use when → Inputs → Returns → Next`; enough information to select `semanticAssess`, one semantic query, or a batch, without restating types. |
| Core MCP/CLI instructions | Cross-tool `discover → assess → read proof → verify` workflow, shared trust boundary, and partial-result rule. |
| Active skill | Conditional examples, large-artifact workflow, recovery recipes, and live-schema discovery; it points to core rather than copying the full schema. |
| Native runtime | Ordinary-tool dispatch, authorization, redaction, capture-once semantics, provider paging, cancellation, output shaping, and recovery. |
| Product docs | Rationale, migration, end-to-end examples, limitations, and measured evidence; no second live field contract. |

The contract audit must compare `semanticAssess` against every ordinary tool allowed in `context.tool`, verify that every advertised `Next` exists, and fail on split ownership, field/name/type drift, phantom continuations, or guidance that exceeds runtime authority.

#### Target tool and agent wording

Use the following wording as the implementation baseline, then change it only when held-out tool-selection evaluation supports the change.

**Short description:** “Evaluate Noul, Choice, or Score questions over background-fetched resources without returning their bodies.”

**Tool description:** “Use when bounded semantic judgments over unread resources can change the next read, test, or decision. Do not use for exact checks, known deciding spans, or evidence that must be read regardless. Input one `SemanticQuery`, or batch complete semantic queries in `queries[]`. Within each semantic query, every atomic question applies to every resource; add questions for more aspects, and add semantic queries only when their resource/question sets or policies differ. Noul returns `P(yes)` for one proposition; Choice returns a selected label, full option distribution, and confidence; Score returns a probability-weighted position over ordered levels, its distribution, legend, and confidence. The runtime acquires and sanitizes resources in the background, pages large logical resources with the same questions, and returns body-free results correlated by query, resource, question, and page. Native answer fields are preserved. Read retained source for proof. Continue incomplete results with the returned `next.assess` unchanged.”

The shared MCP/CLI instruction owns this workflow:

1. Discover candidate identifiers without bodies.
2. Skip `semanticAssess` if direct evidence is mandatory or opposite judgments lead to the same action.
3. Build one semantic query per decision domain. Put shared resources in `resources[]` and atomic aspects in `questions[]`; use `queries[]` only to batch complete semantic queries.
4. Prefer background `context:{tool,query}` acquisition. Use `context:{value}` only for evidence already present in host context.
5. Inspect every query, cell, and page. Retain incomplete, relevant, uncertain, insufficient, conflicting, or errored resources.
6. Read original retained evidence and run the verification appropriate to the claim.

The active skill uses the flow `DISCOVER IDS → BUILD SEMANTIC QUERY → BACKGROUND ACQUIRE → ASSESS MATRIX → READ RETAINED PROOF → VERIFY`. It includes one single-query example, one batch-of-semantic-queries example, one 80,000-character paged resource example, and one failure/continuation example. It must not copy the complete schema or teach provider-specific terms.

### Dependency-ordered implementation

| Step | Change | Depends on | Pass condition |
|---|---|---|---|
| S1 | Define `SemanticQuery`, direct-or-batch input, stable triple correlation, nested page results, limits, `continuationKind`, native primitive descriptions, lossless answer unions, instructions, examples, relations, and diagnostics in octocode-core. Delete the independent-pair schema. | None | Core lint, typecheck, tests, build, JSON Schema audit, contract-set audit, prompt budget, official Noul/Choice/Score fixture parity, invalid legacy-pair, duplicate-ID, and total-cell cases pass. |
| S2 | Rename native public modules/constants; keep Jev terminology only in the private provider adapter. Implement semantic-query expansion, background capture, frozen source identity, token-aware provider paging, batch isolation, and cross-process `next.assess`; remove all flat-pair and public `jev` branches. | S1 | Native tests prove direct/batch parity, resource-major cells, capture once per logical resource, identical questions across pages, stale-source failure, body-free output, partial sibling return, cancellation, and stable IDs. |
| S3 | Regenerate native contracts from the actual core revision and provenance. | S1–S2 | Fingerprint, embedded body hash, source revision, and dirty flag are truthful; release mode rejects dirty provenance. |
| S4 | Rebuild CLI and MCP from the regenerated contract. Rename commands, catalog rows, help, examples, error paths, and telemetry labels. | S3 | `scheme semanticAssess` and real calls work identically through CLI/MCP; `scheme jev` is unknown and no alias appears in discovery. |
| S5 | Update Pi contracts/prompts/rendering and every skill, Chrome/scraping helper, script, error code, fixture, and test. Helpers create hashed resources but delegate semantic acquisition/paging to the public contract. | S4 | No active caller invokes `jev`; agents use one semantic query or a batch of full semantic queries; every cell/page completes or returns an executable continuation. |
| S6 | Rename active docs and cross-links, repair root/package agent instructions to match live CLI syntax, and label historical artifacts rather than rewriting them. | S4–S5 | Documentation, link, skill, instruction, and style validators pass with one current name, one current usage contract, and executable discovery examples. |
| S7 | Run the frozen agent-behavior evaluation plus direct-provider lab, live local-file, GitHub-file, missing-path recovery, supplied-value, direct semantic query, batched semantic queries, 80,000-character paging, saved-browser, CLI, MCP, and Pi smoke paths. | S4–S6 | Agents select the right shape; direct-provider and adapter answer fields match exactly; every required query/cell/page is correlated/body-free/policy-equivalent; slow siblings do not suppress completed results; held-out safety/quality gates pass; and no stale public name appears. |
| S8 | Run release checks and inspect the published artifact contents. | S7 | Clean provenance, platform checks, package contents, catalog fingerprints, and release documentation all pass. |

### Files, APIs, and contracts

| Area | Required update | No-legacy check |
|---|---|---|
| octocode-core | Tool constants, `SemanticQuery` direct/batch schemas, continuation kinds, exported types, descriptions, instructions, catalog, preparation, output schemas, examples, tests, README. | No public `Jev*`, `jev` tool name, flat pair schema, pair result index, or pair-first guidance. |
| Native runtime | Contract preparation, query/cell expansion, security routing, background context acquisition, source identity, token-aware paging, batch isolation, provider translation, results, tests, architecture docs. | One `semanticAssess` dispatch branch; Jev name stays inside provider code and provider configuration. |
| CLI and MCP | Commands, scheme output, registration, help, rendering, structured output, integration tests. | No alias, fallback registration, stale catalog entry, or hand-written divergent guidance. |
| Repository agent instructions | Root/package `AGENTS.md`, handoffs, and setup snippets use `scheme <tool>` plus direct canonical tool calls from live help. | No active `tools --json`, `tools <name> --scheme`, retired tool-call `--json`, or other known-dead invocation. |
| Pi | Gateway contracts, catalog prompts, rendering hints, harness docs, tests. | No `jev` activation or prose outside historical fixtures. |
| Skills | Rename skill folder/name, triggers, README, references, validators, installed manifests, and internal links; teach single-versus-batch semantics, background acquisition, atomic aspects, and page recovery without duplicating field definitions. | No forwarding skill, legacy pair example, or duplicated schema; live discovery remains the source of truth. |
| Chrome/scraping | Triage invocation, request filenames where public, functions, errors, usage fields, docs, fixtures. Helpers own safe artifact extraction, hashes, and resource construction; `semanticAssess` owns acquisition and semantic paging. | Every artifact enters the public query/page flow; no private semantic paging protocol, `JEV_UNAVAILABLE`, or `runJev` remains in active code. |
| Jev lab | Keep `@octocodeai/jev-lab` private and direct-to-provider for repeatable multi-resource primitive, latency, and token experiments. | It never becomes a public tool, runtime-policy clone, release dependency, or alternative semantic contract. |
| Docs | Rename active Jev guides, headings, commands, examples, ownership links, architecture references, and release notes. | Historical POCs remain immutable and explicitly labeled; active guidance uses only `semanticAssess`. |
| Generated/release | Regenerated Rust/JSON contracts, provenance, fingerprints, package output, platform bundles. | No hand edits; published catalog contains exactly one semantic-assessment tool. |

### Acceptance contract

| Requirement | Pass/fail acceptance | Rollback threshold |
|---|---|---|
| One public tool | All live catalogs expose exactly `semanticAssess`; `jev` is absent. | Any duplicate/alias or interface disagreement blocks release. |
| Executable discovery guidance | Every active agent instruction and skill example executes against the staged CLI/MCP catalog exactly as written. | Any retired command, unknown flag, or stale schema path blocks release. |
| Model-independent tool identity | Public tool/module/schema names contain no Jev model or service branding. The stable Noul/Choice/Score primitive names and exact answer fields remain public and documented. | A provider transport/model term in tool identity, or any rewritten primitive answer, blocks release. |
| Semantic-query correctness | Every question evaluates every resource in its semantic query; cells are resource-major and carry query, resource, and question IDs. | Missing/duplicate cells, repeated acquisition, cross-query leakage, or IDs in provider evidence blocks release. |
| Direct and batch modes | Direct input is one `SemanticQuery`; `queries[]` contains only complete semantic queries and isolates their failures. Output uses one shape for both. | Any accepted flat pair, mixed envelope, lost query ID, or slow sibling that suppresses completed results blocks release. |
| Native primitive fidelity | Noul, Choice, and Score request shapes match the official provider primitives. Successful output preserves every documented answer field/value; outer correlation, coverage, receipts, and usage remain adjacent metadata. | Renaming Noul, dropping distributions/legend/confidence, recalculating Score, thresholding Noul, flattening answers, or synthesizing prose blocks release. |
| Agent routing | Held-out agents skip exact/mandatory reads, add questions for shared-resource aspects, batch only distinct semantic queries, and keep dependent questions sequential. | Any systematic wrong-shape selection, forced-call behavior, duplicated resources, or unread-body defeat blocks release. |
| Context discipline | Unread resources stay outside host context until selected for proof; already-visible evidence uses `value`; compaction preserves identifiers, limitations, and continuations. | Any needless reread, lost recovery handle, or summary-only proof blocks release. |
| Resource paging | One logical resource supports 80,000 characters; runtime pages below 20,000 estimated provider-input tokens, repeats the identical question, freezes source identity, and returns ordered body-free page results or cross-process `next.assess`. | A four-characters-per-token assumption, mixed source versions, silent page/error drop, in-memory-only continuation, replayed completed page, or global negative from partial coverage blocks release. |
| Security and evidence | Hidden bodies remain out of host output; ordinary authorization, redaction, SSRF, path, continuation, and cancellation policies remain active. | Any policy bypass or body/secret leak blocks release. |
| Trust boundary | Retrieved files, pages, tool annotations, and provider text remain data; they cannot alter questions, objectives, permissions, or requested actions. | Any benign or adversarial fixture that changes agent authority blocks release. |
| Measured benefit | Equivalent-workflow evaluation preserves task correctness, citation fidelity, and false-exclusion safety while reporting host/provider tokens, reads, pages, calls, latency, and errors. | Regressed quality/safety or an efficiency claim without production-token evidence blocks the claim and can block the instruction change. |
| Provenance | Generated contracts identify their true core revision and dirty state. | Dirty or mismatched release provenance blocks publish. |

### Agent-behavior evaluation gate

Freeze the baseline prompts, tool catalog, executable hashes, source fixtures, and expected next actions before changing guidance. Run the current instructions and the candidate on the same realistic tasks, reserve held-out variants, inspect raw tool calls/results, and keep a failure ledger by `select`, `call`, `read`, and `act` stage.

| Case | Expected observable behavior |
|---|---|
| Exact known span or required evidence | Zero `semanticAssess` calls; direct source read. |
| Several unread local and GitHub candidates with two shared aspects | One semantic query, stable IDs, complete cross-product, and proof reads only for retained results. |
| Two unrelated decision domains | `queries[]` containing two complete semantic queries; no resource/question leakage across them. |
| One resource set with several aspects | One semantic query with several atomic questions, not duplicated batch elements. |
| Question B depends on answer A | Two sequential calls or direct verification, never one semantic query or batch. |
| Evidence already in host context | `context:{value}` or no call; no duplicate read. |
| Local, GitHub, and saved-browser resources | Background acquisition uses the ordinary tool's policy and returns no source body to the host. |
| One 80,000-character code or minified-JSON resource | Token-aware provider pages repeat the identical question object; all page results retain one cell identity. |
| Mutable local file or moving GitHub branch changes between pages | The cell fails stale rather than mixing source versions. |
| Same-resource continuation | Runtime follows only `continuationKind:"sameResource"`; collection and optional-content continuations remain explicit. |
| Roster/result exceeds the public response budget | `isPartial` and executable `next.assess`; a resumed one-shot CLI call completes unresolved work without lost IDs or replay. |
| Missing local path, missing GitHub path, provider error, and cancellation | No inference on failed retrieval; isolated error row and safe recovery where available. |
| One slow semantic query beside a completed query | Return the completed sibling plus an isolated unfinished error/continuation before the batch deadline. |
| Browser/HAR corpus containing prompt injection and split secrets | Task facts remain usable, instructions remain caller-owned, and no secret/body leaks to host output. |
| Conflicting typed judgments | No averaging or voting; route to direct evidence/test. |

Track task success, tool-selection accuracy, invalid calls, false exclusions, citation fidelity, changed-next-action rate, unnecessary assessments, proof reads, and source bytes kept out of host context. Separately track host and provider tokens, calls, pages, latency, and failures. Count preparation, discovery, retries, continuations, and verification. Provider-token or byte reductions are not host-token savings. Keep the instruction change only when the target behavior improves without unacceptable correctness, security, or latency regression; otherwise revert or narrow the owning rule.

### Live background-context paging evidence (pre-rename)

On 2026-09-20, the staged `jev` CLI processed this plan through three sequential background `localFetch` byte pages: offsets 0, 20,000, and 40,000 with a 20,000-byte limit and the same Choice question. The first two receipts were partial and returned exact `continue` queries; the final receipt reported bounded coverage. Host output contained only typed answers, provider usage, result hashes, coverage, and continuations—not source bodies. The page answers were `partial`, `partial`, and `no`, which confirms that page judgments do not form a safe global answer without complete coverage and an explicit reducer.

A second run assessed the first 80,000 UTF-8 bytes of `packages/octocode-native/crates/runtime/src/contracts/generated/tool-contract.json` as four isolated 20,000-byte pages with the same Noul question. Every page completed and returned the next exact offset. Provider input was 6,642, 8,049, 8,057, and 8,094 tokens: 30,842 total for 80,000 mostly ASCII/minified-JSON bytes. This directly rejects the assumption that 80,000 characters reliably equal 20,000 tokens; token density depends on content and serialization.

The equivalent four-row legacy batch emitted no result within 60 seconds; the test cancelled it with exit code 130. The isolated page calls completed in about 18 seconds when launched in parallel. This does not diagnose the provider/runtime boundary, but it establishes a release case for batch deadlines, partial sibling results, progress diagnostics, and bounded concurrency. The staged CLI still exposes legacy flat `queries[]`, so these runs verify background acquisition, body-free receipts, and manual continuation—not the target `SemanticQuery` direct/batch contract or automatic paging.

### Direct API lab evidence

`@octocodeai/jev-lab` is a private, zero-policy development probe for provider experiments. A manifest accepts either provider-ready `state` or several bounded local `resources`, plus the native TypeSafe `questions` map. It resolves credentials through the shared trusted Octocode config loader, sends local content without local paths, records hashes and sizes, supports repeated/concurrent samples, and keeps each parsed provider envelope unchanged under `samples[].response`. It deliberately does not emulate `semanticAssess` acquisition, paging, security, or output shaping.

On 2026-09-21, `yarn jev:probe --input packages/octocode-jev-lab/examples/multi-file.json --compact` sent this plan, `.octocode/JEV.md`, and the Jev reasoning skill together: 71,404 serialized request bytes, 15,625 provider input tokens, 82 output tokens, HTTP 200, and 1,399.89 ms elapsed. One request asked a Choice, Score, and Noul. The native response retained every documented field: Choice label/distribution/confidence, Score value/distribution/legend/confidence, Noul probability, model, and usage.

The result was `partial_drift` (Choice probability 0.70), implementation readiness 1.71/2 (0.72 on ready), and only 0.19 Noul probability that the plan preserved native answers. The negative Noul was actionable: the plan still renamed Noul to `binary` and did not define the lossless answer union. The contract above now keeps native primitive names and fields. This is a live semantic review, not a source oracle; exact contract tests remain the release gate.

After that correction, the lab's default matrix mode sent each of the three resources once with all three questions, concurrently: nine logical cells, three provider calls, 79,078 serialized request bytes, 17,877 input tokens, 240 output tokens, 801.88–1,331.31 ms per call, and no failures. The updated plan returned `aligned` at 0.65 probability, readiness 1.94/2, and Noul 0.99 for native-answer preservation. The older `.octocode/JEV.md` and reasoning skill returned lower preservation Nouls (0.41 and 0.11), so S5 must update their active guidance; those judgments do not weaken the corrected plan contract.

### Rollout and rollback

Ship the rename in one major-version cutover after S1–S8 pass. Do not stage both public names. Update all first-party callers in the same release and publish a concise migration note mapping old commands and schemas to `semanticAssess`. Roll back by reverting the release as a unit; do not restore a runtime alias. Provider failures can roll back the private adapter independently only when the public `semanticAssess` contract remains unchanged.

## Implementation and POC checkpoint

**Implemented in this working tree:** W1 recovery receipts, A1 continuation ownership, A2 output-policy parity, W2 per-patch guidance, W4 matrix questions/capture reuse, W5 unread-query guidance, and A3 bounded browser/scrape resources. At this checkpoint, the matrix is additive and flat pairs remain compatible; the target contract deliberately removes those pairs. Failed retrieval retains its safe error code and body-free receipt, states that inference did not run, and has no answer or provider usage. Nested continuations and direct/background sanitization remain unchanged.

The pre-cleanup integration checkpoint passed 205 core tests, lint/typecheck, 323 native library tests and 19 Jev integration tests; native binary/addon and CLI/MCP builds completed. The recorded CLI/MCP smoke checkpoint covered recovery, executable continuations, and two real-provider successes, but its former `.octocode/tmp/jev-integration-smoke/after-live.json` receipt is not present in this checkout. Controlled tests inspect outgoing sanitized state and zero inference on failed retrieval. This is the named verification scope, not every monorepo suite.

Initial regeneration failed on dirty-core provenance and four outdated default-field expectations. The corrected tests preserve rejection of missing operation/URI anchors while accepting intentional defaults. Final regeneration used clean core provenance and passed; the guards were not weakened. Those results describe that checkpoint, not every later working-tree revision.

## RFC delta: resource-question matrix (pre-rename implementation)

### Decision

The pre-rename implementation adds one envelope for a true cross-product. The following pre-rename example records that checkpoint; it is not the target public input. The target adds the required semantic-query `id`, accepts this object directly or inside `queries[]`, and removes the checkpoint's flat-pair branch.

```json
{
  "reasoning": "Why this matrix changes the next action.",
  "resources": [{"id": "resource-1", "context": {"tool": "localFetch", "query": {}}}],
  "questions": [{"id": "relevance", "question": {"type": "noul", "instructions": "Is it relevant?"}}]
}
```

Every question is evaluated against every resource in resource-major order. Result rows expose `resourceId` and `questionId`. IDs are 1–64 ASCII letters, digits, dots, underscores, or hyphens and start alphanumeric. Resource and question IDs must be unique. This checkpoint admits at most 25 cells, 25 resources, and five questions. Its `queries[]` branch contains independent pairs correlated by `index`; S1 deletes that branch and reuses `queries[]` only for complete `SemanticQuery` objects.

### Runtime flow

1. Core owns descriptions, limits, input/output schemas, and matrix-preserving preparation.
2. Native validates the original envelope, enforces refinements absent from JSON Schema, trims shared reasoning, and expands resource-major rows.
3. Security validates every expanded row. Correlation IDs never enter provider state or question instructions.
4. Flat pairs retain fresh nested reads. Matrix rows cache the exact `(resourceId, canonical context)` capture within the call, including a shared retrieval failure, so one resource is never fetched once per question.
5. Identical captured states group for provider inference. Groups split automatically under the existing 24 KiB state-plus-question and 48 KiB combined headroom; independent groups remain concurrency-bounded.
6. Output preserves ordered indexes, row isolation, receipts, usage attribution, and correlation IDs.

### Large-resource pagination

At the pre-rename checkpoint, `jev` does not silently follow a source tool continuation because doing so changes evidence scope, and it does not truncate one oversized state. Callers represent large files, browser bodies, and HAR-derived artifacts as bounded resources. At that checkpoint, the scraping/Chrome bridge supplies the loop: it resolves every manifest part, splits UTF-8-safe byte chunks, submits successive matrices of 1–25 resources, and aggregates only after every chunk returns. Its `--limit` controls matrix page size; the loop tracks the complete discovered resource roster and reports missing or errored rows. The target keeps safe extraction, canonical paths, hashes, and resources beyond the 80,000-character logical cap in the bridge; `semanticAssess` owns same-resource acquisition continuations and provider paging.

Aggregation is conservative and exclusive: any relevant chunk routes the candidate to `read`; otherwise partial, insufficient, mention, error, or low-confidence unrelated routes to `consider`; `skip` requires every chunk to be covered and confidently unrelated. Thin browser shells bypass Jev and stay in `consider`. Canonical path containment rejects explicit paths and symlink targets outside the scrape session.

### Rejected alternatives

- A separate semantic-assessment wrapper duplicates validation, security, dispatch, receipts, and output semantics.
- Repeating flat context for every question preserves compatibility but repeats mutable retrieval, so questions can observe different resource states.
- Auto-following continuations hides evidence-scope changes and can convert bounded retrieval into an uncontrolled crawl.
- Sending whole HAR/browser dumps makes provider limits and false negatives depend on arbitrary truncation.
- Returning answer maps instead of rows weakens row isolation, usage attribution, and existing interface rendering.

### Pre-rename acceptance record

These checkpoint bullets preserve the earlier matrix scope. The target acceptance contract and S1–S8 above supersede them.

- Core: union schema, matrix refinements, descriptions, prompt budget, JSON Schema export, and correlated output validate.
- Native: resource-major expansion, 25-cell rejection, duplicate-ID rejection, provider-payload non-leakage, one background capture across questions, fresh flat repeated captures, ordered IDs, and isolated failures pass.
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

### Frozen real-provider evaluation (2026-09-21)

The [real-API suite](../.octocode/octocode-eval-benchmark/jev-real-api-2026-09-21/REPORT.md) exercised structured Noul, Choice, and Score; a two-resource by three-question matrix; CLI batching; and four exact pages over one 78,377-character local resource. It made an estimated 13 provider calls with no request failures. All 23 held-out semantic assertions passed, direct-provider p95 was 861.71 ms, and total direct plus CLI-reported usage was 29,070 input / 561 output tokens.

The verdict is **CONTINUE**, not release acceptance. The built CLI still exposes legacy independent `context + question` queries instead of `semanticAssess` `resources[] × questions[]`, so the target automatic paging flow is not interface-verifiable. The adapter also reports requested `jev-latest` where the direct provider returned resolved `jev-1.13.0`; preserve requested and resolved model provenance separately. Native answer fields, structured Score legends, ordered batch rows, exact continuations, bounded coverage, and compact-output body non-leakage passed.

The implementation should keep the provider's documented native answers unchanged, expose page-level judgments, and avoid generated explanations or hidden chain-of-thought. Background acquisition is the differentiator: capture and sanitize each resource once, submit every independent question together, page the same logical resource with unchanged questions, and read exact proof after scouting. No page-local absence may become global absence without an explicit reducer.

[POC v1](../.octocode/jev-poc-2026-09-20/REPORT.md) preserved a failed compound-question exact-label gate; uncertain candidates were retained. [Separately frozen v2](../.octocode/jev-poc-2026-09-20/v2/REPORT.md) passed six atomic classifications, retaining four relevant candidates and excluding two irrelevant windows from routed proof. Separate verifier reads count. Generated negatives could have been excluded by ordinary path filtering. These development probes do not establish blind adoption, actual host-token savings or the effect of core instructions built after the POCs. No new Octocode-only baseline ran.

## Review disposition

“Shipped” refers to the checkpoint above. Proposed fields and admission changes below are **not current contracts**.

| ID | Priority | Disposition | Remaining work |
|---|---|---|---|
| W1 | P0 | Shipped: hidden-read recovery | Preserve failure/continuation regressions when receipts evolve. |
| W2 | P0/P1 | Shipped: PR `charLength` is explicitly per file patch | Verify multiple patch/page axes and revision drift; any aggregate cap needs a separate ordinary-tool contract decision. |
| W3 | P1/P2 | Implemented at checkpoint: matrix capture reuse | Preserve capture-once behavior while deleting the flat-pair branch; add frozen multi-page source identity. |
| W4 | P1 | Implemented at checkpoint: resource-question matrix | Promote the matrix to `SemanticQuery`, add query IDs/direct-or-batch envelopes, and replace flat output rows with logical cells plus page results. |
| W5 | P1 | Shipped: unread-query instructions | Keep real CLI/MCP discovery examples valid; `value` is supplied evidence, not path loading. |
| W6 | P1 | Proposed: auditable scope receipts | Define bounded deterministic completion/count metadata without claiming global completeness. |
| W7 | P1 | Target decided: strict question admission | Enforce nonblank strings, explicit criteria, Choice minimum two, scalar wrapping, and indexed diagnostics in S1; provide no compatibility branch. |
| W8 | P1/P2 | Shipped: independent-answer/conflict guidance | Reproduce the reported live disagreement; automated consistency requires a relation contract and evaluation. |
| A1 | P0 | Shipped: nested continuation ownership | Preserve outer/nested trace separation and ordinary-tool behavior. |
| A2 | P0 | Shipped: background/direct redaction parity | Keep output policy before provider transmission, hashing, and reuse. |
| A3 | P1/P2 | Implemented at checkpoint for saved browser/scrape files | Keep safe extraction/hashes in helpers; move same-resource and provider paging into `semanticAssess`. |
| A4 | P1 | Pending: failure/packing accounting | Audit known versus unknown usage on failures and cancellation; measure actionable grouping diagnostics. |
| A5 | P1 | Unverified: stale discovery/error hints | Reproduce any remaining retired hints in current CLI help/errors, then repair the owning layer. |

## Open work and acceptance

### Scope receipts and pagination — W2/W6

Keep existing `tool`, `resultHash`, `coverage`, `next` and limitations compatible. A receipt describes the requested view, not repository completeness. Add fields only where deterministic source metadata changes the next action; candidates include requested/returned ranges, resolved revision, source-reported counts and `scopeStatus:complete|partial|unknown`. An explicit evaluation discriminator is also a proposal, not a shipped field or a prerequisite for the existing recovery fix.

Never infer completion from missing `next`, hash equality, bounded coverage or confidence. Omit unknown counts instead of reporting zero. A completed comments surface says nothing about inaccessible reviews or inline threads. Exact counts already answering a question should come from ordinary tools without inference.

PR patch windows and file pages are separate axes. The review recorded 87,419 patch characters across 30 files with `charLength:4000`; that field limits each patch, not the aggregate. Prefer changed-file metadata, selected hidden patches, then deciding source at the captured revision. Pinning only the final source read does not prove the earlier mutable patch matched it.

**Acceptance:** complete/partial/empty/unknown/error views remain distinguishable; all valid continuation axes execute under normal policy; metadata has a bounded budget; oversized metadata produces an explicit limitation rather than dropped recovery. Cover selected/all/default patch modes, binary/unavailable patches, multiple active axes, secret redaction, real interface response caps and PR-head changes. Any aggregate cap belongs in the ordinary history tool and must preserve executable recovery. Do not silently redefine `charLength`, crawl pages or average page judgments.

### Admission and semantic agreement — W7/W8

The hard cutover resolves the admission decision: reject null/blank instructions, require at least two Choice alternatives and 2–10 independently described Score levels, and validate optional Noul criteria as a complete `true`/`false` pair. Accept nonempty strings, objects, or arrays for instructions and the provider-supported JSON entry shapes for criterion descriptions; recommend string form first. Accept strings, nonempty objects, or nonempty arrays for supplied state. Wrap scalars explicitly, for example `context:{value:{observed:false}}`; do not silently transform caller values. Validate background queries through their canonical ordinary-tool schema rather than embedding every read-tool schema in `semanticAssess`.

Publish the break in the migration note, regenerate contracts, and make invalid examples assert rule IDs plus indexed `queries`, `resources`, and `questions` paths. Do not add keyword bans or provider-specific validation.

The newer review reports a **live semantic disagreement**: 496 input/79 output tokens, with Score assigning 0.96 probability to “no implementation evidence” while Noul/Choice and the function indicated otherwise. Its raw receipt was not located and the claim is not independently reproduced. A different, older Score contradiction was an intentionally malformed mock response whose live smoke passed. The mock result does not dismiss the newer report.

**Acceptance:** freeze a reproducible live case and source oracle before changing semantic behavior. Mathematical validation of labels, distributions, legends and Score expectation cannot establish agreement between differently worded questions. Preserve independent answers; resolve consequential conflicts through evidence/tests. Automated consistency needs explicitly declared relationships and held-out benefit. Do not silently vote, average, rewrite answers or add paid judge calls.

### Capture reuse and accounting — W3/A4

Measure capture, cache, sanitization and inference costs separately. Identical captured state can already share inference; repeated nested reads still execute independently. A matching request JSON or result hash is not a freshness proof or retrieval capability. The GitHub cache's default 32 MiB limit is a memory budget; disk retention is bounded by entry count, not that aggregate byte ceiling (defaults: 1,000 entries, 300-second TTL).

A within-call prototype must key eligibility on canonical selection, established source identity, effective availability/path policy, redaction policy, endpoint, credential/session partition and all output-affecting options. Preserve row-specific trace metadata, bound retained bytes, and release on completion/cancellation. Pinned immutable content is a candidate; local size/mtime, moving branches, live searches and LSP state are not immutable identities. Preserve the moving-branch regression unless an explicit snapshot contract is designed. Do not strip freshness metadata merely to force grouping.

Public context handles remain gated on demonstrated cross-call benefit and a lifecycle design: ownership, expiry/eviction, memory limits, policy revalidation, credential/endpoint partitioning, source-version checks, invalidation, restart and stale-handle errors. Keep content reuse separate from judgment caching. A recorded live design Choice favored bounded internal reuse, but its former `.octocode/tmp/jev-improvement-plan/reuse-design.output.json` receipt is not present in this checkout. Treat the record as unverified decision input, not proof of safety or performance.

**Acceptance:** unchanged or better task quality and coverage, measured capture reduction, no mutable-state or policy cross-contamination, and accurate per-group usage. Audit failed-only/mixed groups, packing fallback, cancellation and duplicate accounting; distinguish unknown usage from zero. Existing completed-usage behavior is not a reason to claim every failure-accounting case is covered. Discard reuse if overhead or changed semantics outweigh its benefit.

### Artifact workflows and shared acquisition — A3

Use the [semantic assessment research guide](SEMANTIC_ASSESS_RESEARCH_GUIDE.md) for supported read families; do not duplicate its matrix here. For RFCs, saved articles, and browser artifacts, expose a small visible manifest, build background resource descriptors, then read selected proof. Supplied alternatives can use `value`. `semanticAssess` can select sections or assess a supplied summary; its primitives do not generate free-form summaries.

Use the existing [HAR capture guide](../skills/octocode-chrome-devtools/references/har-capture.md) and [ingestion bridge](../skills/octocode-chrome-devtools/scripts/har-ingest-to-scrape.mjs) to produce bounded records or selected response bodies. Skills produce hashed artifacts; the pure CLI/MCP tool acquires them in the background through ordinary `localFetch`. Do not create a provider-only acquisition path.

The pre-rename `localFetch` contract has distinct limits: 10 MiB source acquisition, 100 KiB raw full-content preflight, and a 50,000-byte complete-view cap. It acquires source before extraction; selecting a range does not bypass the source ceiling. The target reaches an 80,000-character logical resource only through validated same-resource continuations and token-aware provider pages; it does not raise the ordinary one-page limit. Huge single-line HAR files can exceed scanner budgets, so use existing bounded artifact slicing before shared streaming/structured acquisition.

**Acceptance:** representative public/synthetic RFC and HAR tasks complete with cited proof; supported size/format limits are explicit; UTF-8, record boundaries, cancellation and applicable full scanning are preserved. Verify redaction of Authorization/Cookie/Set-Cookie, URLs/query parameters, post bodies, malformed/base64 payloads and split secrets using synthetic fixtures. A locally readable session file is not automatically safe to transmit.

## Ownership and delivery gates

| Area | Owning source |
|---|---|
| Input descriptions and validation | [Core semantic assessment schema](../../octocode-mcp-host/packages/octocode-core/src/toolContract/validation/semanticAssess.ts) |
| Agent guidance and descriptions | [Core instructions](../../octocode-mcp-host/packages/octocode-core/src/toolContract/instructions.ts), [tool descriptions](../../octocode-mcp-host/packages/octocode-core/src/toolContract/descriptions.ts) |
| Output contract | [Core output schemas](../../octocode-mcp-host/packages/octocode-core/src/toolContract/outputSchemas.ts) |
| Capture, failure receipts and grouping | [Context execution](../packages/octocode-native/crates/runtime/src/runtime/jev_context.rs), [batch execution](../packages/octocode-native/crates/runtime/src/runtime/jev_batch.rs) |
| Shared output policy and continuation ownership | [Response finalization](../packages/octocode-native/crates/runtime/src/runtime/response.rs) |
| Ordinary retrieval and cache | [Dispatcher](../packages/octocode-native/crates/runtime/src/runtime/domain_dispatch.rs), [GitHub content provider](../packages/octocode-native/crates/runtime/src/providers/github/content.rs) |

Implement the `SemanticQuery` and continuation contracts before background acquisition, paging, and measured reuse. Keep public context handles and automatic judgment reduction separately gated. For each production slice: change owning sources, regenerate contracts, rebuild native and interfaces, inspect real CLI schema/help and stdio MCP discovery, then exercise changed behavior. Do not hand-edit generated contracts or weaken provenance, security, lint, or coverage guards. Fingerprint isolated executables before paid evaluations to avoid shared-build races.

## Measurement and completion

Keep the existing Octocode-only baseline. Do not launch another without a new user instruction. Candidate-only probes establish mechanics and routing; unmatched historical runs cannot establish causal savings.

Freeze natural tasks and source oracles covering optional large files, all-relevant controls, cheap exact lookups, repeated questions, mutable sources, recoverable/terminal failures, PR paging axes, RFC/HAR triage, supplied alternatives and conflicting judgments. Reserve unseen artifacts for adoption/quality validation. Keep failures and setup costs; never add a call quota or benchmark-only Jev recipe.

Measure actual host input/cached/output tokens separately from tool-wire proxies and provider usage. Include discovery, preparation, retries, background captures, proof reads, and verification. Track task quality, citation fidelity, false exclusions, changed next actions, known/unknown cost, and recovery. Reading every candidate after screening is not a read-saving success. Reasoning benefit is a separate outcome. Compare equivalent complete workflows against targeted ordinary tools, not only read-all.

Mocks establish contract, policy and scheduling properties; real provider/agent trials establish semantic quality and adoption. The plan is complete when each review item has a verified disposition and equivalent-workflow evidence supports every claimed benefit. The first shipped slice does not imply universal consistency, arbitrary-size ingestion or public context handles are complete.
