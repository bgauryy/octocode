# Skill improve

Load when you rate, review, improve, refactor, or prune an Agent Skill (before ship, after a rewrite, or when review flags orphans or duplicates). Why: keep its job while the folder gets leaner and easier to navigate; every shipped file travels with the skill, and dead weight wastes context.

For a whole-skill review, inspect every behavioral route and report any unexamined surface.

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
2. Drop wrappers and stubs whose instruction fits the lobby.
3. Shape files per `references/skill-anatomy.md`.
4. Prune with § Cleanup below and report residual risk.

## Cleanup

Do not ship unused or duplicate files, development-only metadata, probes, drafts, scratch notes, old renames, nested `node_modules`, secrets, or files that only work inside another repository.

Runtime state, caches, browser profiles, and helper copies go under `<workspace>/.octocode/` or `<home>/.octocode/`, never beside `SKILL.md`. A script that writes to `process.cwd()/.octocode` fills the skill folder when launched there: relocate any `.octocode/` found inside a skill.

### Cleanup checklist

1. Reachability: route or delete each unreachable file (`unused-file`).
2. Internal references: vendor required files (`link-outside-skill`).
3. Duplicates: one owner and one copy per concept; cross-link, do not restate. Merge small or overlapping references into one owner page and update every route to it (grep the whole repository).
4. Routes: each used reference, script, and scheme has a route with its use condition.
5. Bloat: one coherent conditional job per file; a lean lobby.
6. Dead routes: remove lobby lines that match no real job, and their files.
