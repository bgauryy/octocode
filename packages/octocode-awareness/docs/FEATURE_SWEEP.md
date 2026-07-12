# Full Feature Sweep

The multi-agent smoke proves work/lock/verify/signal collision handling; this lane
adds planning, learning, wiki generation, agent registry, and maintenance. Use one
scratch database/workspace so a real workboard is never touched. Command semantics
remain owned by [LOCKS.md](LOCKS.md), [REFLECTION.md](REFLECTION.md), and
[WIKI.md](WIKI.md).

```bash
SWEEP_WS="$(mktemp -d)"
SWEEP_DB="$SWEEP_WS/awareness.sqlite3"
S="$AWARENESS --db $SWEEP_DB"
$S maintenance init --compact
```

| Surface | Proof commands | Pass signal |
|---|---|---|
| Planning & task | `plan create --name … --objective … --lead-agent-id … --workspace $SWEEP_WS` → `task create --plan-id … --title … --reasoning … --acceptance … --path …` → `task depend --task-id <b> --depends-on <a>` → `task claim <b>` (expect blocked) → `task claim <a>` → `task submit` → `verify mark --status SUCCESS` → `task claim <b>` (expect success) → `task release` | dependent task is blocked on claim while its predecessor is unverified, then claimable once `verify mark --status SUCCESS` moves the predecessor to `DONE` |
| Multi-agent comms | `agent register` ×2 → `signal publish --kind … --subject … --body …` → `signal list` (peer, unread) → `signal reply --in-reply-to …` → `signal resolve` → `signal prune --resolved --older-than-days 1 --dry-run` | peer sees the signal once, reply shares the original `thread_id`, resolve closes the thread, prune previews `would_delete` without deleting fresh rows |
| Locks | `lock acquire` (agent A) → `lock acquire` (agent B, same file, expect exit `2` with `conflicts`) → `lock wait --workspace $SWEEP_WS --wait-seconds 2` (expect `lock_free:false`) → `lock release` → `lock prune` | conflict is reported before any wait; `lock wait` with `--workspace` correctly reports the held lock |
| Learning | `memory record --task-context … --observation … --label …` → `memory recall --query …` → `memory archive` → `memory restore` → `memory forget` → `refinement set --reasoning … --remember …` → `refinement get` → `refinement delete` → `reflect record --task … --outcome worked --lesson …` → `reflect mine-weakness` → `reflect export-harness` | recall finds the record by lexical score; archive/restore/forget round-trip; reflect auto-creates a learning memory; mine-weakness/export-harness return valid empty shapes on low-volume data |
| Wiki / repo context | `docs list` → `docs show <name>` → `docs staleness --targets-json '[{"docFile":"<doc>","sourceDirs":["<dir>"]}]'` → `wiki sync --workspace $SWEEP_WS --mode local --compact` | catalog lists all skill reference docs and `show` renders one; sync writes projections and `awareness/manifest.json`; manifest completeness is explained and Markdown budgets are within bounds |
| Updates / maintenance | `session capture --reason …` → `maintenance digest` → `maintenance self-test` | each returns `ok:true`; digest reports pressure/prune counts, not an error, on a fresh store |

Pass only when every command ran and its pass signal was observed. A missing,
errored, or silently wrong-workspace step is FAIL, not partial. Delete `$SWEEP_WS`
when done. Never record routine sweep output as memory in the real store; record
only a genuinely new, reusable finding.
