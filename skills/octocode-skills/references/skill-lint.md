# Skill Lint

Load when the user asks to lint, audit, or check a skill's structure/prompt hygiene, or after creating/editing a `SKILL.md`. For deeper quality scoring (workflow, gates, evidence) pair with `agent-skills-guide.md`; this file is the mechanical structure check.

## Run it

```bash
node scripts/skill-lint.mjs                       # lint every skill under the nearest parent skills/ root
node scripts/skill-lint.mjs ../some-skill         # lint one or more skill folders
node scripts/skill-lint.mjs ../some-skill --json  # machine-readable findings
```

Exit `1` if any ERROR is found; WARN is advisory. Always run it before reporting a created/edited skill as done, and surface the findings.

No-arg scans are relative to the linter copy being run: the local `.agents/skills/octocode-skills` copy scans `.agents/skills`, while the packaged `skills/octocode-skills` copy scans `skills`.

## What it enforces

ERROR — fix:

- `frontmatter` — `SKILL.md` has a `---` block with both `name` and `description` (non-empty).
- `missing-readme` — the skill folder has no `README.md`. Every skill needs a human-facing overview, features, how-it-works, and install guide.
- `missing-reference` — every `references/<file>.md` mentioned in `SKILL.md` or behavior references actually exists; fenced examples and `references/references*.md` audit-trail files are ignored.
- `missing-script` — every `scripts/<file>` mentioned from `SKILL.md` or frontmatter hooks exists in the bundled `scripts/` folder. Deterministic helpers are part of the skill contract; broken paths mean the installed skill cannot run as described.
- `link-outside-skill` — markdown links in `SKILL.md` or any `references/*.md` must not point outside the skill folder via `../`, absolute local paths (`/`, `~/`), or `file://` URLs. Fenced examples are ignored. A skill is installed as a self-contained folder; relative escapes break on install. Use a GitHub URL instead (e.g. `https://github.com/owner/repo/blob/main/path/to/file.md`).

WARN — lean/prompt hygiene (fix unless the domain justifies the exception, and say why):

