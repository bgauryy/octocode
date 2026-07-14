# Skill Review Rules

Load when interpreting or fixing review findings — after running `scripts/skill-review.mjs`. Why: map each code to the exact gap.

## ERROR (exit 1)

| Rule | Meaning |
|------|---------|
| `frontmatter` | `---` with non-empty `name` + `description` |
| `missing-readme` | `README.md` present |
| `missing-reference` | every mentioned `references/*.md` exists (fenced + `references*.md` exempt) |
| `missing-script` | every mentioned `scripts/<file>` exists |
| `missing-scheme-script` | Protocol/scheme declared → script exposes it |
| `link-outside-skill` | no `../`, `/`, `~/`, `file://` links — use GitHub URL |

## WARN groups → fix

| Group | Fix |
|-------|-----|
| Length (`skill-too-long`, `reference-too-long`, `duplicate-content`) | `SKILL.md` and each ref ≤50 lines, one concept; cross-link |
| Routing (`*-routing`, `*-map-complete`, `route-description`, `link-no-condition`, `orphan-reference`) | conditional root routes in `SKILL.md`; every other ref graph-reachable from one root; same-line when/why for direct refs/scripts |
| README (`readme-*`) | overview, features, how-it-works, audiences, `npx octocode skill` install |
| Scripts/hooks (`script-quality`, `deterministic-prose`, `hooks-*`) | `--help`/flags, extract script, route hook + `timeout` |
| Frontmatter/metadata | drop authoring keys/headings → README |
| Prose (`rigid`, `verbose`, `clarity`, `tautology`, `duplicate-content`) | direct verb, named object, IF/THEN; cut filler — dense, no redundant tokens, no data loss |
| Output/gates | concrete template — real markdown table for tabular data, not prose; complete gate sections |
| `description-concise` | chars 1–50 = `Use when <trigger>`; ≤1024 total |
| `description-rigid` | no MUST/NEVER/ALWAYS/SOLE/ONLY-skill; no internals dump — intents only |
| `description-redundant` | one Use-when; no second `Triggers:`; no "This skill should be used"; no quote spam |

Key limits: `SKILL.md` ≤50 lines; each `references/*.md` ≤50 lines, one short H1; every non-audit ref listed from `SKILL.md` or reachable via ref→ref; every citation states why it matters.

Next: when re-running the loop load `references/skill-review.md`; for design rationale load `references/skill-anatomy.md`.
