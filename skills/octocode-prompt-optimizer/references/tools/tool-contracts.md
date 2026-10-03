# MCP, tool, and schema layers

Load when instructions govern MCP server behavior, tool selection, descriptions, input/output schemas, or result shape. The agent reads each layer at a different moment, so a rule in the wrong layer is never read when needed. One behavior, one owner.

| Layer | Read when | Owns | Never owns |
|---|---|---|---|
| Server instructions | before the first call | which family applies, cross-tool order, shared conventions (request envelope, pagination shapes, hint grammar), approval and trust boundaries, what the server cannot do | per-field types; per-tool selection detail |
| Tool name + description | choosing a tool | when to call, when not to, knowledge that makes the choice decidable, what it returns, next useful tool | exact types and limits; global workflow |
| Input/output schema | filling a call, reading a result | types, required vs. optional, enums, limits, dependent fields, how to use each field, how to continue | which tool to pick; workflow prose |

## Server instructions

- Give the routing table (intent → tool), conditional order when one call supplies the next required anchor, and each shared convention once.
- State what the agent cannot infer: what the server refuses, what needs approval, what an empty result does and does not prove.
- Stay high level: one line per tool at most.

## Tool description

- Write it as for a capable new hire: name implicit context, terms, and resource relations ([Anthropic, writing tools](https://www.anthropic.com/engineering/writing-tools-for-agents)).
- Name: stable namespace + precise verb + noun (`repo_search_code`, `issue_get`, `artifact_list`). `search` = filtered discovery, `get` = known ID, `list` = bounded browsing; mutation verbs for state changes. Avoid near-synonyms unless evals show agents tell them apart.
- Order: **Use when → Do not use when → Inputs → Returns → Next.** Include only what makes selection decidable; restating types spends selection tokens on schema content.
- Pass tools through the API tools field, not pasted into prompt text ([OpenAI GPT-4.1](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide)).

## Schema and result

- Name fields unambiguously (`user_id`, not `user`); constrain ranges, enums, string lengths, and incompatible combinations.
- Describe each field by its usage rule: when to set it, what omission does, what it requires or conflicts with.
- Model exclusive branches as a discriminated operation with a strict field set per branch, so an invalid mix is unrepresentable.
- Use the runtime's strict mode when the schema fits its subset, then validate again at the executing boundary. Shape conformance neither authorizes effects nor proves semantics.
- Return action-relevant fields first; keep completeness, security, and capability diagnostics visible; include opaque IDs and raw payloads only when evidence or continuation needs them.
- Keep default output bounded. Add an output view (for example `response_format: concise | detailed`) only for a distinct evidence need; no redundant knobs.
- Make errors actionable: say what failed and the corrected call, not an opaque code or traceback.
- Name the continuation: pass the returned handle unchanged; never infer an offset or invent a cursor. One pagination shape per field name; budget policy lives in `../context/context-budget.md`.
- Never claim completeness when a page, truncation, or permission boundary hides results; expose the partial-state field.
- Generate equivalent shared fields from one definition; keep documented unit or scope differences.

Octocode: `@octocodeai/octocode-core` owns tool names, schemas, descriptions, and shared MCP context; runtime adapters consume them. Skills explain workflows and point to live discovery, never a second schema.

Sources: [Anthropic, writing tools](https://www.anthropic.com/engineering/writing-tools-for-agents); [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling); [Anthropic strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use).

Next: MCP lifecycle `mcp-wire-contract.md`; tool-set sweep `contract-audit.md`; cross-app capability `../agents/cross-app-contracts.md`; Zod `../agents/zod-agent-contracts.md`; outside instructions in results `../context/untrusted-content.md`; prove selection accuracy with `octocode-eval-benchmark`.
