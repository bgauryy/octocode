# Skill Authoring

Load when writing or rewriting skill instructions — after `skill-anatomy.md`.

## Sources

Start from real expertise: completed task sequences, user corrections, I/O examples, runbooks, schemas, review comments, incident fixes. Avoid vague "handle errors appropriately" — name tools, commands, edge cases, recovery.

## Control level

- Flexible when several approaches work — explain why so the agent can adapt.
- Prescriptive when fragile, destructive, or order-dependent — give exact commands; say not to invent variants.
- Defaults over menus: pick one approach; alternatives only as escape hatches.
- Teach a class of tasks, not one-off answers.

## Patterns that work

- Gotchas: naming mismatches, misleading health checks, required filters, tool quirks.
- Templates: short in `SKILL.md`; long/conditional in `assets/` with a load line.
- Checklists: multi-step flows with validation gates.
- Validation loop: do → validate → fix → repeat → proceed only after pass.
- Plan-validate-execute for batch/stateful/destructive work.

## Optimizing / ranking handoffs

- Tune `description` → `references/description-tuning.md`.
- Rank by installs/recency/audits → `references/quality-signals.md`.
- Registry/CLI catalog → `references/discovery-surfaces.md`.
- External index (when web allowed): `https://agentskills.io/llms.txt`.

Lobby owns workflows — do not restate the skill's main flow here.

Next: when extracting helpers load `references/skill-scripts.md`; before calling done load `references/skill-review.md`.