- `description-style` — `description` is "Use when ..." style: imperative + a `when` clause stating triggers, ≤ 1024 chars. Focus on user intent, not internals.
- `description-concise` — the description LEAD is scannable: open `Use when …`, put the trigger/`when` clause in the **first ~50 chars** (that hook is what agents read first), no meta filler (`this skill`, `the following`, …) in the lead. Total length is NOT capped here — Anthropic's limit is 1024 (enforced by `description-style`) and descriptions should be trigger-rich/"pushy" to avoid undertriggering, so packing in concrete trigger phrases is encouraged, not penalized.
- `skill-too-long` — `SKILL.md` ≤ 50 lines. After 50 lines, separate conditional detail into `references/`; keep the activation file as the smart map, not the manual.
- `skill-map-summary` — `SKILL.md` should summarize how the skill works near the top: flow, modes, routing, or core steps. Agents read `SKILL.md` first, so it should be an efficient context map before detail.
- `no-references` — `SKILL.md` links at least one `references/*.md`. Lean skills route detail out of the activation context.
- `capability-routing` — when a skill bundles several references, `SKILL.md` should directly mention enough of them to act as a capability map. Each major capability should route to a focused reference or script, not live as long prose.
- `reference-map-complete` — every bundled non-audit `references/*.md` file is listed in `SKILL.md`. The main skill must be the complete concise map; reference-to-reference links are still useful but not enough.
- `script-routing` — when `scripts/` exists, `SKILL.md` should mention `scripts/` so deterministic capabilities are discoverable and not reimplemented in prose.
- `script-map-complete` — every agent-facing, non-hook code script under `scripts/` is listed in `SKILL.md`. Internal hook adapters are covered by hook checks instead.
- `route-description` — each listed reference/script has a concise same-line purpose or load condition (3-28 words). A bare path is not enough; a long paragraph belongs in the reference.
- `script-quality` — agent-facing scripts should communicate clearly: accept explicit flags/env/stdin, provide `--help`/usage text, and run unattended. Interactive prompts are flagged because agents cannot reliably answer them mid-run.
- `deterministic-prose` — a long numbered, command-like procedure appears in prose without a `scripts/` helper. Move mechanical, repeatable work into scripts and keep prose for judgment, gating, and interpretation.
- `installation-section` — skills that bundle install-related references or install scripts need an `Installation` section in `SKILL.md` that routes install behavior, gates, and verification.
- `readme-overview` — `README.md` should start with an H1 and high-level explanation of what the skill does and when to use it.
- `readme-features` — `README.md` should describe the visible features/capabilities users can expect.
- `readme-how-it-works` — `README.md` should explain the workflow or implementation model for users and developers.
- `readme-audience` — `README.md` should speak to both users and developers/maintainers.
- `readme-installation` — `README.md` should include an `Installation` section with an `npx octocode skill ...` command.
- `link-no-condition` — every reference link states WHEN to load it (`when`/`if`/`before` ...). A bare "see references/" is too weak — the agent will not know which file matters.
- `orphan-reference` — every bundled reference should be reachable from `SKILL.md` or another reference. Unrouted reference files are dead context and likely drift.
- `reference-focus` — every reference file should declare one clear issue/purpose with a short H1; multiple H1s usually mean it should be split or subtopics demoted.
- `reference-too-long` — each `references/*.md` ≤ 150 lines. Split larger files and cross-link them.
- `reference-name` — reference filenames are short, indicative, kebab-case (no generic `doc.md`/`notes.md`/`misc.md`).
- `duplicate-content` — the same sentence (≥ 12 words) appears in two or more skill files. Cross-file duplication inflates context and creates drift when one copy is updated. Consolidate into the canonical file and cross-link.
- `frontmatter-metadata` — frontmatter holds only keys the agent or installer needs (`name`, `description`, `allowed-tools`, `license`). Agents read just `name`+`description` at discovery, so authoring/repo keys (`version`, `author`, `tags`, `created`/`updated`, `category`, …) are dead weight in the activation context. Drop them or track them in the repo, not in `SKILL.md`.
- `metadata-section` — a body heading is authoring/repo metadata rather than a task instruction (`## Changelog`, `## Version History`, `## Author(s)`, `## Credits`, `## License`, `## Metadata`, `## Table of Contents`, `## TODO`, `## Maintainers`, …). The agent never acts on these; they only spend tokens. Keep changelogs, credits, and version notes in the repo README, not the skill.
- `rigid` — density of imperative modals exceeds 12% of content lines. Rigid prompts break on legitimate edge cases. Prefer defaults with escape hatches; reserve strict modal keywords for genuinely fragile, destructive, or order-dependent steps.
- `verbose` — filler phrases detected. They consume tokens without adding information; rewrite concisely.
- `weak-critical-language` — weak words (`should`, `could`, `may`, `consider`, `as needed`, etc.) appear inside a critical rule line. Critical rules need `MUST`/`REQUIRED`, or the action must be explicitly optional.
- `ambiguous-action` — vague phrases such as `do some`, `as needed`, or `handle ... appropriately` appear outside an example/anti-pattern. Name the action, command, or IF/THEN condition.
- `decision-clarity` — uppercase/bold `IF` appears without `THEN`. Agent branch rules should use explicit `IF ... THEN ...` structure.
- `referential-ambiguity` — an instruction starts with an unclear referent such as `it`, `this`, `that`, `above`, or `below`. Name the object, step, section, or file.
- `missing-output-format` — an output/report/deliverable section lacks a concrete table, template, placeholder shape, or fenced format.
- `gate-structure` — gate/checkpoint language is present but the gate lacks required parts: Pre-Conditions, Gate Check, FORBIDDEN, ALLOWED, or On Failure.
- `low-density-section` — a long section has little structure. Compress it into routing bullets, tables, gates, or scripts.
- `xml-overuse` — paired/attributed XML-like control tags are overused. Markdown is the default; XML is only for attention-control needs.
- `tautology` — two adjacent narrative sentences share > 75% significant-token overlap. One is likely restating the other. Remove the weaker restatement. (Blockquotes and list items are exempt: parallel enumerated/quoted items — e.g. an Advocate line mirrored by a Critic line — are intentionally similar, not redundant.)
- `contradiction` — the same verb appears after both strict positive and strict negative phrasing in the same file. Conflicting instructions cause unpredictable agent behavior. Resolve to a single clear rule.
- `hooks-handling` — hooks are configured or bundled but `SKILL.md` does not explain when they run, how to inspect them, or how to verify/handle them.
- `hook-script-routing` — frontmatter hook commands should route to bundled `scripts/` or `hooks/` helpers instead of embedding complex inline shell.
- `hook-timeout` — frontmatter hook commands need a nearby `timeout`; lifecycle hooks must be bounded.

## Prompt rules the lint backs

