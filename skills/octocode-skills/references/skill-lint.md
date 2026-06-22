# Skill Lint

Load when the user asks to lint, audit, or check a skill's structure/prompt hygiene, or after creating/editing a `SKILL.md`. For deeper quality scoring (workflow, gates, evidence) pair with `agent-skills-guide.md`; this file is the mechanical structure check.

## Run it

```bash
node scripts/skill-lint.mjs                       # lint every skill under the repo skills/ root
node scripts/skill-lint.mjs ../some-skill         # lint one or more skill folders
node scripts/skill-lint.mjs ../some-skill --json  # machine-readable findings
```

Exit `1` if any ERROR is found; WARN is advisory. Always run it before reporting a created/edited skill as done, and surface the findings.

## What it enforces

ERROR — must fix:

- `frontmatter` — `SKILL.md` has a `---` block with both `name` and `description`.
- `missing-reference` — every `references/<file>.md` linked in `SKILL.md` actually exists.

WARN — lean/prompt hygiene (fix unless the domain justifies the exception, and say why):

- `description-style` — `description` is "Use when ..." style: imperative + a `when` clause stating triggers, ≤ 1024 chars. Focus on user intent, not internals.
- `skill-too-long` — `SKILL.md` ≤ 100 lines. It holds only always-needed instructions; conditional detail moves to `references/`.
- `no-references` — `SKILL.md` links at least one `references/*.md`. Lean skills route detail out of the activation context.
- `link-no-condition` — every reference link states WHEN to load it (`when`/`if`/`before` ...). A bare "see references/" is too weak — the agent will not know which file matters.
- `reference-too-long` — each `references/*.md` ≤ 150 lines. Split larger files and cross-link them.
- `reference-name` — reference filenames are short, indicative, kebab-case (no generic `doc.md`/`notes.md`/`misc.md`).

## Prompt rules the lint backs

- Lean over complete: every token in `SKILL.md` competes with conversation context. Cut anything the agent already does well without the skill.
- Not rigid, not verbose: prefer defaults with escape hatches over exhaustive menus; reserve MUST/NEVER for fragile, destructive, or order-dependent steps.
- Smart routing: `references/` files may link other `references/` files so an agent loads only the next file it needs — the lint counts these cross-links. Keep each reference single-purpose with a short indicative name.
- Runnable logic lives in `scripts/`, invoked from `SKILL.md` by relative path (`scripts/x.mjs`), never pasted inline.
- Deterministic over agentic: when a step is mechanical, repeatable, or token-heavy to spell out in prose, ship a `scripts/` helper and have `SKILL.md` *call* it. A script runs the same way every time and costs near-zero activation tokens; narrated steps get re-interpreted (and drift) on every run. Hand procedure to scripts; reserve natural-language instructions for genuine judgment. When reviewing or authoring a skill, flag any multi-step deterministic prose block that should be a script.

## Fixing a failing skill

1. Run the lint; group findings ERROR-first.
2. For `skill-too-long`/`no-references`: extract the conditional sections into short `references/*.md` with explicit load conditions in `SKILL.md`.
3. For `reference-too-long`: split by sub-topic and cross-link.
4. Re-run until ERRORs clear; treat residual WARNs as a gated decision with the user.
