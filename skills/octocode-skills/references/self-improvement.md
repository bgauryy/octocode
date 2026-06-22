# Self-Improvement Mode

Load when the user asks to rate, review, score, improve, refactor, or lint a `SKILL.md` — theirs or someone else's. Read `agent-skills-guide.md` before rating or rewriting, and run `scripts/skill-lint.mjs` (see `skill-lint.md`) first for the mechanical structure/prompt-hygiene pass.

## Pick a mode

If the request is ambiguous (e.g. "check my skill"), present this gate first:

```text
Which mode?
1. Rate-only — score and report issues; no file edits.
2. Improve / refactor — fix issues and rewrite; gate before writing.
3. Fix all — apply fixes from a prior rating in this conversation; skip re-rating.
4. Cancel.
```

- `Rate-only` (rate, review, score, audit): stop after REPORT. MUST NOT edit files. End with a numbered next-action gate (apply fixes, show diff, cancel).
- `Improve` / `refactor` / `rewrite`: full flow including REWRITE and VALIDATE; gate before writing.
- `Fix all` / `apply fixes`: skip MAP INTENT and RATE ISSUES if a prior rating exists; go straight to REWRITE → VALIDATE → REPORT.

## Flow

`READ -> MAP INTENT -> RATE ISSUES -> [REWRITE -> VALIDATE] -> REPORT`

Read: the full target `SKILL.md` and all behavior-affecting referenced files; note purpose, line count, resources, gates, output format.

Map intent: preserve the skill's core job, trigger domain, and user-facing promises; identify what behavior must become more reliable (activation, research quality, safety gates, tool routing, output shape, recovery).

Rate issues: run the lint, then check weak rules in critical sections, vague actions, raw-search handoff, missing gates, unsafe writes, missing verification, stale references, line-count bloat. Group by `Critical`/`High`/`Medium`/`Low`, cite `file:line`. Score per dimension using `quality-rubric.md`.

Rewrite (skip in Rate-only): fix Critical+High first; keep `SKILL.md` lean (the lint targets ≤100 lines); move long examples/schemas/static refs into `references/` with explicit load conditions; keep `description` trigger-rich (see `description-tuning.md`) without keyword stuffing.

Validate (skip in Rate-only): valid `name`+`description`; clear steps, gates, recovery, output UX; referenced files exist (or missing ones documented as risks); critical actions use MUST/NEVER/FORBIDDEN where needed; no write/install bypasses a user gate. Re-run `scripts/skill-lint.mjs` until ERRORs clear.

## Report

Rate-only shape:

```text
Overall:        <score>/10 — <letter grade> (one-sentence summary).
Score card:     per-dimension High/Medium/Low using quality-rubric.md.
Issues:         grouped Critical / High / Medium / Low, each with file:line.
Validation:     pass/fail per checklist item above.
Strengths:      2-4 bullets worth preserving.
Residual risk:  1-3 bullets.
Next action:    numbered choices ending with "Cancel".
```

Improve / Fix all: summarize intent preserved, major fixes applied, lint + validation result, and any residual risk.
