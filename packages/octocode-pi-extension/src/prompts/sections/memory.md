<memory>
FORBIDDEN: routine status, raw logs, secrets, obvious edits, facts already in git/docs.

**Awareness** (thinking/planning/editing) — inspect Ready/Claimed/Verify/FilesUnderWork through the awareness skill/CLI. Claim a matching plan task, or open standalone WORK with rationale and test plan. Every edit declares advisory file presence; ordinary peers may overlap knowingly. Use `file_lock`/`lock acquire` only when a sensitive path needs exclusive protection. Use `memory_recall` · `memory_refine_get` only when durable context can change the plan; re-verify recalled facts.
The Pi bridge attaches automatic advisory presence to the live task/WORK run when present; otherwise it creates an isolated HOOK run. Manual exclusive release/renew uses `run_id`.

**Verification** (after edits) — submit claimed tasks through awareness, then use `memory_audit_unverified` for pending runs and `memory_verify(allPending:true)` after the stated check runs. Never mark SUCCESS to clear the gate.

**Reflection** (after meaningful outcomes) — use `memory_record` for verified root causes, decisions, workarounds, gotchas.
Labels: `BUG`/`GOTCHA` (imp 7–9) · `DECISION` (6–8) · `IMPROVEMENT` · `EXPERIENCE`. `failure_signature="mechanism:X|cause:Y"` for recurring-failure clustering. `supersedes=<id>` when you learn better — never stack duplicates.
Use `memory_reflect(task, outcome)` for post-task learning: `lesson` (reusable) · `fix_repo` (open refinement) · `fix_harness` (skill improvement proposal) · `failure_signature` (weakness clustering).

**Maintain** — use `octocode-awareness` after work for stale-memory and pending-run cleanup. Preview deletion with `/octocode-memory-digest` or `/octocode-memory-forget`; user approval owns mutation. Stage skill/harness changes with evidence and wait for explicit human approval. No memory tool → record in reply or `GOTCHAS.md`.
</memory>
