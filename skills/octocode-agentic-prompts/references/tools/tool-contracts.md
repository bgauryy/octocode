# MCP, tool, and schema contracts

Load when instructions govern MCP server behavior, tool selection, descriptions, schemas, results, or MCP wire behavior; when a server has more than one tool; or after editing any of them. One behavior, one owner: a rule in the wrong layer is never read.

| Layer | Read when | Owns | Never owns |
|---|---|---|---|
| Server instructions | before the first call | which family applies, cross-tool order, shared conventions (envelope, pagination, hints), approval and trust boundaries, what the server cannot do | per-field types; per-tool selection |
| Tool name + description | choosing a tool | when to call and not, knowledge that makes the choice decidable, what it returns, next tool | types and limits; global workflow |
| Input/output schema | filling a call, reading a result | types, required vs. optional, enums, limits, dependent fields, field usage, continuation | tool choice; workflow prose |

## Server instructions and descriptions

- Server instructions add a routing table (intent → tool), conditional order when one call supplies the next anchor, and what the agent cannot infer: refusals, approvals, what an empty result proves. One line per tool at most.
- Description, for a capable new hire: name implicit context and terms. Order: **Use when → Do not use when → Inputs → Returns → Next.** Only what makes selection decidable; no restated types.
- Name: namespace + verb + noun (`repo_search_code`, `issue_get`). `search` = filtered discovery, `get` = known ID, `list` = bounded browsing; mutation verbs for state changes. No near-synonyms unless evals show agents tell them apart.
- Pass tools through the API tools field, not prompt text.

## Schema and result

- Unambiguous field names (`user_id`, not `user`); constrain ranges, enums, lengths, and incompatible combinations. Describe each field by usage: when to set it, what omission does, what it conflicts with.
- Exclusive branches: a discriminated operation with a strict field set per branch.
- Use strict mode when the schema fits its subset, then validate again at the executing boundary.
- Bounded default output, action-relevant fields first; security and capability diagnostics stay visible; opaque IDs and raw payloads only when evidence or continuation needs them. An output view (`response_format: concise | detailed`) only for a distinct evidence need.
- Actionable errors: what failed and the corrected call, not an opaque code or traceback.
- Never claim completeness when a page, truncation, or permission boundary hides results; expose the partial-state field.

Octocode: `@octocodeai/octocode-core` owns tool names, schemas, descriptions, and shared MCP context; skills point to live discovery, never a second schema.

## MCP wire: branch on the negotiated version

Never merge revision-specific fields; verify the live spec before editing. For MCP `2026-07-28`:

- Requests carry protocol/client metadata. `tools/list` is paginated, returns a typed `resultType` and cache hints (`ttlMs`, `cacheScope`), in deterministic order for stable sets. A `listChanged` server notifies clients subscribed through `subscriptions/listen`.
- A tool has a unique case-sensitive `name`, optional display metadata, an object `inputSchema` (also with no arguments), and optional `outputSchema` and `annotations`. JSON Schema 2020-12 composition is allowed; `outputSchema` may describe any JSON value, and `structuredContent` must conform to it.
- Streamable HTTP may mirror primitive arguments with `x-mcp-header`; validate its constraints and never expose secrets through headers.
- `tools/call` validates `name` and `arguments`. Results are `complete` or `input_required`; a retry carries the requested inputs and any opaque `requestState` under a new JSON-RPC request ID.
- Separate protocol errors from actionable tool errors. Annotations are untrusted hints, not authorization.
- No implicit cross-call state: the server mints an opaque handle, accepts it later, reauthorizes every use, and documents expiry and recovery.

Keep the older branch when the peer negotiates it. In `2025-11-25`, `structuredContent` and `outputSchema` are object-rooted, discovery lacks the cache-result contract, and Tasks use the older experimental vocabulary. In `2026-07-28`, Tasks are a separate extension and calls can use multi-round-trip `input_required`. Migrate server, client, tests, and cached catalog together. A definition or order change publishes a new catalog version, refreshes discovery, reruns the set audit, and rechecks frozen agent prefixes; never hot-patch one worker's cached copy.

## Set audit

1. **Inventory**: name, one-line job, and the layer stating it. A job stated nowhere is unroutable.
2. **Selection overlap**: for each close pair, write the deciding sentence. If you cannot, merge, rename, or add the condition to both descriptions.
3. **Shared descriptors**: compare reused fields (names, types, units, defaults, requiredness, scope). Generate equivalent ones from one definition; document real differences.
4. **Drift**: check each pair against every class below (a `Next` prerequisite must match the target's required fields).

| Class | Symptom | Repair |
|---|---|---|
| Split owner | same rule in server instructions and a description, worded differently | one owner; delete the copy |
| Name drift | `path` vs. `directory`, `limit` vs. `maxResults` for one input | one canonical name |
| Type drift | required in one tool, optional in another, no reason | align or document the difference |
| Semantic drift | `page` 1-based in one tool, a byte offset in another | one meaning per name (`page`, `charOffset`, `cursor`) |
| Enum drift | guidance suggests a value the operation rejects | validate against that operation's schema |
| Phantom next | points to a missing tool, field, or mode | fix the pointer or delete the claim |
| Silent-failure drift | tools disagree on whether empty proves absence | state it once in server instructions |
| Authority drift | description implies a mutation the server forbids | server instructions win; narrow the description |

Output: `## Contract Audit` table `| Tool | Job | Overlaps with | Finding | Class | Repair |` (one row per tool), then `Shared descriptors: <field -> tools, single definition yes/no>` and `Unresolved: <contradiction needing an owner decision>`. An unexamined tool is a gap, not a pass.

Source: MCP [2026-07-28 Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools).

Next: record repairs in `../flow/fix.md`; prove selection accuracy with `octocode-eval-benchmark`.
