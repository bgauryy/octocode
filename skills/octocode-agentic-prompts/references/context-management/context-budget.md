# Context budget, pagination, and compaction

Load when a prompt, tool, retrieval path, or handoff can overfill context, state nears the window, or a prompt summarizes content it may need later. Unknown executing surface → `../flow/runtime-context.md` first.

The next decision needs the smallest evidence set that lets the agent act; fetch detail on demand through identifiers.

## Keep four quantities distinct

- **Context window**: request plus output capacity, from the live model capability.
- **Occupancy**: instructions, tool definitions, messages, images, retained reasoning, tool calls and results, output.
- **Usable input budget**: window minus output and reasoning reserve, provider overhead, and a safety margin.
- **Billing tokens**: uncached input, cache writes and reads, output, tool fees. Cached tokens still occupy context.

Count with the provider counting endpoint or production tokenizer, never characters.

## Input, output, and pagination

| Situation | Default | Escalate only when |
|---|---|---|
| Known target | One focused lookup | The result lacks a required handle or fact |
| Search/retrieval | Query + scope + small limit + relevant fields | Evidence is insufficient or conflicting |
| Large collection | Server-side filter, sort, aggregate, page | The task needs another page |
| Tool result | Concise answer, stable handles, completeness state | A downstream call needs technical detail |
| Phase handoff | The handoff packet, no source excerpts | The new phase needs a source excerpt |

- Never fetch a whole corpus "in case". Pass continuation state unchanged as the exact resume call; never invent an offset or cursor. One pagination shape per field name.
- Summarize a finished search with scope, count, decisive evidence, and gaps.
- Handoffs keep constraints and next-call identifiers; drop raw logs and abandoned branches.
- When intermediate data would crowd context, use supported filtering outside the model while retaining the source evidence.
- Measure prompt plus tool output together.

## Compaction: what to cut

| Cut: no longer informs the next decision | Never cut: unreachable once dropped |
|---|---|
| Repeated tool logs, duplicate results; old raw tool results (clear them first) | Identifiers: IDs, hashes, versions, exact paths |
| Failed attempts whose lesson is recorded in one line | Exact parameters: arguments, flags, limits, thresholds |
| Boilerplate: banners, help text, unchanged headers, retry noise | Failure specifics: exception type, error code, stack frames |
| Closed hypotheses, minus the conclusion and its evidence ID | Ordering: event sequence, timestamps, causal order |
| Resolved subtasks the evidence store can re-derive | Citations, permissions, scope, experimental settings; open derivations |

Measure the effect of filtering on the actual task; token reduction alone does not establish quality.

## When to compact

- Trigger from measured occupancy and quality, not a universal percentage. Recount after compaction.
- Compact at subtask boundaries. Mid-derivation, first checkpoint assumptions, evidence, open branches, and next action in retrievable state.
- No summaries of summaries; rebuild active state from original evidence.
- Never compact away an incomplete-result marker, approval requirement, error, or recovery path.
- Long tasks: keep structured notes (progress, test status, todos) outside the window; reload after compaction.
- Require evidence before enabling recurrent lossy compaction on long-horizon runs. Savings claims: `token-economics.md`.

## Keep evidence retrievable

Active context (goal, constraints, hypotheses, key results, next action) → evidence index (IDs resolving to passages, URLs, paths, tool outputs) → cold store (originals, re-insertable). Reopen the primary source before a consequential conclusion.

Never let a summary be the only copy.

Source: Anthropic [context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).

Next: handoff packet `../agents/agent-communication.md`; measure with `octocode-eval-benchmark`.
