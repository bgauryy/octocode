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

- `frontmatter` — `SKILL.md` has a `---` block with both `name` and `description` (non-empty).
- `missing-reference` — every `references/<file>.md` linked in `SKILL.md` actually exists.
- `link-outside-skill` — markdown links in `SKILL.md` or any `references/*.md` must not point outside the skill folder via `../`, absolute local paths (`/`, `~/`), or `file://` URLs. A skill is installed as a self-contained folder; relative escapes break on install. Use a GitHub URL instead (e.g. `https://github.com/owner/repo/blob/main/path/to/file.md`).

WARN — lean/prompt hygiene (fix unless the domain justifies the exception, and say why):

- `description-style` — `description` is "Use when ..." style: imperative + a `when` clause stating triggers, ≤ 1024 chars. Focus on user intent, not internals.
- `description-concise` — the description LEAD is scannable: open `Use when …`, put the trigger/`when` clause in the **first ~50 chars** (that hook is what agents read first), no meta filler (`this skill`, `the following`, …) in the lead. Total length is NOT capped here — Anthropic's limit is 1024 (enforced by `description-style`) and descriptions should be trigger-rich/"pushy" to avoid undertriggering, so packing in concrete trigger phrases is encouraged, not penalized.
- `skill-too-long` — `SKILL.md` ≤ 100 lines. It holds only always-needed instructions; conditional detail moves to `references/`.
- `no-references` — `SKILL.md` links at least one `references/*.md`. Lean skills route detail out of the activation context.
- `link-no-condition` — every reference link states WHEN to load it (`when`/`if`/`before` ...). A bare "see references/" is too weak — the agent will not know which file matters.
- `reference-too-long` — each `references/*.md` ≤ 150 lines. Split larger files and cross-link them.
- `reference-name` — reference filenames are short, indicative, kebab-case (no generic `doc.md`/`notes.md`/`misc.md`).
- `duplicate-content` — the same sentence (≥ 12 words) appears in two or more skill files. Cross-file duplication inflates context and creates drift when one copy is updated. Consolidate into the canonical file and cross-link.
- `frontmatter-metadata` — frontmatter holds only keys the agent or installer needs (`name`, `description`, `allowed-tools`, `license`). Agents read just `name`+`description` at discovery, so authoring/repo keys (`version`, `author`, `tags`, `created`/`updated`, `category`, …) are dead weight in the activation context. Drop them or track them in the repo, not in `SKILL.md`.
- `metadata-section` — a body heading is authoring/repo metadata rather than a task instruction (`## Changelog`, `## Version History`, `## Author(s)`, `## Credits`, `## License`, `## Metadata`, `## Table of Contents`, `## TODO`, `## Maintainers`, …). The agent never acts on these; they only spend tokens. Keep changelogs, credits, and version notes in the repo README, not the skill.
- `rigid` — density of imperative modals (`MUST`/`NEVER`/`ALWAYS`/`FORBIDDEN`/`REQUIRED`) exceeds 12% of content lines. Rigid prompts break on legitimate edge cases. Prefer defaults with escape hatches; reserve these keywords for genuinely fragile, destructive, or order-dependent steps.
- `verbose` — filler phrases detected (`in order to`, `please note`, `make sure to`, `it is important`, etc.). These consume tokens without adding information. Cut or rewrite concisely.
- `tautology` — two adjacent narrative sentences share > 75% significant-token overlap. One is likely restating the other. Remove the weaker restatement. (Blockquotes and list items are exempt: parallel enumerated/quoted items — e.g. an Advocate line mirrored by a Critic line — are intentionally similar, not redundant.)
- `contradiction` — the same verb appears after both `MUST`/`ALWAYS` and `NEVER`/`MUST NOT`/`do not` in the same file. Conflicting instructions cause unpredictable agent behavior. Resolve to a single clear rule.

## Prompt rules the lint backs

- Lean over complete: every token in `SKILL.md` competes with conversation context. Cut anything the agent already does well without the skill.
- Not rigid, not verbose: prefer defaults with escape hatches over exhaustive menus; reserve MUST/NEVER for fragile, destructive, or order-dependent steps.
- No duplication: each fact lives in one place. Cross-link instead of repeating.
- No redundant data for agents: ship only what the agent reads to do the task. Authoring/repo metadata (extra frontmatter keys, changelogs, author/license/version sections) belongs in the repo, not in `SKILL.md` or `references/` where it burns activation tokens.
- Smart routing: `references/` files may link other `references/` files so an agent loads only the next file it needs — the lint counts these cross-links. Keep each reference single-purpose with a short indicative name.
- Runnable logic lives in `scripts/`, invoked from `SKILL.md` by relative path (`scripts/x.mjs`), never pasted inline.
- Deterministic over agentic: when a step is mechanical, repeatable, or token-heavy to spell out in prose, ship a `scripts/` helper and have `SKILL.md` *call* it. A script runs the same way every time and costs near-zero activation tokens; narrated steps get re-interpreted (and drift) on every run. Hand procedure to scripts; reserve natural-language instructions for genuine judgment. When reviewing or authoring a skill, flag any multi-step deterministic prose block that should be a script.

## Fixing a failing skill

1. Run the lint; group findings ERROR-first.
2. For `skill-too-long`/`no-references`: extract the conditional sections into short `references/*.md` with explicit load conditions in `SKILL.md`.
3. For `reference-too-long`: split by sub-topic and cross-link.
4. For `duplicate-content`: move the sentence to the canonical file; replace the other occurrence with a cross-link.
5. For `rigid`/`verbose`/`tautology`/`contradiction`: edit the offending lines directly; the lint message quotes the exact text.
5a. For `frontmatter-metadata`/`metadata-section`: delete the agent-irrelevant frontmatter keys and metadata headings (changelog/author/license/version); if the data is worth keeping, move it to the repo README.
6. For `description-concise`: rewrite the frontmatter `description` so chars 1–50 are `Use when <triggers>`; keep the rest trigger-rich and ≤1024 chars total (don't strip useful triggers to chase brevity).
7. Re-run until ERRORs clear; treat residual WARNs as a gated decision with the user.
