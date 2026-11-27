Load when writing or auditing server instructions, tool names, descriptions, input schemas, or the reason attached to a tool call.

# Tool instructions, descriptions and schemas

## Layer owners

A rule in the wrong layer is never read; give each rule one owning layer.

| Layer | Read when | Owns (nothing else) |
|---|---|---|
| Server instructions | Before the first call | Cross-tool order, shared conventions, lifecycle, approval and trust boundaries |
| Tool name + description | Choosing a capability | Its job and distinction from neighboring tools |
| Operation description, when a tool has variants | Choosing an operation | Behavior, the meaning of this operation's result, side effects and next step (a meaning shared by several tools, such as what an empty result proves, goes in server instructions) |
| Input/output schema + field notes | Filling a call, reading a result | Structural constraints; values, units, defaults, scope, dependent fields and continuation |
| Runtime | After a call | The error text, hint and output the tool actually returns |

Octocode tools: edit `@octocodeai/octocode-core`, which owns tool names, schemas, descriptions and shared MCP context; skills point to live discovery, never a second schema.

## Instructions, descriptions, names

- Hosts may shorten loaded instructions or descriptions. Put routing and trust boundaries first and inspect what the target host actually serves; consult current documentation for its limits.
- Instructions reach MCP agents only, from the first turn even where tool schemas load deferred; CLI agents and hosts that drop instructions skip them, so a field an agent must fill keeps its own note, and CLI help carries its own routing.
- Server instructions explain dependencies and what the agent cannot infer (refusals, approvals, what empty proves). Include routing only when it resolves a cross-tool decision; do not repeat the tool catalog or tell the model how to behave outside this server.
- Descriptions give a capable new hire the selection or behavior information missing from the name and schema: what the tool does, when to use it and when not, what each parameter changes, and what the result omits. Anthropic calls detailed descriptions the largest factor in tool performance and suggests three to four sentences or more; a 2026 study found that augmenting every description raised task success by a median 5.85 points but added 67% more steps and regressed 17% of cases, while compact variants often held. So add the sentence that decides a call, cut the one the schema already states, and measure both success and steps.
- Names express the capability or action (`messages`, `repo_search_code`, `issue_get`); `search` filters, `get` takes a known ID, `list` browses bounded sets, mutation verbs change state. No near-synonyms unless evals show agents distinguish them. Stay inside every target host's name rule: MCP recommends 1–128 case-sensitive characters from `A-Z a-z 0-9 _ - .`, the Claude API rejects the dot, and aggregating clients prefix a server identifier, so a name must stay unique and short after prefixing.
- Pass tools in the API tools field, not prompt text.

## Schema

- Fields are unambiguous (`user_id`, not `user`), with one name, unit, encoding, cardinality, sensitivity and owner each; constrain ranges, enums, lengths and incompatible combinations; describe when to set, what omission does and conflicts.
- Exclusive branches use a discriminant (`kind`/`status`) and strict field set per branch. Define absent, `null`, empty, default and unknown-field behavior; validate again where the call executes.
- Strict modes accept a provider-specific JSON Schema subset, while MCP allows any JSON Schema 2020-12 keyword, so a valid MCP schema can be rejected. OpenAI requires every field (optional as a `null` union), `additionalProperties: false` and a root object, and rejects `allOf`, `not` and `if`; Anthropic's strict mode rejects recursion, `oneOf`, numeric and string bounds, and caps optional parameters. Read the target's current list, serve the intersection or [export per provider](message-schemas.md#one-schema-many-boundaries), and enforce each dropped constraint at execution.
- A schema states what is valid, not how the tool is used. For nested input, format conventions or fields that depend on each other, add input examples where the host supports them (Anthropic measured 72% → 90% on complex parameters); skip them for simple tools, and measure on reasoning models, where OpenAI notes examples may hurt.
- Result fields are in [tool output design](tool-results.md#tool-output-design). [Errors](tool-errors.md#actionable-errors) provide a corrected call; [pagination](tool-results.md#pagination-contract) preserves all selected evidence and makes incomplete scope visible.

## Reason-aware invocation

For a consequential call, identify the missing information or authorized effect it serves. Use the tool's real input fields; do not add a universal intent envelope or demand long rationales for trivial calls. Capture timing, usage or correlation only when diagnosing a failure or measuring that dimension.

A model-written reason does not prove the call useful, and is not authorization ([security](security-risks.md#risks-and-mitigations)).

## Unified tool contract

Generate each application's tool surface from its owning definition; keep inputs, results, continuation and error semantics consistent across adapters. Add only fields the consumer uses and test the actual boundaries.

## Sources

- Anthropic: [writing effective tools for AI agents](https://www.anthropic.com/research/writing-tools-for-agents), [define tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools), [structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
- OpenAI: [structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs), [function calling](https://developers.openai.com/api/docs/guides/function-calling)
- [MCP: server instructions](https://blog.modelcontextprotocol.io/posts/2025-11-03-using-server-instructions/); description length: [arXiv 2602.14878](https://arxiv.org/html/2602.14878v1)
