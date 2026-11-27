Load when deciding what enters the context window or diagnosing context-driven quality loss.

# Context and attention

## Attention first

| Tier | Contents | Handling |
|---|---|---|
| Critical | Objective, acceptance criteria, safety constraints | In the system prompt or the active-state summary; confirm it is still present after each compaction |
| Active | Current plan, relevant code, verified findings | In context |
| Supporting | Related files, dependencies, earlier decisions | Brief or referenced |
| Available | Large logs, full documents, broad search results | Load on demand |

These are engineering priorities, not measured attention weights: context selection shapes what the model sees but does not control transformer attention.

Length alone can cost accuracy below the window limit. With task difficulty held constant, accuracy fell as input grew across 18 models (Chroma, 2025), and fell 13.9–85% even when every relevant fact was retrieved (Du et al., 2025). One 2026 preprint found no drop to 150,000 tokens on newer models with a needle probe, so compare a focused input against the full one on your model and task instead of assuming either result. Run the same cases twice: once with only the passages each answer needs, once with the full input; the gap is the cost of length.

- Keep the current goal and constraints salient; maintain a short active-state summary.
- Prefer exact symbols, relevant line ranges and concise evidence over whole files.
- Remove duplicated and obsolete material. Do not reinject unchanged tool results. Restate the goal and constraints only when a trace shows drift or after a long untrusted block ([steer during a long run](guiding-agents.md#steer-during-a-long-run)); check that the drifting behavior stops in the next turns.
- Keep large artifacts reachable through references; retrieve details when they can change a decision.

## Context failures

| Failure | Symptom | Fix, then check |
|---|---|---|
| Overflow | Request exceeds the context limit | Count occupancy ([budget](context-budget.md#context-budget)), [compact](compaction.md#compaction) at a subtask boundary, recount |
| Pollution | Irrelevant information dominates | Narrow the query or read ranges ([retrieval](retrieval.md)); rerun the failing step on the focused input |
| Context rot | Accuracy drops as input grows | Compare the focused input with the full one on the same cases; keep the smaller if results hold |
| Lost-in-the-middle | A fact present in context is ignored | Move it beside the question ([placement](prompt-writing.md#tags-and-placement)); rerun |
| Stale context | Agent acts on an outdated value | Reread the source at its current revision before the dependent action |
| Summary drift | A fact differs after compaction | Rebuild the summary from originals ([compaction](compaction.md#checkpoint-template)); compare the facts |
| Instruction collision | Two rules demand opposite actions | Resolve by host hierarchy and give the behavior one owner ([write and repair](prompt-writing.md#write-and-repair)) |
| Tool-output flooding | One result fills the window | Return a summary and a reference, or page ([tool results](tool-results.md#reference-first-responses)) |
| Cross-task contamination | Another task's details appear in the output | Run the task in a fresh context or subagent ([delegation](delegation.md)) |
| Evidence laundering | An inference is reported as fact | Relabel with the [grounding labels](grounding.md#grounding); reopen the source before the conclusion |

## Sources

- [Anthropic: effective context engineering for AI agents](https://www.anthropic.com/research/effective-context-engineering-for-ai-agents)
- Input length: [Chroma: Context Rot](https://www.trychroma.com/research/context-rot), [Du et al.](https://arxiv.org/abs/2510.05381); no drop found: [Jagtap](https://zenodo.org/records/20753849)
