# Skill authoring

Load when you write or rewrite skill instructions, or create a local skill. Why: instructions decide agent behavior; resolve purpose, destination, and authority before writing.

## Sources

Start from real expertise: completed task sequences, user corrections, I/O examples, runbooks, schemas, review comments, incident fixes. Name tools, commands, edge cases, and recovery; never write "handle errors appropriately".

## Control level

- Flexible when several approaches work: explain why so the agent can adapt.
- Prescriptive when fragile, destructive, or order-dependent: give exact commands; forbid variants.
- Pick one default; give alternatives only as escape hatches.
- Teach a class of tasks, not one-off answers.

## Prose style

Write in STE-80, "80% of the way to" ASD-STE100: one idea per sentence (about 20 words), active voice, imperative steps, one term per concept, no rationale beyond the evidence. Full profile: the `octocode-documentation` skill, `style-ste80`. Show a flow, routing decision, or loop as one small Mermaid diagram (≤12 nodes) with a one-line caption; keep facts as text.

## Patterns that work

- Gotchas: naming mismatches, misleading health checks, required filters, tool quirks.
- Templates: short ones in `SKILL.md`; long or conditional ones in `assets/` with a load line.
- Checklists and validation loops: do → validate → fix → repeat; proceed only after pass.
- Plan-validate-execute for batch, stateful, or destructive work.

## Lobby convention

Below the H1, declare `tools:` (actual commands or host tools), `output:` (artifact/state destination, or none), and `routes:` (when to load supporting files). Add `related-skill: <skill-name>` only when useful; it grants no install authority.

## Outputs

Use `<workspace>/.octocode/` for workspace work and `<home>/.octocode/` only when no workspace applies or the artifact is user-scoped. Default durable artifacts to `<root>/<skill-name>/` and scratch to `<root>/tmp/<skill-name>/`. Chat-only results stay in chat. Approved source edits, installs, symlinks, and config use their named targets. If the root is unwritable, fail clearly; never switch roots silently.

## Machine-readable contracts

Add `scheme/<contract-name>.json` only when a tool, host, script, or evaluator parses it. Keep prose out of schemes; delete schemes that restate prose.

## Create a local skill

To fetch a remote skill first, load `references/install.md`.

1. Synthesize: user need, inspected sources, gates, resources, exclusions.
2. Plan: name, destination, trigger draft (`references/description-tuning.md`), flow outline, validation. An authorized creation request needs no second approval.
3. Write the lobby per `references/skill-anatomy.md` and this page. Put depth in one-concept references. Hooks → `references/hooks.md`; scripts → `references/skill-anatomy.md` § Scripts.
4. Optional: record consulted sources in a `references.md` file under `references/` (audit trail, not a load target). Fill only rows you used:

```markdown
# References

| Source | Owner/Repository or URL | Path or query | Used for |
|---|---|---|---|
```

5. Run `node scripts/skill-review.mjs <new-skill-dir>`; clear ERRORs before done.

Next: to extract helpers, load `references/skill-anatomy.md` § Scripts; before done, load `references/skill-review.md`.
