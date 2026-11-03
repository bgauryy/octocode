Load when writing or auditing MCP server instructions, tool names, descriptions, schemas, results, or wire behavior, or when a server has more than one tool.

## Layer owners

A rule in the wrong layer is never read; give each rule one owning layer.

| Layer | Read when | Owns (nothing else) |
|---|---|---|
| Server instructions | Before the first call | Cross-tool order, shared conventions, lifecycle, approval and trust boundaries |
| Tool name + description | Choosing a capability | Its job and distinction from neighboring tools |
| Operation description, when a tool has variants | Choosing an operation | Behavior, result meaning, side effects and next step |
| Input/output schema + field notes | Filling a call, reading a result | Structural constraints; values, units, defaults, scope, dependent fields and continuation |

- Octocode tools: edit `@octocodeai/octocode-core`, which owns tool names, schemas, descriptions, and shared MCP context; skills point to live discovery, never a second schema.

## Instructions, descriptions, names

- Host caps cut text silently: Claude Code keeps 4,096 characters of server instructions and of each loaded description (2,048 before v2.1.296). Put routing and trust boundaries first.
- Instructions reach MCP agents only. Deferred tool loading and CLI agents skip them, so a field an agent must fill keeps its own note, and CLI help carries its own routing.
- Server instructions: explain dependencies and what the agent cannot infer (refusals, approvals, what empty proves). Include routing only when it resolves a cross-tool decision; do not repeat the tool catalog.
- Descriptions: give a capable new hire the selection or behavior information missing from the name and schema. A simple tool may need one sentence; add an example or next step only when it prevents ambiguity.
- Names: express the capability or action (`messages`, `repo_search_code`, `issue_get`); `search` filters, `get` takes a known ID, `list` browses bounded sets, mutation verbs change state. No near-synonyms unless evals show agents distinguish them.
- Pass tools in the API tools field, not prompt text.

## Schema and result

- Fields: unambiguous (`user_id`, not `user`), one name, unit, encoding, cardinality, sensitivity, owner each; constrain ranges, enums, lengths, incompatible combinations; describe when to set, what omission does, conflicts.
- Exclusive branches: discriminant (`kind`/`status`), strict field set per branch; define absent, `null`, empty, default, unknown-field behavior; strict mode where it fits; validate again where the call executes.
- Results: bounded default, action-relevant fields first, security and capability diagnostics visible; opaque IDs or raw payloads only for evidence or continuation; `response_format: concise | detailed` only for a distinct evidence need.
- Errors say what failed and give the corrected call: `Error 400` → `path must be workspace-relative; retry with path: "src/a.ts"`.
  - A misplaced field reports where it belongs, for example: `cursor is the whole input; send {"cursor":"…"} alone`.
  - Drop errors that only follow from the first one, such as a row that "lacks" its own fields because it was never a row.
- Large results return complete pages with executable continuations; a terminal limit or denied scope is explicit. Never silently truncate evidence or claim completeness from a partial page.
- Continuations and warnings name what remains:
  - A lead's name says what the next page carries, so a lead that carries more entries is not named after an enrichment.
  - A warning never implies the page is complete.
  - Required follow-ups (next page, restart after a stale snapshot) are required leads, never items in an optional list.
  - Server instructions state how a complete result looks, for example: no `hasMore` and no continuation.
- Partial results are visible, for example as `isPartial` with reasons, and the instructions say what an agent concludes from them.

## Efficient MCP context

Optimize successful work per loaded context, not tool count or character count alone.

### Measure the served surface

