# Quality Rubric

Load when judging a skill candidate. For evidence signals beyond stars, also read `quality-signals.md`.

For every plausible candidate, inspect enough `SKILL.md` content to understand behavior. For strong, risky, or ambiguous candidates, inspect full `SKILL.md` plus referenced scripts, templates, install docs, evals, or reference files that affect execution.

## Evaluate

- Trigger: clear activation conditions and non-activation boundaries.
- Workflow: ordered steps, decision points, recovery paths, stop conditions.
- Evidence: real file contents, referenced resources, tests, examples, scripts.
- Gates: validation, approval, preview, review, permissions, rollback, install-conflict handling.
- Output UX: concise results, useful comparison cards, explicit next-step gate.
- Specificity: domain knowledge an agent would not have by default.
- Portability: agent/runtime assumptions, hardcoded paths, external services, dependencies, secrets.
- Risk: unsafe commands, hidden network actions, missing referenced files, license ambiguity, stale docs, broad triggers.

## Quality labels

- `High`: direct match, clear trigger, executable workflow, useful resources and gates, no obvious safety or portability red flags.
- `Medium`: partial match or adaptable, but missing some validation, UX, or domain detail.
- `Low`: keyword-only match, generic workflow, unclear trigger, stale pattern, or meaningful caveat.
