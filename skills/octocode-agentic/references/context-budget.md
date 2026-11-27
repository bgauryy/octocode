Load when sizing a request, counting tokens, weighing always-loaded instruction files, or measuring token and cost savings.

# Context budget and cost

## Context budget

Separate window, occupancy (all sent, tool definitions included), usable input (window minus output reserve and margin), and billed tokens; cached tokens still occupy the window. Count tokens with the provider's count endpoint or production tokenizer, never infer them from characters. UTF-8 bytes can compare serialized surfaces; label them as bytes, not tokens or billed savings. For MCP discovery, use the [served-context comparison](mcp-context.md#measure-the-served-surface).

Send the smallest actionable evidence, never a corpus "in case":

- Known target: one focused lookup; more only if a needed handle is missing.
- Search: query, scope, small limit and needed fields; widen if evidence is thin or conflicting.
- Large collection: server-side filter, sort and aggregate, then page per the [evidence loop](evidence-loop.md#pagination). [Tool results](tool-results.md#tool-output-design) and [handoffs](handoffs.md#handoff-protocols) carry detail only when the next step needs it.
- Finished search: scope, count, decisive evidence and gaps. Filter bulky intermediates outside the model, keep source evidence, and measure prompt plus tool output together.

Always-loaded instruction files (`AGENTS.md`, `CLAUDE.md`) spend this budget on every request. A controlled study of four coding agents on Python repositories found they did not generally raise task success and raised cost by over 20%; developer-written files did slightly better than generated ones, without significance, and another study measured lower runtime with them. Keep what the agent cannot discover from the repository (commands, constraints, owners), drop generated overviews, and measure success and cost with and without the file.

Reserve output and headroom for tool results, metadata and unexpected expansion using the actual model and endpoint limits. Measure occupancy; do not fill remaining capacity or copy a fixed percentage split.

## Cost

- Cost per successful task: record each attempt's `costUsd` with its retries and recovery included, then read `costPerSuccess` from run-stats.mjs. It is `null` when a run lacks a cost or none succeeded: fill the gap instead of estimating.
- Price by vendor usage buckets (uncached input, cache write, cache read, output and reasoning, paid tools) at current prices, never a fixed multiplier. Freeze task, quality target, model, tools and traffic; count prefix, dynamic input, history and output separately; price cold, warm, miss, retry and compaction paths; record model, endpoint, tokenizer and date.
- Delete an instruction when the same cases pass without it; defer tools per [tool count](mcp-context.md#choose-a-small-coherent-tool-set); route bounded work per [escalation policy](model-routing.md#escalation-policy).
- Keep a change only if [sealed-test](eval-cases.md#splits-and-exposure) success and safety hold and cost per success or latency improves; report absolute cost with model, requests, context length, output size and cache telemetry.
- Token-saving claims compare tokens per fact with uncompressed text on your own tasks (simultaneous facts, unresolved work, delayed retrieval), with model, context length and variance. Plain sentences beat `!EV12` codes or dropped vowels; shorter but missing identifiers loses.

## Sources

- Instruction files: [Gloaguen et al.: Evaluating AGENTS.md](https://arxiv.org/html/2602.11988v3); lower runtime: [Lulla et al.](https://arxiv.org/abs/2601.20404)
