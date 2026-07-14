# Quality Rubric

Load when judging a skill candidate. Why: content fit beats stars. For installs/recency/audits also read `quality-signals.md`.

Inspect enough `SKILL.md` to understand behavior. For strong/risky/ambiguous candidates, read full `SKILL.md` plus scripts, templates, install docs, and refs that affect execution.

## Dimensions

- Trigger — activation + non-activation boundaries.
- Workflow — ordered steps, decisions, recovery, stop conditions.
- Evidence — real files, resources, tests, examples, scripts.
- Gates — validation, approval, preview, permissions, rollback, conflicts.
- Output UX — concise results, comparison cards, next-step gate.
- Specificity — domain knowledge the agent lacks by default.
- Portability — runtime assumptions, hardcoded paths, deps, secrets.
- Risk — unsafe commands, hidden network, missing refs, license, stale docs, broad triggers.

## Labels

- `High` — direct match, clear trigger, executable workflow, useful gates, no obvious red flags.
- `Medium` — partial/adaptable; missing some validation, UX, or domain detail.
- `Low` — keyword-only, generic, unclear trigger, stale, or meaningful caveat.

Next: when ranking load `references/quality-signals.md`; when presenting load `references/output-format.md`; when rewriting load `references/self-improvement.md`.
