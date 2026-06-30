# Brain Model

Use this reference when improving awareness behavior around memory layers, cleanup, consolidation, or documentation. The neuroscience terms are an operating metaphor for agents, not a claim that the system is biologically faithful.

## Layer map

| Brain-ish role | Awareness surface | Agent behavior |
|----------------|-------------------|----------------|
| Attention | `status`, unread `notify-get`, active locks | Notice what is live right now before acting. |
| Working memory | Current prompt, local reads, claimed files | Keep only task-relevant context in focus. |
| Episodic memory | `refine-set` / `refine-get` | Preserve what happened in this repo/branch for the next run. |
| Semantic memory | `tell-memory` / `get-memory` | Store reusable lessons that transfer across tasks. |
| Long-term documents | `~/.octocode/awareness/corpus/**/*.md` | Turn repeatedly useful knowledge into browsable notes. |
| Motor control | `pre-flight-intent`, file locks, release | Coordinate writes so intention becomes safe action. |
| Reward / error signal | `verify`, failed tests, `reflect --outcome` | Strengthen what worked and mark failure signatures. |
| Sleep | audit + `verify` + `reflect` + `forget`/supersede + prune + release | Consolidate useful traces and clear stale state. |

## Working loop

1. **Attend:** run recall, handoff, status, and inbox checks before planning. Salient signals are active locks, unread messages, high-importance memories, and unfinished refinements.
2. **Encode:** after a surprising finding or decision, store the smallest useful trace in the right layer: refinement for repo state, memory for reusable lesson, corpus note for browsable knowledge.
3. **Retrieve:** treat recalled memories as cues, then verify against current files or commands before relying on them. If lexical recall misses, broaden or use semantic recall after `embed-index`.
4. **Act:** claim files before edits. The lock is the agent's motor plan: it binds intention, target files, and test plan.
5. **Reward:** run the declared test plan. A passing verification strengthens the path; a failed or skipped check becomes a failure signature, not a success story.
6. **Sleep:** finish by auditing idle state, then consolidate and clean: reflect, mark refinements done, supersede stale memories, prune resolved notifications, update corpus notes when the knowledge should be browsable, and release locks.

## When sleep runs

Sleep is explicit, not time-based. Run it when the task is complete, the session is ending, a subagent hands off, or the user asks for cleanup. Do not infer sleep from silence alone.

Treat a run as idle only after an audit shows: no live locks for the agent, no active intents for the agent, no missing verification for its claimed work, and no unresolved blocker/question messages that need a response. Existing `status`, `audit-unverified`, `notify-get`, and refinement checks are enough to decide; if any check is unclear, leave a handoff instead of cleaning aggressively.

## Audit before cleanup

Cleanup should be preview-first:

- `status` / `audit-unverified`: identify locks, active intents, and missing verification.
- `notify-prune --dry-run`: preview message cleanup; prune resolved or old threads only when safe.
- `forget --dry-run`: preview stale or superseded memories before deletion; prefer `tell-memory --supersedes` for better replacements.
- `refine-get`: find open/ongoing handoffs; mark `done` only when the current state was verified.
- `mine-weakness` / `memory-graph`: audit recurring failures or dense memory clusters before turning them into harness changes.

Audit records should preserve evidence, decisions, and judgment notes. Do not store raw private reasoning or secrets.

## Memory hygiene

- Prefer several small layers over one giant note. A handoff, a reusable lesson, and a corpus doc answer different future questions.
- Record memories only when they change a future decision. Routine progress belongs in the conversation or a refinement, not a reusable memory.
- Supersede stale memories instead of letting old conclusions compete with better ones.
- Promote repeated high-value memories into corpus docs when an agent would benefit from reading the whole pattern, not just recalling one row.
- Prune notifications after threads are resolved; they are collaboration traces, not permanent knowledge.
- Never store secrets in any layer.

## "Sleep" checklist

Run this after non-trivial work, before claiming completion:

1. Idle audit passed, or a blocker/handoff explains why it did not.
2. Verification result recorded with `verify` or `release-file-lock --verified`.
3. Locks released or intentionally left with a clear blocker message.
4. Refinement updated to `done`, or left `open`/`ongoing` with the next action.
5. Reusable lesson recorded with `tell-memory` or `reflect --lesson` only if it will help later.
6. Obsolete memory superseded or deleted with `forget --dry-run` first.
7. Resolved messages pruned when a thread is no longer useful.
8. Corpus note updated only when the knowledge is curated, stable, and worth browsing.

## Design guardrails

- Do not add a new storage layer just because the metaphor has a brain part. Map new behavior onto the existing store unless a real query or retention need is missing.
- Do not make sleep automatic destructive cleanup. Prefer preview/dry-run, explicit verification, and human review for harness changes.
- Keep salience explainable: importance, recency, access count, and relevance should be inspectable with `get-memory --explain`.
- If a concept becomes a mechanical repeated action, add or reuse a script. Keep `SKILL.md` as the routing map.
