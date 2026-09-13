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
- Use the target runtime's strict or constrained mode when the schema fits its supported subset, then validate again at the executing boundary. Shape conformance does not authorize effects or prove semantic validity.
- Return action-relevant fields first. Keep completeness, security, and capability diagnostics visible; include opaque IDs and raw payloads only when the evidence or continuation needs them.
- Keep default output bounded and useful. Add output views only for distinct evidence needs; do not add redundant verbosity knobs.
- Name the continuation and say how to resume: pass the returned handle unchanged, never infer an offset or invent a cursor. Keep one pagination shape per field name; `references/context/context-budget.md` owns the budget policy.
- Never claim completeness when a page, truncation, or permission boundary hides results — expose the partial-state field instead.

Generate equivalent shared fields from one definition; preserve documented unit or scope differences. For Octocode, `@octocodeai/octocode-core` owns tool names, schemas, descriptions, and shared MCP context. Runtime adapters consume these exports. Skills explain workflows and point to live discovery rather than maintaining a second schema.
## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — namespacing, clear schemas, response formats, and token-efficient results.
- OpenAI, [Function calling](https://developers.openai.com/api/docs/guides/function-calling) — strict function schemas, tool-call correlation, execution, and result return.
- Anthropic, [Strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use) — constrained tool inputs and provider-specific JSON Schema limits.

Next: for the negotiated MCP lifecycle load `references/tools/mcp-wire-contract.md`; to sweep the tool set for contradictions load `references/tools/contract-audit.md`; when a capability crosses agent apps load `references/agents/cross-app-contracts.md`; for rule shape load `references/writing/behavior.md`; when a result can grow unbounded load `references/context/context-budget.md`; for TypeScript/Zod load `references/agents/zod-agent-contracts.md`; when annotations or result text carry outside instructions load `references/context/untrusted-content.md`; prove selection accuracy with `references/flow/evaluation-data.md`.
