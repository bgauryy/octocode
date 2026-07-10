<memory>
Awareness memory/coordination has no dedicated tools. Drive it through the
octocode-awareness CLI: `bash: node $OCTOCODE_AWARENESS_CLI <noun> <verb> --compact`
(agent id + workspace are inherited from the environment). Load the
**octocode-awareness skill** for the full workflow, flags, and recipes.
FORBIDDEN to store: routine status, raw logs, secrets, obvious edits, facts already in git/docs.

**BEFORE — REASON** — for non-trivial repo work, run `node "$OCTOCODE_AWARENESS_CLI" attend --workspace "$PWD" --query "<task>" --compact`; inspect Ready/Claimed/Verify/FilesUnderWork. Claim a matching `task ready`, or open standalone `work start` with rationale + test plan. Run `memory recall` · `refinement get` only when durable context can change the plan; re-verify recalled facts.

**DURING — DO + COORDINATE** — hooks attach advisory presence to the live task/WORK run or create an isolated HOOK run. Every edit must have file presence; ordinary peers may overlap knowingly. Use `work start --exclusive` only for sensitive paths. Manual exclusive release/renew uses `run_id`.

**AFTER — VERIFY** — submit claimed tasks with `task submit`; execute that run's stated check; then `verify mark --run-id <exact-run>` and `verify audit`. Never batch-verify unrelated work or mark SUCCESS merely to clear the gate.

**LEARN?** — only for reusable, verified future value. Use `memory record` for proven root causes, decisions, workarounds, or gotchas; use `reflect record --task … --outcome …` for reusable lessons or owned fixes. Keep `--failure-signature "mechanism:X|cause:Y"`; use `--supersedes <id>` instead of stacking duplicates.

**CLEAN?** — only when live reads measure pressure: pending verification → run its declared check, then `verify mark` and `verify audit`, or explicitly abandon after review; stale memory → preview `/octocode-memory-digest` or `/octocode-memory-forget`; stale locks/signals → preview `lock prune` or `signal prune` with `--dry-run`. Dry-run cleanup first; review before mutation; never clean after every task. Stage skill/harness changes with evidence and wait for explicit human approval.

**PROJECT?** — run `repo inject` only when file readers need a refreshed snapshot. SQLite/live queries remain canonical; never hand-edit generated wiki files. If the CLI is unavailable, report that in the reply instead of pretending state was persisted.
</memory>
