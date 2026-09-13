# MCP, tool, and schema layers
Load when instructions govern MCP server behavior, tool selection, descriptions, input/output schemas, or result shape. Why: the three layers are read at different moments, so a rule placed in the wrong layer is never read when it is needed.

**One behavior, one owner.** The agent reads server instructions before choosing a family, the description while choosing a tool, and the schema while filling the call.

## Layer contract

| Layer | Read when | Owns | Never owns |
|---|---|---|---|
| MCP/server instructions | before the first call | which family applies, cross-tool order and workflow, shared conventions (request envelope, pagination shapes, hint grammar), approval and trust boundaries, what the server cannot do | per-field types; per-tool selection detail |
| Tool name + description | choosing among tools | when to call, when not to, the base knowledge that makes the choice decidable, what it returns, the next useful tool | exact types and limits; global workflow |
| Input/output schema | filling a call, reading a result | exact types, required versus optional, enums, limits, mutually dependent fields, how to use each field, how to continue | which tool to pick; workflow prose |

## Write the server instructions

Give the routing table (intent → tool), conditional ordering when one call supplies the next required anchor, and every convention shared by all tools stated once. State the boundaries the agent cannot infer: what the server refuses, what needs approval, what an empty result does and does not prove. Keep it high level — one line per tool at most.

## Write the tool description

Use a stable namespace plus a precise verb and noun: `repo_search_code`, `issue_get`, `artifact_list`. Reserve `search` for filtered discovery, `get` for a known identifier, `list` for bounded browsing, and mutation verbs for state changes. Avoid overlapping near-synonyms unless evaluations show agents distinguish them.

Order it **Use when → Do not use when → Inputs → Returns → Next.** Include only the knowledge that makes selection decidable — which branch of the tool applies, what the result proves, and the tool that follows. A description that restates types is spending selection tokens on schema content.

## Write the schema

- Name fields unambiguously (`user_id`, not `user`); constrain ranges, enums, string lengths, and incompatible combinations.
- Describe each field with its usage rule, not its type: state when to set it, what happens when it is omitted, and which fields it conflicts with or requires.
- Model mutually exclusive branches as a discriminated operation with a strict field set per branch, so an invalid mix is unrepresentable rather than merely discouraged.
- Return action-relevant fields first. Keep completeness, security, and capability diagnostics visible; include opaque IDs and raw payloads only when the evidence or continuation needs them.
- Keep default output bounded and useful. Add output views only for distinct evidence needs; do not add redundant verbosity knobs.
- Name the continuation and say how to resume: pass the returned handle unchanged, never infer an offset or invent a cursor. Keep one pagination shape per field name; `references/context/context-budget.md` owns the budget policy.
- Never claim completeness when a page, truncation, or permission boundary hides results — expose the partial-state field instead.

## Check the negotiated MCP tool contract

Verify the protocol version the client and server negotiated before relying on a field. For the 2025-11-25 specification, audit the whole lifecycle:
- `tools/list` returns tool definitions and an opaque `nextCursor` when more pages exist. If the server declares `listChanged`, a changed catalog sends `notifications/tools/list_changed`; the client then refreshes and versions the catalog instead of mutating an already frozen prompt in place.
- A tool definition has a unique case-sensitive `name`, optional display metadata (`title`, `description`, `icons`), a valid object `inputSchema`, and optional `outputSchema`, `annotations`, `execution`, and `_meta`. A no-argument tool should explicitly accept an empty object rather than omit `inputSchema`.
- `tools/call` carries the declared tool `name` and schema-valid `arguments`. Validate before execution; schema validity does not grant mutation authority.
- A result can contain typed `content`, `structuredContent`, and `isError`. When `outputSchema` exists, the server's `structuredContent` must conform and the client should validate it. Keep partial/completeness and recovery data in the machine-readable result rather than hiding it in prose.
- Treat annotations as untrusted hints unless the server is trusted. `readOnlyHint`, `destructiveHint`, or similar metadata never replaces authorization or server-side enforcement.

Serialize definitions in a canonical, deterministic order for caching and audit diffs. When a definition changes, bump the tool-catalog version and run `references/tools/contract-audit.md`; do not hot-patch one worker's cached copy.

Generate equivalent shared fields from one definition; preserve documented unit or scope differences. For Octocode, `@octocodeai/octocode-core` owns tool names, schemas, descriptions, and shared MCP context. Runtime adapters consume these exports. Skills explain workflows and point to live discovery rather than maintaining a second schema.
## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — namespacing, clear schemas, response formats, and token-efficient results.
- Model Context Protocol, [Tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) — tool metadata, input/output schemas, and paginated discovery.

Next: to sweep the whole tool set for contradictions and descriptor drift load `references/tools/contract-audit.md`; for the rule shape inside each layer load `references/writing/behavior.md`; when a result can grow unbounded load `references/context/context-budget.md`; for TypeScript/Zod types load `references/agents/zod-agent-contracts.md`; when annotations or result text carry outside instructions load `references/context/untrusted-content.md`; when the catalog is part of a frozen agent prefix load `references/agents/agent-prompt-integrity.md`; prove selection accuracy with `references/flow/evaluation-data.md`.
