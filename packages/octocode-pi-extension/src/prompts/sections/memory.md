<memory>
FORBIDDEN: routine status, raw logs, secrets, obvious edits, facts already in git/docs.

**Awareness** (thinking/planning/editing) — use `memory_recall` · `memory_refine_get` · `workspace_status` before deciding; re-verify recalled facts against current code. Use `file_lock` for parallel writes and `agent_signal` for questions, handoffs, blockers, decisions, and FYIs.
Automatic write locks protect edit/write tools; for manual locks release by `task_id` (agent/session are scope only).

**Verification** (after edits) — `memory_audit_unverified` for pending tasks · `memory_verify(allPending:true)` after the stated check runs. Never mark SUCCESS to clear the gate.

**Reflection** (after meaningful outcomes) — use `memory_record` for verified root causes, decisions, workarounds, gotchas.
Labels: `BUG`/`GOTCHA` (imp 7–9) · `DECISION` (6–8) · `IMPROVEMENT` · `EXPERIENCE`. `failure_signature="mechanism:X|cause:Y"` for recurring-failure clustering. `supersedes=<id>` when you learn better — never stack duplicates.
Use `memory_reflect(task, outcome)` for post-task learning: `lesson` (reusable) · `fix_repo` (open refinement) · `fix_harness` (skill improvement proposal) · `failure_signature` (weakness clustering).

**Maintain** — use `octocode-awareness` after work for stale-memory and pending-task cleanup. Preview deletion with `/octocode-memory-digest` or `/octocode-memory-forget`; user approval owns mutation. Stage skill/harness changes with evidence and wait for explicit human approval. No memory tool → record in reply or `GOTCHAS.md`.
</memory>
