Load when designing or auditing a tool, MCP server instructions, tool names, descriptions, schemas, results, wire behavior, errors or pagination, the reason attached to tool calls, or skill discovery and loading.

# Tools and MCP

## Contents

- [Capabilities and triggers](#capabilities-and-triggers)
- [MCP server design](#mcp-server-design)
- [Layer owners](#layer-owners)
- [Instructions, descriptions, names](#instructions-descriptions-names)
- [Schema and result](#schema-and-result)
- [Tool output design](#tool-output-design)
- [Reference-first responses](#reference-first-responses)
- [Pagination contract](#pagination-contract)
- [Actionable errors](#actionable-errors)
- [Reason-aware invocation](#reason-aware-invocation)
- [Unified tool contract](#unified-tool-contract)
- [Skills and progressive disclosure](#skills-and-progressive-disclosure)
- [Efficient MCP context](#efficient-mcp-context)
- [MCP wire](#mcp-wire)
- [Set audit](#set-audit)
- [Sources](#sources)

## Capabilities and triggers

| Component | What it represents | Trigger |
|---|---|---|
| Tool | Executable capability | Required external data or action |
| MCP server | Standardized capability provider | Relevant connected service |
| MCP resource | Readable contextual data | Need to inspect external context |
| MCP prompt | Reusable prompt template | User or workflow invokes it |
| Agent Skill | Domain procedure and references | Task matches the skill's purpose |
| Subagent | Delegated worker with its own context | Bounded independent subtask |
| Workflow | Predetermined sequence or graph | Known repeatable process |
| Hook | Event-triggered processing | Lifecycle event |
| Policy / guardrail | Constraint and validation | Before or after selected actions |
| Scheduler | Time- or event-based invocation | Scheduled trigger |
| Router | Chooses capabilities | Task classification |

When the agent should call what is in [retrieval and grounding](retrieval-and-grounding.md#when-to-call-a-tool).

## MCP server design

| Concern | Good design |
|---|---|
| Naming | Clear namespace and action verbs |
| Discovery | Concise descriptions and typed schemas |
| Tool selection | Distinct capabilities, minimal overlap |
| Input validation | Reject invalid or unauthorized arguments |
| Output schema | Structured and predictable |
| Error semantics | Typed, actionable failure results |
| Pagination | Opaque cursors and bounded responses |
| Filtering | Server-side filtering and projection |
| Authentication | Permissions scoped by identity |
| Authorization | Enforced at execution time |
| Idempotency | Safe retries where applicable |
| Cancellation | Stop unneeded long-running work |
| Timeouts | Explicit execution deadlines |
| Observability | Enough evidence to diagnose failures and support measurements |
| Versioning | Stable contracts and compatibility |
| Security | Server-delivered content is untrusted |

Good agent tools return what a decision needs, not a thin wrapper around a large API payload.

## Layer owners

A rule in the wrong layer is never read; give each rule one owning layer.

| Layer | Read when | Owns (nothing else) |
|---|---|---|
| Server instructions | Before the first call | Cross-tool order, shared conventions, lifecycle, approval and trust boundaries |
| Tool name + description | Choosing a capability | Its job and distinction from neighboring tools |
| Operation description, when a tool has variants | Choosing an operation | Behavior, result meaning, side effects and next step |
| Input/output schema + field notes | Filling a call, reading a result | Structural constraints; values, units, defaults, scope, dependent fields and continuation |

Octocode tools: edit `@octocodeai/octocode-core`, which owns tool names, schemas, descriptions and shared MCP context; skills point to live discovery, never a second schema.

## Instructions, descriptions, names

- Hosts may shorten loaded instructions or descriptions. Put routing and trust boundaries first and inspect what the target host actually serves; consult current documentation for its limits.
- Instructions reach MCP agents only, from the first turn even where tool schemas load deferred; CLI agents and hosts that drop instructions skip them, so a field an agent must fill keeps its own note, and CLI help carries its own routing.
- Server instructions explain dependencies and what the agent cannot infer (refusals, approvals, what empty proves). Include routing only when it resolves a cross-tool decision; do not repeat the tool catalog.
- Descriptions give a capable new hire the selection or behavior information missing from the name and schema. A simple tool may need one sentence; add an example or next step only when it prevents ambiguity.
- Names express the capability or action (`messages`, `repo_search_code`, `issue_get`); `search` filters, `get` takes a known ID, `list` browses bounded sets, mutation verbs change state. No near-synonyms unless evals show agents distinguish them.
- Pass tools in the API tools field, not prompt text.

## Schema and result

- Fields are unambiguous (`user_id`, not `user`), with one name, unit, encoding, cardinality, sensitivity and owner each; constrain ranges, enums, lengths and incompatible combinations; describe when to set, what omission does and conflicts.
- Exclusive branches use a discriminant (`kind`/`status`) and strict field set per branch. Define absent, `null`, empty, default and unknown-field behavior; use strict mode where it fits; validate again where the call executes.
- Results have a bounded default, action-relevant fields first, and visible security and capability diagnostics; opaque IDs or raw payloads only for evidence or continuation; `response_format: concise | detailed` only for a distinct evidence need.
- [Errors](#actionable-errors) provide a corrected call; [pagination](#pagination-contract) preserves all selected evidence and makes incomplete scope visible.

## Tool output design

| Poor response | Better response |
|---|---|
| Entire repository tree | Relevant paths, counts and matching modules |
| Full build log | Status, errors, first actionable locations, artifact reference |
| Entire database rows | Requested fields, filtered records, cursor |
| Unbounded search hits | Ranked top results and `hasMore` |
| Large file | Symbol metadata and relevant ranges |
| Full test output | Pass/fail counts, failures, log reference |
| Raw exception flood | Typed failure, cause, retryability |
| Entire web page | Title, relevant excerpt, source reference |
| Full subagent transcript | Evidence-backed summary and outstanding risks |

Return action-relevant results, source locations, completeness and recovery information. Include counts, revisions and diagnostic details when they support the task; a larger payload is not more progress.

## Reference-first responses

Tool results should not automatically become model context. Keep the full result in a store and give the model a compact description plus a way to read more.

```text
Tool runs the analysis (repository scan, large build, database query)
  → Artifact store keeps the complete output under a stable reference
  → Agent receives status · summary · relevant findings · resource ID · next actions
  → Agent reads a range, symbol, page or artifact only if needed
```

A reference must resolve, be scoped to the right identity and live as long as the workflow needs it. A file path alone does not prove the file exists or is current. Default: return a summary and a reference; inline raw data only when it is small and needed.

## Pagination contract

Design pagination into search, listing, scanning and retrieval tools from the start.

1. Bounded default page sizes with server-enforced limits.
2. Opaque cursors over offsets for frequently changing data.
3. Filtering, projection and stable sort order.
4. Source revision or snapshot identity when consistency matters.
5. Direct requests for a specific range, file, record or symbol.
6. Return complete pages with executable continuations; every `hasMore` needs a reachable continuation. Never silently truncate evidence or claim completeness from a partial page; a terminal limit or denied scope is explicit.
7. Tiny results may return whole; pagination is not mandatory for them.
8. Continuations and warnings name what remains: a lead carrying more entries is not named after an enrichment; a warning never implies completeness.
9. Required follow-ups (next page, restart after a stale snapshot) are required leads, never optional items. Server instructions state how a complete result looks, for example no `hasMore` and no continuation.
10. Partial results are visible, for example `isPartial` with reasons; instructions say what the agent may conclude from them.

MCP list operations use opaque cursors: clients follow `nextCursor` and never parse it. Consumer rules are in [retrieval](retrieval-and-grounding.md#pagination).

## Actionable errors

An error should tell the agent whether to retry, change the request, inspect another resource or stop. State what failed and give the corrected call: a generic error → `path must be workspace-relative; retry with path: "src/a.ts"`.

- A misplaced field reports where it belongs, for example: `cursor is the whole input; send {"cursor":"…"} alone`.
- Drop errors that only follow from the first one, such as a row that "lacks" its own fields because it was never a row.

| Code | Agent response |
|---|---|
| `INVALID_ARGUMENT` | Correct parameters using the schema |
| `RESOURCE_NOT_FOUND` | Check the identifier or search alternatives |
| `AMBIGUOUS_MATCH` | Narrow the scope |
| `PERMISSION_DENIED` | Stop or request authorization |
| `RATE_LIMITED` | Respect retry-after and budget |
| `TIMEOUT` | Smaller scope or another query |
| `RESULT_TOO_LARGE` | Add filters, page or request an artifact |
| `STALE_CURSOR` | Restart from a consistent snapshot |
| `CONFLICT` | Refresh state before modifying |
| `DEPENDENCY_UNAVAILABLE` | Bounded fallback, if valid |
| `INTERNAL_ERROR` | Retry only where appropriate |
| `PARTIAL_SUCCESS` | Keep completed work, address the rest |

Separate the **error** (what failed), **cause** (the observable condition), **hint** (a valid next step), **retryability** (is repeating the same call safe) and **evidence** (partial result or diagnostic artifact). Hints are suggestions, never authorization ([security](security.md#risks-and-mitigations)): a tool must not override the agent's security or task constraints.

## Reason-aware invocation

For a consequential call, identify the missing information or authorized effect it serves. Use the tool's real input fields; do not add a universal intent envelope or demand long rationales for trivial calls. Capture timing, usage or correlation only when diagnosing a failure or measuring that dimension.

A model-written reason proves neither authorization nor usefulness; enforce authorization at execution ([security](security.md#risks-and-mitigations)).

## Unified tool contract

Generate each application's tool surface from its owning definition; keep inputs, results, continuation and error semantics consistent across adapters. Add only fields the consumer uses and test the actual boundaries. There is no mandatory cross-tool request or diagnostic envelope.

## Skills and progressive disclosure

An illustrative layout for a hypothetical architecture-review skill:

```text
architecture-review/
├─ SKILL.md          short entry: when to use, next action
├─ references/       detail for one task branch each
├─ scripts/          deterministic operations, run rather than read
└─ assets/           templates a step consumes
```

1. **Discover:** read the name and description to judge relevance.
2. **Activate:** read `SKILL.md` only when the task matches.
3. **Execute:** open a reference, script or template only when a step needs it.

A strong description says when to use the skill, what it distinguishes from neighbors, inputs and outputs; no skill should claim every engineering task. Remote or MCP-served skill content crosses a prompt-injection and execution-trust boundary: treat it as untrusted. Authoring and checking skills belongs to `octocode-skills`.

## Efficient MCP context

Optimize successful work per loaded context, not tool count or character count alone.

### Measure the served surface

Run [mcp-surface.mjs](../scripts/mcp-surface.mjs) from the directory the server expects; `--help` lists options, outputs and exit codes.

```bash
node <skill-folder>/scripts/mcp-surface.mjs capture --cmd '<server command>' --out <round>/before.json
node <skill-folder>/scripts/mcp-surface.mjs size <round>/before.json
```

- `capture` writes the served initialize result and every `tools/list` page to a new file and stops the server; exit 1 means no capture: the printed error code (`serverExited`, `rpcError`, `protocolError`, `timeout`) and the server's `stderr` say why. Freeze the build for each comparison and keep the raw captures. Inspect the host's projected tool definitions separately: they can differ from the served list.
- `size` reports UTF-8 bytes per tool and `totals` (`toolBytes`, `instructionsBytes`, `surfaceBytes`, `schemaProseBytes`). Compare the same profiles, authorization and lifecycle states; separate variable identity/path metadata from static guidance so it cannot distort savings.
- Report bytes as bytes. Token claims need the production tokenizer or provider count; cost claims need usage and retry costs ([context metrics](context-and-memory.md#context-budget)). A smaller definition is not evidence of better task success.

### Remove duplication, retain decisions

- Give each rule one [owner](#layer-owners). Remove repeated routing, boilerplate and explanations already carried by constraints. Keep purpose that names cannot convey: `wholeWord` may still need “Use for identifiers.”
- Let types, required fields, enums and branch constraints express structure. Field notes explain non-obvious values, units, omission/default behavior and ID scope; do not recite every allowed value. Keep distinctions such as acknowledgement versus completion, timeout versus failure, and record ID versus message ID when the runtime has them.
- Generate schemas from one definition. Shared source definitions do not guarantee smaller served schemas: adapters may inline them. Factor references or unions only when receiving hosts support them and measured exposure improves; preserve branch validation and execution-time checks.
- Prefer clear language to cryptic aliases, abbreviated fields or a second schema hidden in prose. Preserve evidence and executable continuations; a context target never justifies truncation or weaker validation.

### Choose a small, coherent tool set

- Merge aliases and overlapping operations when their purpose, lifecycle and authority agree. Related actions may share a typed family, for example `{type:"read",input:{...}}`, with exact fields per variant. Keep unrelated jobs separate; avoid an unrestricted `execute` tool or one object full of optional fields.
- Tool-level annotations and approval policies must remain accurate for every variant. Split a family when combining reads and mutations obscures permissions or recovery; fewer names alone is not a gain.
- Use supported profiles or scopes to expose only relevant operations. Descriptions and instructions must not promise hidden capabilities. Bootstrap tools and changing catalogs require the negotiated revision and client to support that discovery model; configuration files alone do not prove active hooks or identity binding ([MCP wire](#mcp-wire)).

### Verify retained behavior

- For wording-only edits, capture after the edit and run `node <skill-folder>/scripts/mcp-surface.mjs diff <round>/before.json <round>/after.json`. Exit 0 means names, requiredness, defaults, bounds, enums, types, composition and annotations are unchanged; exit 1 lists each `changes[]` row (`tool`, `path`, `kind`) to revert or justify; `prose.tools` names the tools whose wording changed. Check affected CLI help and host projections as well as MCP calls.
- For changed tool surfaces or substantial rewrites, use [cold interviews and live replay](interview.md): selection, request/result handling, invalid combinations, hidden capabilities, retries, empty pages, continuations and untrusted content. Execute proposed calls; valid syntax or self-reported confidence is not enough.
- Keep reductions that preserve required decisions and recovery. Report measured size and observed outcomes separately; use held-out evaluation for comparative reliability or cost-per-success claims. Do not turn a few successful checks into a general model-performance claim.

## MCP wire

- Follow the [MCP specification](https://modelcontextprotocol.io/specification/latest) revision the peer negotiated; never merge revision-specific fields (long-running calls, multi-round-trip input, header mirroring); keep the older branch while peers use it.
- Each tool has a unique case-sensitive `name` and object `inputSchema` even with no arguments; a declared `outputSchema` binds `structuredContent`.
- Follow the `tools/list` cursor; cache hints only where the revision defines them.
- Separate protocol errors from actionable tool errors; annotations are [untrusted hints](security.md#untrusted-content), not authorization.
- Cross-call state uses server-minted opaque [handles](agent-communication.md#schema-rules), reauthorized on every use; no implicit state.
- Definition or order change: new catalog version, refreshed discovery, rerun set audit, recheck [frozen prefixes](models-and-delegation.md#frozen-base-prompt); never hot-patch one worker's cached copy.

## Set audit

1. Inventory from the published view the client receives: a `mcp-surface.mjs capture` file ([measure](#measure-the-served-surface)), not an author, enforcement or catalog view that also holds private fields. Record each tool's name, one-line job and the layer that states it. A job stated nowhere is unroutable.
2. Per close pair, write the deciding sentence; if you cannot, merge, rename or add the condition to both descriptions.
3. Compare shared fields (name, type, unit, default, requiredness, scope); generate equivalents from one definition; document real differences.
4. Check each pair for drift; a `Next` prerequisite must match the target's required fields. Class (symptom) → repair:
   - Split owner (one rule in two layers) → one owner; delete the copy.
   - Name drift (`path` vs `directory` for one input) → one canonical name.
   - Type drift (required here, optional there) → align or document.
   - Semantic drift (`page` 1-based here, byte offset there) → one meaning per name.
   - Enum drift (guidance suggests a rejected value) → validate against the schema.
   - Phantom next (missing tool, field or mode) → fix or delete it.
   - Phantom output key (a result key the tool never emits or spells differently) → name the emitted key.
   - Silent failure (disagree on what empty proves) → state it once in server instructions.
   - Authority drift (description implies a forbidden mutation) → server instructions win.

Saved audit: a Findings row per tool (job, overlaps, finding, class, repair, severity), shared fields with whether one definition owns them, and contradictions needing an owner decision. An unexamined tool is a gap.

## Sources

- [Anthropic — Writing effective tools for AI agents](https://www.anthropic.com/research/writing-tools-for-agents)
- [Model Context Protocol specification](https://modelcontextprotocol.io/specification)
- [Agent Skills specification](https://agentskills.io/specification)