- Capture actual server instructions and every `tools/list` page; inspect the host's projected tool definitions too. Freeze the build for each comparison. Include descriptions, schemas and repeated metadata, not just tool names or source-file size.
- Compare the same profiles, authorization and lifecycle states. Keep raw captures; separate variable identity/path metadata from static guidance so it cannot distort savings.
- Report UTF-8 bytes as bytes. Token claims need the production tokenizer or provider count; cost claims need usage and retry costs ([context metrics](context.md#budget)). A smaller definition is not evidence of better task success.

### Remove duplication, retain decisions

- Give each rule one [owner](#layer-owners). Remove repeated routing, boilerplate and explanations already carried by constraints. Keep purpose that names cannot convey: `wholeWord` may still need “Use for identifiers.”
- Let types, required fields, enums and branch constraints express structure. Field notes explain non-obvious values, units, omission/default behavior and ID scope; do not recite every allowed value. Keep distinctions such as acknowledgement versus completion, timeout versus failure, and record ID versus message ID when the runtime has them.
- Generate schemas from one definition. Shared source definitions do not guarantee smaller served schemas: adapters may inline them. Factor references or unions only when the receiving hosts support them and measured exposure improves; preserve branch validation and execution-time checks.
- Prefer clear language to cryptic aliases, abbreviated fields or a second schema hidden in prose. Preserve evidence and executable continuations; a context target never justifies truncation or weaker validation.

### Choose a small, coherent tool set

- Merge aliases and overlapping operations when their purpose, lifecycle and authority agree. Related actions may share a typed family, for example `{type:"read",input:{...}}`, with exact fields per variant. Keep unrelated jobs separate; avoid an unrestricted `execute` tool or one object full of optional fields.
- Tool-level annotations and approval policies must remain accurate for every variant. Split a family when combining reads and mutations obscures permissions or recovery; fewer names alone is not a gain.
- Use supported profiles or scopes to expose only relevant operations. Descriptions and instructions must not promise hidden capabilities. Bootstrap tools and changing catalogs require the negotiated revision and client to support that discovery model; configuration files alone do not prove active hooks or identity binding ([MCP wire](#mcp-wire)).

### Verify retained behavior

- For wording-only edits, compare non-prose schema structure before/after: names, requiredness, defaults, bounds, enums, composition and annotations must remain unchanged. Check affected CLI help and host projections as well as MCP calls.
- For changed tool surfaces or substantial rewrites, use [cold interviews and live replay](interview.md): selection, request/result handling, invalid combinations, hidden capabilities, retries, empty pages, continuations and untrusted content. Execute proposed calls; valid syntax or self-reported confidence is not enough.
- Keep reductions that preserve the required decisions and recovery. Report measured size and observed outcomes separately; use held-out evaluation for comparative reliability or cost-per-success claims. Do not turn a few successful probes into a general model-performance claim.

## MCP wire

- Follow the [MCP specification](https://modelcontextprotocol.io/specification/latest) revision the peer negotiated; never merge revision-specific fields (long-running calls, multi-round-trip input, header mirroring); keep the older branch while peers use it.
- Each tool: unique case-sensitive `name`, object `inputSchema` even with no arguments; a declared `outputSchema` binds `structuredContent`.
- Follow the `tools/list` cursor; cache hints only where the revision defines them.
- Separate protocol errors from actionable tool errors; annotations are [untrusted hints](context.md#untrusted-content), not authorization.
- Cross-call state: server-minted opaque [handles](agents.md#schema-rules), reauthorized on every use; no implicit state.
- Definition or order change: new catalog version, refreshed discovery, rerun set audit, recheck [frozen prefixes](agents.md#frozen-base-prompt); never hot-patch one worker's cached copy.

## Set audit

1. Inventory from the published view the client receives (`tools/list`, initialize instructions), not from an author, enforcement or catalog view that also holds private fields. Record each tool's name, one-line job, and the layer that states it. A job stated nowhere is unroutable.
2. Per close pair, write the deciding sentence; if you cannot, merge, rename, or add the condition to both descriptions.
3. Compare shared fields (name, type, unit, default, requiredness, scope); generate equivalents from one definition; document real differences.
4. Check each pair for drift; a `Next` prerequisite must match the target's required fields. Class (symptom) → repair:
   - Split owner (one rule in two layers) → one owner; delete the copy.
   - Name drift (`path` vs `directory` for one input) → one canonical name.
   - Type drift (required here, optional there) → align or document.
   - Semantic drift (`page` 1-based here, byte offset there) → one meaning per name.
   - Enum drift (guidance suggests a rejected value) → validate against the schema.
   - Phantom next (missing tool, field, or mode) → fix or delete it.
   - Phantom output key: the instructions name a result key the tool never emits, or spell it differently. Name the emitted key.
   - Silent failure (disagree on what empty proves) → state it once in server instructions.
   - Authority drift (description implies a forbidden mutation) → server instructions win.

Saved audit: a Findings row per tool (job, overlaps, finding, class, repair, severity), shared fields with whether one definition owns them, and contradictions that need an owner decision. An unexamined tool is a gap.