- Lean over complete: every token in `SKILL.md` competes with conversation context. Cut anything the agent already does well without the skill.
- `SKILL.md` is the agent's context map: give the minimal "how it works" summary, then point each feature/capability to the relevant `references/*.md`, `scripts/*`, or asset. Put the reusable detail there.
- `README.md` is the human map: explain the high-level purpose, user-facing features, developer-facing workflow/internals, and `npx octocode skill` installation path.
- Not rigid, not verbose: prefer defaults with escape hatches over exhaustive menus; reserve strict modal language for fragile, destructive, or order-dependent steps.
- Prompt optimization: preserve working intent, strengthen only critical rules, add gates/output formats where behavior depends on them, and validate that changes did not bloat or conflict.
- No duplication: each fact lives in one place. Cross-link instead of repeating.
- No redundant data for agents: ship only what the agent reads to do the task. Authoring/repo metadata (extra frontmatter keys, changelogs, author/license/version sections) belongs in the repo, not in `SKILL.md` or `references/` where it burns activation tokens.
- Smart routing: `references/` files may link other `references/` files so an agent loads only the next file it needs — the lint counts these cross-links. Keep each reference single-purpose with a short indicative name.
- Complete main map: every bundled non-audit reference and every agent-facing script must be discoverable from `SKILL.md` with a short same-line description.
- Reference files are issue-focused: one topic per file, short H1, and cross-links only to the next related reference an agent may need.
- Runnable logic lives in `scripts/`, invoked from `SKILL.md` by relative path (`scripts/x.mjs`), never pasted inline.
- Deterministic over agentic: when a step is mechanical, repeatable, or token-heavy to spell out in prose, ship a `scripts/` helper and have `SKILL.md` *call* it. A script runs the same way every time and costs near-zero activation tokens; narrated steps get re-interpreted (and drift) on every run. Hand procedure to scripts; reserve natural-language instructions for genuine judgment. When reviewing or authoring a skill, flag any multi-step deterministic prose block that should be a script.

## Hooks Handling

Hooks are behavior, not metadata decoration. If a skill defines frontmatter `hooks:` or bundles hook helpers, `SKILL.md` must briefly say what lifecycle they affect, when an agent should inspect them, and what verification proves they behaved. Hook commands should be thin launchers to bundled helpers (`scripts/hooks/x.sh`, `scripts/x.mjs`, or `hooks/x.sh`) with a timeout. Do not hide install-impacting behavior in long inline shell.

When copying, installing, or auditing third-party skills, inspect hooks before writing them into a user/project scope. Report what they run, whether they touch files, and whether they are optional, required, or unsafe.

## Fixing a failing skill

1. Run the lint; group findings ERROR-first.
2. For `missing-readme`: add `README.md` with overview, features, how-it-works, developer notes, and `npx octocode skill` installation.
3. For `missing-reference`/`missing-script`: fix the filename, create the missing helper, or keep illustrative paths inside fenced examples/prose that does not look like a real bundled path.
4. For README warnings: add concise human docs for high-level purpose, all major features, workflow/internals, user/developer audience, and installation via `npx octocode skill --name <skill>` or `--add <github-path>`.
5. For `skill-too-long`/`no-references`/`skill-map-summary`: keep only the operational map in `SKILL.md`; extract conditional sections into short `references/*.md` with explicit load conditions.
6. For `capability-routing`/`script-routing`/`reference-map-complete`/`script-map-complete`: add a compact routing list in `SKILL.md` that maps each bundled reference/script to a concise purpose.
7. For `orphan-reference`: route the file from `SKILL.md` or another reference, or remove/split it if it is stale.
8. For `route-description`: add a same-line 3-28 word purpose or load condition; move extra explanation into the target file.
9. For `installation-section`: add `## Installation` to `SKILL.md` for skills with install scripts/references, covering approval gates and verification.
10. For `reference-too-long`/`reference-focus`: split by sub-topic, keep one H1, and cross-link related references.
11. For `duplicate-content`: move the sentence to the canonical file; replace the other occurrence with a cross-link.
12. For `script-quality`: add `--help`/usage, explicit input flags/env/stdin, bounded output, and noninteractive failure messages.
13. For `hooks-handling`/`hook-script-routing`/`hook-timeout`: document hook scope in `SKILL.md`, route commands to bundled helpers, and add bounded timeouts.
14. For `deterministic-prose`: turn repeatable command sequences into scripts; keep prose for judgment.
15. For prompt optimization findings (`weak-critical-language`, `ambiguous-action`, `decision-clarity`, `referential-ambiguity`, `missing-output-format`, `gate-structure`, `low-density-section`, `xml-overuse`): preserve intent, strengthen critical wording, add explicit IF/THEN branches, output templates, and complete gates only where needed.
16. For `rigid`/`verbose`/`tautology`/`contradiction`: edit the offending lines directly; the lint message quotes the exact text.
17. For `frontmatter-metadata`/`metadata-section`: delete the agent-irrelevant frontmatter keys and metadata headings (changelog/author/license/version); if the data is worth keeping, move it to the repo README.
18. For `description-concise`: rewrite the frontmatter `description` so chars 1–50 are `Use when <triggers>`; keep the rest trigger-rich and ≤1024 chars total (don't strip useful triggers to chase brevity).
19. Re-run until ERRORs clear; treat residual WARNs as a gated decision with the user.
