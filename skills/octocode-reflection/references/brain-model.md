# Brain Model

Use this reference when improving awareness behavior around memory layers, cleanup, consolidation, or documentation. The neuroscience terms are an operating metaphor for agents, not a claim that the system is biologically faithful.

## Layer map

| Brain-ish role | Awareness surface | Agent behavior |
|----------------|-------------------|----------------|
| Attention | `workspace_status`, unread `agent_signal action:list`, active locks | Notice what is live right now before acting. Smart briefing (UserPromptSubmit hook) pre-surfaces: (1) **all `OVERRIDE` memories** (no importance floor — they contradict model defaults), (2) top memories from `GOTCHA`, `BUG`, `DECISION`, `IMPROVEMENT`, `ARCHITECTURE`, `SECURITY` with importance ≥ 6, (3) top recurring failure cluster, (4) open refinement count. Other labels (`PERFORMANCE`, `INCIDENT`, etc.) require explicit `memory_recall`. |
| Working memory | Current prompt, local reads, claimed files | Keep only task-relevant context in focus. |
| Episodic memory | `memory_refine_get` / `memory_reflect fix_repo` | Preserve what happened in this repo/branch for the next run. |
| Semantic memory | `memory_record` / `memory_recall` | Store reusable lessons that transfer across tasks. |
| Long-term documents | `~/.octocode/awareness/corpus/**/*.md` | Turn repeatedly useful knowledge into browsable notes. |
| Motor control | `file_lock type:lock`, file locks, release | Coordinate writes so intention becomes safe action. |
| Reward / error signal | `memory_verify`, failed tests, `memory_reflect outcome:` | Strengthen what worked and mark failure signatures. |
| Sleep | audit + `memory_verify` + `memory_reflect` + `memory_forget`/supersede + prune + release | Consolidate useful traces and clear stale state. |

## Working loop

1. **Attend:** run recall, handoff, status, and inbox checks before planning. Salient signals are active locks, unread messages, high-importance memories, and unfinished refinements.
2. **Encode:** after a surprising finding or decision, store the smallest useful trace in the right layer: refinement for repo state, memory for reusable lesson, corpus note for browsable knowledge.
3. **Retrieve:** treat recalled memories as cues, then verify against current files or commands before relying on them. If lexical recall misses, broaden the query. Use exact `references:` / `regex:` filters when source anchors matter.
4. **Act:** claim files before edits. The lock is the agent's motor plan: it binds intention, target files, and test plan.
5. **Reward:** run the declared test plan. A passing verification strengthens the path; a failed or skipped check becomes a failure signature, not a success story.
6. **Sleep:** finish by auditing idle state, then consolidate and clean: reflect, mark refinements done, supersede stale memories, prune resolved signals, update corpus notes when the knowledge should be browsable, and release locks.

## When sleep runs

Sleep is explicit, not time-based.
Run it when the task is complete, the session is ending, a subagent hands off, or the user asks for cleanup.
Do not infer sleep from silence alone.

Treat a run as idle only after an audit shows no live locks, active tasks, missing verification, or unresolved blocker/question signals.
Run `workspace_status`, `memory_audit_unverified`, `agent_signal action:list`, and refinement checks are enough.
If any check is unclear, leave a handoff.

## Audit before cleanup

Cleanup should be preview-first:

- `workspace_status` / `memory_audit_unverified`: identify locks, active tasks, and missing verification.
- `digest --dry-run` (CLI `awareness.mjs digest`, or the `/octocode-memory-digest` command — not a callable tool): preview message cleanup; prune resolved or old threads only when safe.
- `forget --dry-run` (CLI `awareness.mjs forget`, or the `/octocode-memory-forget` command — not a callable tool): preview stale or superseded memories before deletion; prefer `memory_record supersedes:` for better replacements.
- `memory_refine_get`: find open/ongoing handoffs; mark `done` only when the current state was verified.
- `mine-weakness` (CLI): cluster memories by `failure_signature` to surface recurring patterns before turning them into harness proposals.

Audit records should preserve evidence, decisions, and judgment notes. Do not store raw private reasoning or secrets.

## Memory hygiene

- Prefer several small layers over one giant note. A handoff, a reusable lesson, and a corpus doc answer different future questions.
- Record memories only when they change a future decision. Routine progress belongs in the conversation or a refinement, not a reusable memory.
- Supersede stale memories instead of letting old conclusions compete with better ones.
- Promote repeated high-value memories into corpus docs when an agent would benefit from reading the whole pattern, not just recalling one row.
- Prune signals after threads are resolved; they are collaboration traces, not permanent knowledge.
- Never store secrets in any layer.

## "Sleep" checklist

Run this after non-trivial work, before claiming completion:

1. Idle audit passed, or a blocker/handoff explains why it did not.
2. Verification result cleared with `memory_verify` or `file_lock type:release verified:true`.
3. Locks released or intentionally left with a clear blocker message.
4. Refinement updated to `done`, or left `open`/`ongoing` with the next action.
5. Reusable lesson recorded with `memory_record` or `memory_reflect` only if it will help later.
6. Obsolete memory superseded or deleted with `forget --dry-run` first (CLI `awareness.mjs forget` / `/octocode-memory-forget`; deletion is human-gated, not a callable tool).
7. Resolved messages pruned when a thread is no longer useful.
8. Corpus note updated only when the knowledge is curated, stable, and worth browsing.

## Design guardrails

- Do not add a new storage layer just because the metaphor has a brain part. Map new behavior onto the existing store unless a real query or retention need is missing.
- Do not make sleep automatic destructive cleanup. Prefer preview/dry-run, explicit verification, and human review for harness changes.
- Keep salience explainable: importance, recency, access count, and relevance should be inspectable with `memory_recall` (use sort:smart to inspect scoring).
- If a concept becomes a mechanical repeated action, add or reuse a script. Keep `SKILL.md` as the routing map.
