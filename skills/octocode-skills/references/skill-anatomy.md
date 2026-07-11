# Skill Anatomy

Load when evaluating, improving, or creating a skill's folder shape — before rewriting structure.

A skill is a **standalone folder** with `SKILL.md` (required) plus optional `scripts/`, `references/`, `assets/`. Install/sync ships that folder as-is — prune dead files via `references/skill-cleanup.md` before done.

```text
my-skill/
|-- SKILL.md       # metadata + operating map
|-- scripts/       # deterministic helpers
|-- references/    # one-concept depth
|-- assets/        # templates / resources
```

## Progressive disclosure

1. Discovery — agent sees only `name` + `description`.
2. Activation — matching task → full `SKILL.md`.
3. Execution — load refs/scripts only when the map says so.

`SKILL.md` is the lobby: workflows, hard rules, stop conditions, and the route table live there. Refs never redefine the main flow.

## Reference discipline

- One short H1, one concept per file, ≤ 50 lines; one owner per concept (no overlaps).
- Every link states WHEN and WHY; load one ref at a time.
- Ref→ref OK for depth — end with the next load when needed.
- Gotchas stay in the lobby only if the agent must know them before the trigger.
- Tabular content → a real markdown table, never prose describing rows/columns.
- Every reference/citation states why it matters — no bare links.
- Every sentence earns its tokens: dense, no filler, no duplicate phrasing, no data loss.

## Context cut

Ask: "Would the agent get this wrong without the skill?" If not, cut.
Prefer stepwise guidance over exhaustive docs. Keep each skill a coherent unit of work.

Next: when improving an existing skill load `references/skill-improve.md`; when writing instructions load `references/skill-authoring.md`; before bundling scripts load `references/skill-scripts.md`.
