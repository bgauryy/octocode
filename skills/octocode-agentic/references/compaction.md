Load when compacting a long-running session or writing a checkpoint.

# Compaction

| Strategy | Best use | Danger |
|---|---|---|
| Tool-result clearing | Old bulky tool outputs | Removing needed evidence |
| Structured summary | Long conversations | Losing critical details |
| Checkpoint notes | Long engineering tasks | Stale progress information |
| Hierarchical summaries | Extensive research and code exploration | Accumulated drift |
| Task-specific compaction | Details the next action needs | Wrong relevance prediction |
| Externalized artifacts | Large logs, diffs, datasets | Broken references |
| Subagent isolation | Separate exploration contexts | Incomplete handoffs |
| Rolling working set | Recent active context | Forgetting older constraints |
| Provider-native compaction | Supported long-running sessions | Provider-specific semantics |

## Checkpoint template

Keep the current goal and constraints, verified findings with source references, consequential decisions, changed artifacts, pending work and next action. Preserve exact handles, commands, approval state or failure details only where resumption depends on them; do not invent checkpoint IDs or metadata fields.

- **Discard:** redundant narration, repeated logs, duplicate or stale raw results, banners, help text, unchanged headers, retry noise and repeated explanations. For failed attempts keep the one-line lesson; for closed hypotheses keep conclusion and evidence ID. Remove subtasks only when the evidence store can re-derive them.
- Compact at a subtask boundary on measured occupancy and quality (the continuity test below), not a fixed percentage; recount after. Mid-derivation, checkpoint assumptions, evidence, open branches and next action first.
- Rebuild from original evidence, never a summary of a summary. Hierarchical summaries need rehydration checks against originals to avoid accumulated drift.
- Keep long-task progress, test status and todos outside the window and reload them after compaction.
- Evidence tiers: active context (goal, constraints, key results, next action) → evidence index (IDs to passages, paths, outputs) → cold originals. Reopen the primary source before a consequential conclusion.
- Recurrent lossy compaction or filters need evidence that task quality holds, not just fewer tokens.
- Test continuity: after compaction, can the agent still name the next correct action and its supporting evidence?

## Sources

- [OpenAI: compaction](https://developers.openai.com/api/docs/guides/compaction)
