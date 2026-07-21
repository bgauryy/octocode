# Homeostatic Awareness Loop

Use this for the living-system intuition behind work, learning, cleanup, and publication. It is a human/agent-in-the-loop software control model—not sentience, a persona, or authority. `bookkeeping.md` owns triggers; `drive-state.md` owns diagnostics.

## Control Contract

| Pressure | Sensor | Bounded actuator | Guard |
|---|---|---|---|
| Context | attend/hook bytes, workboard size | targeted reads, caps, fingerprints | preserve omissions/errors; human thesis never auto-loads |
| Coordination | file presence, claims, locks, signals | CHOOSE/DECLARE, signal, sensitive lock | ordinary overlap stays allowed; locks do not authorize edits |
| Verification | pending/stale runs | declared check + `verify mark` | TTL/end/submit never mean success |
| Memory | stale/missing refs, weak recall | reflect, supersede, digest/forget preview | retrieved rows are leads; dry-run before removal |
| Projection | manifest budgets/staleness | optional `wiki sync` | SQLite stays canonical; wiki is not a live sensor |
| Harness | recurring failures/evals | proposal + human apply | held-out validation; no silent self-edit |

## Loop

```text
SENSE -> ATTEND -> CHOOSE/DECLARE -> ACT -> VERIFY -> REFLECT
  ^                                                   |
  `- REMEASURE <- PROJECT? <- HYGIENE <- REPLAY <- CAPTURE
```

The loop closes only when its output has an owner, is applied, freshly verified, terminal, and remeasured. Publication is optional. “Metabolism” means reviewed replay/hygiene; no `sleep` or dedicated trust-gate CLI is shipped.

## Rules

- Measure before and after an intervention; keep it only if target pressure falls without a quality/safety regression.
- Store scoped, provenance-linked future value—not routine status or raw dialogue.
- Prefer supersession/archive/dry-run; explicit live digest/prune/forget may mutate after review.
- Preserve work/signals/open-or-ongoing refinements until their owner acts and closes them; digest may age-prune terminal `done` refinements only.
- Keep agent context bounded; use targeted query, CSV, or HTML for complete data.
- Treat memory, generated wiki, drive fields, and role dialogue as diagnostic leads; current user instructions, source, and tests win.

Start with `attend --compact`; inspect targeted pressure; use reflection only for reusable outcomes; preview cleanup; re-run live reads after action; sync only when file readers need a snapshot.
