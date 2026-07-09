# Agent Tool & MCP Contract

Load when instructions govern tool selection, MCP behavior, schemas, descriptions, or response shape.

**One behavior, one owner.** Put global routing in server/agent instructions, selection guidance in a tool description, valid shapes in its schema, and continuation guidance in the result.

## Contract map

| Layer | Must answer |
|---|---|
| Server/agent instructions | Which tool family applies, approval boundaries, and cross-tool workflow |
| Name + description | When to call it, when not to, required context, and the next useful tool |
| Input schema | Exact types, required fields, limits, enums, and mutually dependent inputs |
| Output schema/result | What was found or changed, stable handles, completeness, and the next action |

## Name and describe for selection

- Use a stable namespace plus a precise verb and noun: `repo_search_code`, `issue_get`, `artifact_list`.
- Reserve `search` for filtered discovery, `get` for a known identifier, `list` for bounded browsing, and mutation verbs for state changes.
- Avoid overlapping near-synonyms unless evaluations show agents distinguish them.
- Write the description as: **Use when** → **Do not use when** → **Inputs** → **Returns** → **Next**.

## Make calls valid and useful

- Schemas enforce shape, not intent: use descriptions/examples for optional-parameter conventions and tool choice.
- Give fields unambiguous names (`user_id`, not `user`); constrain ranges, enums, string lengths, and incompatible combinations.
- Return action-relevant fields first; hide diagnostics, raw blobs, and opaque IDs unless the next call needs them.
- Offer `concise` by default and a deliberate `detailed` mode only when both are useful.

## Large-result contract

- Narrow server-side with query, scope, fields, range, and limit before calling.
- For incomplete output, return `items`, `isPartial`, a short scope summary, and opaque `nextCursor`.
- State exactly how to resume: pass `nextCursor` unchanged; never make agents infer offsets or invent cursors.
- Do not claim completeness when a page, truncation, or permission boundary hides results.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — namespacing, clear schemas, response formats, and token-efficient results.
- Model Context Protocol, [Tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) — tool metadata, input/output schemas, and paginated discovery.
