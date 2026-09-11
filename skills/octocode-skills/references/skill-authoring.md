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

## Lobby convention

Below the H1, declare `tools: npx octocode / octocode-mcp`, one `related-skill: <skill-name>`, and `output: <workspace>/.octocode/ for workspace work | <home>/.octocode/ when no workspace applies`. Keep short shared decisions in the lobby. Add a reference, doc, script, or scheme only for a coherent conditional job that changes the next action more effectively than inline guidance.

## Workspace outputs

Every lobby states where generated artifacts go. Use `<workspace>/.octocode/` for repository or workspace-scoped work and `<home>/.octocode/` only when no workspace applies or the artifact is explicitly user-scoped. Default durable artifacts to `<root>/<skill-name>/` and scratch/run data to `<root>/tmp/<skill-name>/`; a stable specialized namespace under the selected root is fine. Keep chat-only results in chat. User-approved source edits, installs, symlinks, and configuration use their named targets. If the selected root is unwritable, fail clearly rather than silently switching roots.

## Machine-readable contracts

Add `scheme/` only when a tool, host, script, or evaluator needs a contract it can parse. Store one contract per `scheme/<contract-name>.json`; each file is valid JSON with one top-level object. Keep explanatory prose in the lobby or references, route the scheme from its consumer, and delete schemes that merely restate prose.

## Optimize and rank handoffs

- Tune `description` → `references/description-tuning.md`.
- Rank by installs/recency/audits → `references/quality-signals.md`.
- Registry/CLI catalog → `references/discovery-surfaces.md`.
- External index (when web allowed): `https://agentskills.io/llms.txt`.

Lobby owns workflows — do not restate the skill's main flow here.

Next: when extracting helpers load `references/skill-scripts.md`; before calling done load `references/skill-review.md`.
