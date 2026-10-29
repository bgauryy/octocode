Load when writing or auditing MCP server instructions, tool names, descriptions, schemas, results, or wire behavior, or when a server has more than one tool.

## Layer owners

A rule in the wrong layer is never read; give each rule one owning layer.

| Layer | Read when | Owns (nothing else) |
|---|---|---|
| Server instructions | Before the first call | Family choice, cross-tool order, shared conventions, approval and trust boundaries, limits |
| Name + description | Choosing a tool | When to call and not, returns, next tool |
| Input/output schema | Filling a call, reading a result | Types, required, enums, limits, dependent fields, continuation |

- Octocode tools: edit `@octocodeai/octocode-core`, which owns tool names, schemas, descriptions, and shared MCP context; skills point to live discovery, never a second schema.

## Instructions, descriptions, names

- Server instructions: intent → tool routing, conditional order when one call feeds the next, what the agent cannot infer (refusals, approvals, what empty proves); at most one line per tool: `Find text: search_code → get_file at the returned path.`
- Descriptions: for a capable new hire, naming implicit context: use when, not when, inputs, returns, next; only what decides selection, never types. "Searches code." → "Use when the path is unknown; not for a known file (get_file). Returns paths, lines. Next: get_file."
- Names: namespace + verb + noun (`repo_search_code`, `issue_get`); `search` filters, `get` takes a known ID, `list` browses bounded sets, mutation verbs change state. No near-synonyms unless evals show agents distinguish them.
- Pass tools in the API tools field, not prompt text.

## Schema and result

- Fields: unambiguous (`user_id`, not `user`), one name, unit, encoding, cardinality, sensitivity, owner each; constrain ranges, enums, lengths, incompatible combinations; describe when to set, what omission does, conflicts.
- Exclusive branches: discriminant (`kind`/`status`), strict field set per branch; define absent, `null`, empty, default, unknown-field behavior; strict mode where it fits; validate again where the call executes.
- Results: bounded default, action-relevant fields first, security and capability diagnostics visible; opaque IDs or raw payloads only for evidence or continuation; `response_format: concise | detailed` only for a distinct evidence need.
- Errors say what failed and give the corrected call: `Error 400` → `path must be workspace-relative; retry with path: "src/a.ts"`.
- Large results return complete pages with executable continuations; a terminal limit or denied scope is explicit. Never silently truncate evidence or claim completeness from a partial page.

## MCP wire

- Follow the [MCP specification](https://modelcontextprotocol.io/specification/latest) revision the peer negotiated; never merge revision-specific fields (long-running calls, multi-round-trip input, header mirroring); keep the older branch while peers use it.
- Each tool: unique case-sensitive `name`, object `inputSchema` even with no arguments; a declared `outputSchema` binds `structuredContent`.
- Follow the `tools/list` cursor; cache hints only where the revision defines them.
- Separate protocol errors from actionable tool errors; annotations are [untrusted hints](context.md#untrusted-content), not authorization.
- Cross-call state: server-minted opaque [handles](agents.md#schema-rules), reauthorized on every use; no implicit state.
- Definition or order change: new catalog version, refreshed discovery, rerun set audit, recheck [frozen prefixes](agents.md#frozen-base-prompt); never hot-patch one worker's cached copy.

## Set audit

1. Inventory each tool's name, one-line job, and the layer stating it; a job stated nowhere is unroutable.
2. Per close pair, write the deciding sentence; if you cannot, merge, rename, or add the condition to both descriptions.
3. Compare shared fields (name, type, unit, default, requiredness, scope); generate equivalents from one definition; document real differences.
4. Check each pair for drift; a `Next` prerequisite must match the target's required fields. Class (symptom) → repair:
   - Split owner (one rule in two layers) → one owner; delete the copy.
   - Name drift (`path` vs `directory` for one input) → one canonical name.
   - Type drift (required here, optional there) → align or document.
   - Semantic drift (`page` 1-based here, byte offset there) → one meaning per name.
   - Enum drift (guidance suggests a rejected value) → validate against the schema.
   - Phantom next (missing tool, field, or mode) → fix or delete it.
   - Silent failure (disagree on what empty proves) → state it once in server instructions.
   - Authority drift (description implies a forbidden mutation) → server instructions win.

Saved audit: a Findings row per tool (job, overlaps, finding, class, repair, severity), shared fields with whether one definition owns them, and contradictions that need an owner decision. An unexamined tool is a gap.
