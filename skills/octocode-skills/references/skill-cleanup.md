# Skill Cleanup

Load when pruning a skill before ship, after improve/dedupe, or when review flags orphans/duplicates. Why: skills ship as a **standalone folder** — every file travels with the skill; dead weight wastes context and confuses agents.

## Standalone ship shape

A skill is one portable directory. Install/sync copies or symlinks that folder as-is. There is no monorepo outside it at runtime.

Keep only what the agent needs to run the skill:

```text
skill-name/
|-- SKILL.md          # required lobby
|-- README.md         # human overview (review requires)
|-- references/       # one-concept depth, all reachable
|-- scripts/          # deterministic helpers actually routed
|-- assets/           # templates/resources actually used
`-- references/references.md  # audit trail only (optional)
```

Do not ship: drafts, scratch notes, old renames, vendor copies, nested `node_modules`, secrets, or files that only make sense inside another repo.

## Cleanup checklist

1. **Orphan refs** — every `references/*.md` reachable from `SKILL.md` or another ref (`orphan-reference`); else delete or route.
2. **Orphan scripts/assets** — every agent-facing script listed in the lobby; unused assets → remove or document.
3. **Duplicate content** — one owner per concept; delete restated paragraphs; cross-link (`duplicate-content`).
4. **Broken links** — no missing `references/` or `scripts/` targets; no links escaping the skill folder.
5. **Bloat** — `SKILL.md` ≤50 lines; each ref ≤50, one H1; drop authoring metadata from agent files.
6. **Dead routes** — lobby links that no longer match a real job → remove the line and the file if unused.

## Phase

Run cleanup after DEDUPE/REWRITE and before claiming done:

`… → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

Gate deletes of non-empty files behind user approval when unsure. Then `scripts/skill-review.mjs` must report 0 ERROR.

Next: when rewriting load `references/skill-improve.md`; before done load `references/skill-review.md`.
