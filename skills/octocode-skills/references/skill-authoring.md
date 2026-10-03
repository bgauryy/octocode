# Skill authoring

Load when you write or rewrite skill instructions, or create a local skill. Why: instructions decide agent behavior; resolve purpose, destination, and authority before writing.

## Sources

Start from real expertise: completed task sequences, user corrections, I/O examples, runbooks, schemas, review comments, incident fixes. Name tools, commands, edge cases, and recovery; never write "handle errors appropriately".

## Control level

- Flexible when several approaches work: explain why so the agent can adapt.
- Prescriptive steps forbid variants.
- Teach a class of tasks, not one-off answers.
- Explain why instead of all-caps MUST or NEVER; keep hard rules for real gates.

## Prose style

STE-80 is "80% of the way to" ASD-STE100: one idea per sentence (about 20 words), active voice, imperative steps, one term per concept, no rationale beyond the evidence. Full profile: the `octocode-documentation` skill, `style-ste80`.

## Patterns that work

- Gotchas: naming mismatches, misleading health checks, required filters, tool quirks.
- Templates: short ones in `SKILL.md`; long or conditional ones in `assets/` with a load line.
- Checklists and validation loops: do → validate → fix → repeat; proceed only after pass.
- Plan-validate-execute for batch, stateful, or destructive work.
- Examples: concrete input → output pairs show a style better than a description of it.
- No time-sensitive text ("before August, use X"). Put a deprecated form under an `Old patterns` heading.
- MCP tools: write the full server-prefixed name (`<server>:<tool>`), so the agent finds the tool when several servers load.
- No surprise: no hidden network, data upload, or behavior that the description does not state.

## Lobby header

`tools:` names actual commands or host tools; `output:` names the artifact or state destination. `related-skill:` grants no install authority.

## Machine-readable contracts

Add `scheme/<contract-name>.json` only when a tool, host, script, or evaluator parses it. Keep prose out of schemes; delete schemes that restate prose.

## Create a local skill

To fetch a remote skill first, load `references/install.md`.

1. Synthesize: user need, inspected sources, gates, resources, exclusions.
2. Plan: name and frontmatter (`references/frontmatter.md`), destination, trigger draft (`references/description-tuning.md`), flow outline, validation.
3. Write the lobby per `references/skill-anatomy.md` and this page. Put depth in one-concept references. Hooks → `references/hooks.md`; scripts → `references/skill-anatomy.md` § Scripts.
4. Optional: record consulted sources in a `references.md` file under `references/` (audit trail, not a load target). Fill only rows you used:

```markdown
# References

| Source | Owner/Repository or URL | Path or query | Used for |
|---|---|---|---|
```

5. Run the review gate.

## Evaluate first

1. Run the task without the skill. Record each failure and each fact the agent lacked.
2. From those gaps, write three or more eval prompts: realistic, varied, one edge case, each with an expected output. Keep them in `<workspace>/.octocode/tmp/<skill-name>/evals/`, not in the skill folder.
3. Baseline: no skill for a new skill; a snapshot of the old copy for an edit. Add assertions after the first run.
4. Write the minimum text that passes. Rerun with and without the skill.
5. Iterate with two agents: one edits the skill, a fresh one uses it on real tasks. Watch what it reads, skips, rereads, or misses; fix the structure, not only the wording.
6. Run the evals on each target model; a smaller model needs more detail.

Next: to extract helpers, load `references/skill-anatomy.md` § Scripts; before done, load `references/skill-review.md`.
