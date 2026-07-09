# Homeostatic Awareness Loop

Use this for the intuition behind sensing, work, learning, cleanup, and publication. It explains **housekeep (cleanup)**; read `references/bookkeeping.md` for triggers and queue ownership. The metaphor is guidance, not authority; drive lives in `references/drive-state.md`.

`sleep` and a dedicated trust-gate CLI are NOT SHIPPED. Use `maintenance digest --dry-run`, explicit forget/prune, verification, and projection budgets.

## Organ Map

| Function | Surface | Agent rule |
|---|---|---|
| Senses | status, workboard, docs staleness | Sense before acting. |
| Attention | `attend`, targeted recall | Select a compact relevant packet. |
| Memory | recall/record/reflect | Store only durable future value. |
| Error signal | checks, conflicts, corrections | Learn from verified outcomes. |
| Immune prune | supersession, forget dry-run | Review weak/stale/unsafe rows. |
| Cleanup | digest/prune dry-runs | Report before mutation. |
| Corpus/bridge | signals, refinements, file work | Share traceable state, not hidden chat. |
| Executive control | choose, declare, verify | Close action with evidence. |
| Projection | query, `repo inject` | Publish selectively; SQLite stays canonical. |

## Cycle

```text
SENSE -> ATTEND -> CHOOSE/DECLARE -> ACT -> VERIFY -> REFLECT
  ^                                         |
  |                                         v
ATTEND <- PROJECT <- PRUNE <- CONSOLIDATE <- CAPTURE
```

The loop closes only when reflection is routed, applied, verified, and its row is terminal; see `references/learning-loop.md`. Publication is optional and happens only when future readers need files.

## Learning Rules

- Failures, lock conflicts, stale docs, corrections, and recall misses are signals for bounded reflection.
- Durable memories need scope, provenance, and a future-use reason.
- Prefer supersession/archive/dry-run over destructive deletion.
- Preserve signals/refinements/work until their owner acts and closes them.
- Keep Markdown bounded; use CSV/HTML/query for complete sortable data.
- Retrieved memory, generated wiki, and role dialogue are leads; current evidence wins.

Start with `attend --compact`; use `query workboard` for current pressure, `reflect mine-weakness` for repeated failures, digest/forget/prune dry-runs for cleanup, and `repo inject` only for useful publication.
