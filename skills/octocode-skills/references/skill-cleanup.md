# Skill Cleanup

Load when pruning a skill before ship, after improve/dedupe, or when review flags orphans/duplicates. Why: skills ship as a **standalone folder** — every file travels with the skill; dead weight wastes context and confuses agents.

## Standalone ship shape

A skill is one portable directory. Install/sync copies or symlinks that folder as-is. Every local file reference must resolve inside it; sibling skills are named capabilities, never file dependencies. <!-- style-lint: ignore-line passive-voice -->

Keep only what the agent needs to run the skill:

```text
skill-name/
|-- SKILL.md          # required lobby
|-- README.md         # human overview (review requires)
|-- references/       # one-concept depth, all reachable
|-- scripts/          # deterministic helpers actually routed
|-- assets/           # templates/resources actually used
|-- scheme/           # optional JSON contracts, one per file
`-- references/references.md  # audit trail only (optional)
```

Do not ship: unused or duplicate files, development-only metadata, probes, drafts, scratch notes, old renames, nested `node_modules`, secrets, or files that only make sense inside another repository.

**A skill folder is never an artifact root.** Task runtime state, caches, browser profiles, and second copies of a helper belong under `<workspace>/.octocode/` for workspace work or `<home>/.octocode/` when no workspace applies — never beside `SKILL.md`. Application-owned global configuration follows that application's contract. A script that resolves its output base as `process.cwd()/.octocode` writes into whichever directory launched it, so a run started from the skill folder silently fills it; treat a git-ignored `.octocode/` inside a skill as that accident and relocate it.

## Cleanup checklist

1. **Whole-folder reachability** — every file is reachable from `SKILL.md`, `README.md`, or another used file (`unused-file`); otherwise delete or route it.
2. **Internal references** — every local file reference resolves inside the skill; vendor required files and name optional sibling skills without file paths (`link-outside-skill`).
3. **Duplicate content/files** — one owner and one shipped copy per concept; delete restated prose, routing wrappers, fallback stubs, duplicate assets, development-only metadata, and probes. Move a short shared decision into the lobby.
4. **Routes** — each used reference, agent-facing script, and scheme has a reachable route with its use condition; keep detailed catalogs in one place.
5. **Bloat and effectiveness** — keep one coherent conditional job per reference/doc/script/scheme and a lean lobby. Retain a hop only when it changes the next action and saves context or improves execution; assess line-count warnings for actual duplication or difficult navigation rather than splitting coherent procedures.
6. **Dead routes** — lobby links that no longer match a real job → remove the line and the file if unused.

## Phase

Run cleanup after DEDUPE/REWRITE and before claiming done:

`… → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

Gate deletes of non-empty files behind user approval when unsure. Then `scripts/skill-review.mjs` must report 0 ERROR.

Next: when rewriting load `references/skill-improve.md`; before done load `references/skill-review.md`.
