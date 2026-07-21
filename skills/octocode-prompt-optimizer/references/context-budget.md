# Context Budget & Pagination

Load when a prompt, tool, retrieval path, or handoff can fill context with more than the next action needs.

**Budget for the next decision, not for completeness.** Keep the smallest high-signal evidence set that lets the agent act correctly; fetch detail just in time.

## Input and output policy

| Situation | Default | Escalate only when |
|---|---|---|
| Known target | One focused lookup | The result lacks a required handle or fact |
| Search/retrieval | Query + scope + small limit + relevant fields | Evidence is insufficient or conflicting |
| Large collection | Server-side filter, sort, aggregate, and page | The task truly needs another page |
| Tool result | Concise answer, stable handles, completeness state | A downstream call needs technical detail |
| Phase handoff | Goal, evidence, decision, blocker, next action | The new phase needs a source excerpt |

## Pagination workflow

1. Start targeted; do not fetch a whole corpus “just in case.”
2. Return a small page plus `isPartial` and opaque `nextCursor` when more exists.
3. Tell the agent the precise resume call; reuse the cursor unchanged.
4. Fetch another page only if the current evidence cannot answer the task.
5. Summarize a completed search with scope, count, decisive evidence, and unresolved gaps.

## Compaction and handoffs

- Preserve decisions, constraints, identifiers needed for the next call, and evidence anchors; drop raw logs, duplicate prose, and abandoned branches.
- Never compact away an incomplete-result marker, approval requirement, error, or recovery path.
- When a task has three or more dependent calls or huge intermediate data, filter/aggregate outside model context when the platform supports it.
- Measure prompt plus tool output together: a short prompt can still be expensive if its tool path returns unbounded data.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — finite attention budget and minimal high-signal context.
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — filtering, pagination, truncation, and useful continuation guidance.
