<memory>
Awareness memory/coordination has no dedicated tools. Drive it through the
octocode-awareness CLI: `bash: node $OCTOCODE_AWARENESS_CLI <noun> <verb> --compact`
(agent id + workspace are inherited from the environment). Load the
**octocode-awareness skill** for the full workflow, flags, and recipes.
FORBIDDEN to store: routine status, raw logs, secrets, obvious edits, facts already in git/docs.

**Awareness** (thinking/planning/editing) — `node "$OCTOCODE_AWARENESS_CLI" attend --workspace "$PWD" --query "<task>" --compact`; inspect Ready/Claimed/Verify/FilesUnderWork. Claim a matching `task ready`, or open standalone `work start` with rationale + test plan. Every edit auto-declares advisory file presence via hooks; ordinary peers may overlap knowingly. Use `work start --exclusive` only when a sensitive path needs exclusive protection. Run `memory recall` · `refinement get` only when durable context can change the plan; re-verify recalled facts.
The Pi hooks attach automatic advisory presence to the live task/WORK run when present; otherwise they create an isolated HOOK run. Manual exclusive release/renew uses `run_id`.

**Verification** (after edits) — submit claimed tasks (`task submit`), then `verify audit` and `verify mark --run-id <exact-run>` after that run's stated check passes. Never batch-verify unrelated agents' work and never mark SUCCESS merely to clear the gate.

**Reflection** (after meaningful outcomes) — `memory record` for verified root causes, decisions, workarounds, gotchas.
Labels: `BUG`/`GOTCHA` (imp 7–9) · `DECISION` (6–8) · `IMPROVEMENT` · `EXPERIENCE`. `--failure-signature "mechanism:X|cause:Y"` for recurring-failure clustering. `--supersedes <id>` when you learn better — never stack duplicates.
Use `reflect record --task … --outcome …` for post-task learning: `--lesson` (reusable) · `--fix-repo` (open refinement) · `--fix-harness` (skill improvement proposal) · `--failure-signature` (weakness clustering).

**Maintain** — after work, run stale-memory and pending-run cleanup via the CLI. Preview deletion with `/octocode-memory-digest` or `/octocode-memory-forget`; user approval owns mutation. Stage skill/harness changes with evidence and wait for explicit human approval. If the CLI is unavailable → record in reply or `GOTCHAS.md`.
</memory>
