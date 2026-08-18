# Skill Review Rules

Load when interpreting or fixing review findings — after running `scripts/skill-review.mjs`. Why: map each code to the exact gap.

## ERROR (exit 1)

| Code | Meaning |
|------|---------|
| `frontmatter-missing` | `SKILL.md` starts with YAML frontmatter |
| `name-mismatch` | frontmatter `name` equals the folder name |
| `description-missing` | non-empty `description` |
| `description-too-long` | `description` ≤1024 chars |
| `missing-route` | every routed `references/*` and `scripts/*` path exists |
| `link-outside-skill` | no dependency on a file outside the folder — no `../dir/file`, `~/`, `file://`, or absolute path (a bare `../dir` argument is fine, as is a path carrying a `<placeholder>`) |

## WARN codes → fix

| Code | Fix |
|------|-----|
| `description-trigger` | lead with `Use when <trigger>` |
| `lobby-long` | keep `SKILL.md` lean; move depth into one-concept refs |
| `readme-missing` | add `README.md`: overview, capabilities, how it works, install |
| `reference-h1` | one short H1 per reference |
| `reference-long` | one concept per file; split and cross-link |
| `orphan-reference` | route it from `SKILL.md` or another reference, or delete it |
| `lobby-reference-unlisted` | name the reference in `SKILL.md` with when to read it |
| `lobby-script-unlisted` | name the runnable script in `SKILL.md` with when and how to run it |
| `lobby-workflow-missing` | show the workflow on its own line in `SKILL.md` — a `Flow:` line or a `## Workflow` heading, not trailing mid-sentence |
| `script-unreferenced` | a library nothing imports: import it, name it, or drop it |
| `route-condition` | state when or why on the same line as the ref or script |
| `reference-entry-cue` | open the chunk with `Load when …` and `Why:` |
| `reference-dead-end` | end with the next hop, or say the step ends here |
| `flow-phase-unrouted` | name each flow phase in a route or gate, or drop it from the flow |

Navigation codes read the skill as a map: `SKILL.md` is the lobby that lists every reference and runnable script with when and how to use it plus the workflows, each chunk declares its own entry, and each chunk points onward. Audit trails, templates, and fixtures are carried data and skip the entry/exit cues; a named directory (`scripts/hooks/`) stands in for the files under it.

## Judgment checks the script cannot make

| Check | Fix |
|-------|-----|
| Duplicate content | one owner per concept; cross-link instead of restating |
| Prose (rigid, verbose, tautology) | direct verb, named object, IF/THEN; cut filler without losing data |
| Output and gates | real markdown table for tabular data; complete gate sections |
| `description` quality | one `Use when`; intents, not internals; no MUST/NEVER/ONLY-skill, second `Triggers:`, or quote spam |
| Scripts and hooks | `--help` and flags; extract deterministic prose; route hook + `timeout`; a declared scheme is really exposed |
| Portability | runtime assumptions, deps, and secrets travel with the folder; link a sibling skill by name, never by path |

Key limits: each `references/*.md` ≤50 lines with one short H1; every reference and runnable script named in the lobby; every citation states why it matters.

Next: when re-running the loop load `references/skill-review.md`; for design rationale load `references/skill-anatomy.md`.
