# Skill improve

Load when you rate, review, improve, refactor, or prune an Agent Skill (before ship, after a rewrite, or when review flags orphans or duplicates). Why: keep its job while the folder gets leaner and easier to navigate; every shipped file travels with the skill, and dead weight wastes context.

## Mode

| Request | Mode |
|---|---|
| Rate or review | score and report; no edits |
| Improve or refactor | inspect, patch, verify when the request authorizes edits |
| Apply prior findings | reuse this conversation's rating; do not repeat it |

Ask only when write authority or the needed outcome is unclear.

## Inspect first

1. Read the target `SKILL.md`, inventory its files, and read every file that affects the change. For a whole-skill review, inspect every behavioral route and report any unexamined surface.
2. Run `scripts/skill-review.mjs <skill-dir>` (`references/skill-review.md`).
3. Never rewrite from a summary.

## Rate-only report

```text
Overall:     <score>/10 — <grade> — <one sentence>
Score card:  trigger/workflow/evidence/gates/UX/specificity/portability/risk → High|Med|Low
Issues:      Critical / High / Medium / Low — each with file:line
Strengths:   2-4 bullets to preserve
Residual:    1-3 risks
```

## Improve loop

`READ → MAP INTENT → RATE → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

1. Keep the core job. Score it with `references/quality.md`.
2. Keep one owner per concept. Cross-link; do not restate. Merge parallel near-duplicate references; drop wrappers and stubs whose instruction fits the lobby.
3. Show a flow, routing decision, or loop as one small Mermaid diagram (≤12 nodes) plus a one-line caption, not a long paragraph. Keep thresholds, commands, and paths as text.
4. Write in STE-80 (`references/skill-authoring.md`). Shape files per `references/skill-anatomy.md`.
5. Prune orphans with § Cleanup below, re-review to 0 ERROR, and report residual risk.

Measure behavior changes with `octocode-eval-benchmark`.

## Cleanup

Do not ship unused or duplicate files, development-only metadata, probes, drafts, scratch notes, old renames, nested `node_modules`, secrets, or files that only work inside another repository.

A skill folder is never an artifact root. Runtime state, caches, browser profiles, and helper copies go under `<workspace>/.octocode/` or `<home>/.octocode/`, never beside `SKILL.md`. A script that writes to `process.cwd()/.octocode` fills the skill folder when launched there: relocate any `.octocode/` found inside a skill.

### Cleanup checklist

1. Reachability: every file is reachable from `SKILL.md`, `README.md`, or another used file (`unused-file`). Route it or delete it.
2. Internal references: every local reference resolves inside the skill; vendor required files (`link-outside-skill`).
3. Duplicates: one owner and one copy per concept. Merge small or overlapping references into one owner page and update every route to it (grep the whole repository).
4. Routes: each used reference, script, and scheme has a route with its use condition.
5. Bloat: one coherent conditional job per file; a lean lobby. Replace long flow prose with one small Mermaid diagram.
6. Dead routes: remove lobby lines that match no real job, and their files.

Ask before deleting a non-empty file when unsure. Then `scripts/skill-review.mjs` must report 0 ERROR.
