# Skill Lint

Use when linting, updating, or creating a skill — new or existing — or right after editing any `SKILL.md`.

For deeper quality scoring (workflow, gates, evidence) pair with `agent-skills-guide.md`; this file is the mechanical structure check.

## Run it

```bash
node scripts/skill-lint.mjs                       # lint every skill under the nearest parent skills/ root
node scripts/skill-lint.mjs ../some-skill         # lint one or more skill folders
node scripts/skill-lint.mjs ../some-skill --json  # machine-readable findings
```

Exit `1` if any ERROR is found; WARN is advisory. Always run it before reporting a created/edited skill as done, and surface the findings.

No-arg scans are relative to the linter copy being run: the local `.agents/skills/octocode-skills` copy scans `.agents/skills`, while the packaged `skills/octocode-skills` copy scans `skills`.

## What it enforces

ERROR — fix, exit code 1:

| Rule | Meaning |
|---|---|
| `frontmatter` | `SKILL.md` has a `---` block with non-empty `name` and `description` |
| `missing-readme` | skill folder has a `README.md` (overview, features, how-it-works, install guide) |
| `missing-reference` | every `references/<file>.md` mentioned in `SKILL.md`/references exists (fenced examples and `references/references*.md` audit files are exempt) |
| `missing-script` | every `scripts/<file>` mentioned from `SKILL.md`/frontmatter exists under `scripts/` |
| `missing-scheme-script` | `SKILL.md` declares a `## Protocol`/`## Scheme`, a `- protocol:` key, or references `scripts/scheme.js` → that file must exist and expose every scheme/protocol |
| `link-outside-skill` | markdown links in `SKILL.md`/`references/*.md` never escape the folder (`../`, `/`, `~/`, `file://`) — use a GitHub URL instead |

WARN — lean/hygiene; fix unless the domain justifies an exception, and say why:

**Trigger & frontmatter**

| Rule | Meaning |
|---|---|
| `description-style` | `description` is "Use when …" — imperative + when-clause, ≤ 1024 chars |
| `description-concise` | first ~50 chars open with `Use when …` and state the trigger, no meta filler ("this skill", "the following"); total length isn't capped — pack in concrete triggers |
| `frontmatter-metadata` | frontmatter holds only `name`/`description`/`allowed-tools`/`license`/`hooks` — drop authoring keys (version, author, tags, dates) |
| `metadata-section` | a body heading is authoring/repo metadata (Changelog, Credits, License, TOC, Maintainers…), not a task instruction — keep it in the repo README |

**Structure & length**

| Rule | Meaning |
|---|---|
| `skill-too-long` | `SKILL.md` ≤ 50 lines; split conditional detail into `references/` |
| `skill-map-summary` | `SKILL.md` summarizes flow/modes/routing near the top |
| `no-references` | `SKILL.md` links at least one `references/*.md` |
| `reference-too-long` | each `references/*.md` ≤ 150 lines; split and cross-link |
| `reference-focus` | each reference has exactly one short (≤ 8-word) H1; multiple H1s mean it should split |
| `reference-name` | reference filenames are short kebab-case, not generic (`doc.md`, `notes.md`, `misc.md`…) |

**Routing & mapping**

| Rule | Meaning |
|---|---|
| `capability-routing` | a skill with several references directly routes most of them from `SKILL.md` |
| `reference-map-complete` | every non-audit `references/*.md` is listed in `SKILL.md` |
| `script-routing` | when `scripts/` exists, `SKILL.md` mentions `scripts/` |
| `script-map-complete` | every agent-facing, non-hook script is listed in `SKILL.md` |
| `route-description` | each listed reference/script has a same-line 3–28 word purpose or load condition |
| `link-no-condition` | every reference link states WHEN to load it (`when`/`if`/`before`…) — a bare "see references/" is too weak |
| `orphan-reference` | every bundled reference is reachable from `SKILL.md` or another reference |
| `installation-section` | skills with install references/scripts have a `## Installation` section routing gates + verification |

**README**

| Rule | Meaning |
|---|---|
| `readme-overview` | starts with an H1 plus a high-level purpose/when-to-use |
| `readme-features` | describes the visible features/capabilities |
| `readme-how-it-works` | explains the workflow/implementation model |
| `readme-audience` | speaks to both users and developers/maintainers |
| `readme-installation` | has an `## Installation` section with an `npx octocode skill …` command |

**Scripts & hooks**

| Rule | Meaning |
|---|---|
| `script-quality` | agent-facing scripts accept flags/env/stdin, show `--help`, and run unattended (no interactive prompts) |
| `deterministic-prose` | a long numbered, command-like procedure appears in prose with no `scripts/` helper — move mechanical steps to a script |
| `hooks-handling` | bundled/configured hooks are explained in `SKILL.md` (when they run, how to inspect/verify) |
| `hook-script-routing` | a frontmatter hook `command:` routes to a bundled `scripts/`/`hooks/` helper, not inline shell |
| `hook-timeout` | every frontmatter hook `command:` has a nearby `timeout:` |

**Prose quality**

| Rule | Meaning |
|---|---|
| `rigid` | imperative modals (MUST/NEVER/ALWAYS/FORBIDDEN) exceed 12% of lines — reserve them for fragile/destructive steps |
| `verbose` | hedge/filler phrasing that adds no information |
| `clarity` | a nominalization, double negative, or > 35-word line — state it more directly |
| `weak-critical-language` | a rigid-modal rule line also carries a hedge word, undercutting the rule |
| `ambiguous-action` | vague phrasing ("do some", "handle … appropriately") instead of naming the action, command, or IF/THEN |
| `decision-clarity` | uppercase `IF` with no matching `THEN` |
| `referential-ambiguity` | a line opens with an unclear referent ("it", "this", "above"…) instead of naming the object |
| `tautology` | two adjacent narrative sentences share > 75% significant-token overlap (list/blockquote items are exempt) |
| `contradiction` | the same verb follows both MUST/ALWAYS and NEVER/MUST NOT in one file |
| `duplicate-content` | the same ≥ 12-word sentence appears in two files — keep one canonical copy and cross-link |
| `xml-overuse` | more than 8 paired/attributed XML-like tags — Markdown is the default |
| `low-density-section` | a > 20-line section is < 20% structured (lists/tables/code/critical lines) — compress it |

**Output & gates**

| Rule | Meaning |
|---|---|
| `missing-output-format` | an output/report/deliverable section has no concrete table/template/fenced format |
| `gate-structure` | gate language is present but missing Pre-Conditions/Gate Check/FORBIDDEN/ALLOWED/On Failure |

Design rationale behind these rules — lean `SKILL.md`, references as the detail layer, scripts for deterministic work — lives in `agent-skills-guide.md`; this file only states the checks.

## Hooks Handling

Hooks are behavior, not metadata.

When copying, installing, or auditing third-party skills, inspect hooks before writing them into a user/project scope. Report what they run, whether they touch files, and whether they are optional, required, or unsafe.

The lint's `hooks-*` checks cover Claude-style `hooks:` frontmatter in `SKILL.md`. Cursor and Codex native hook configs (`.cursor/hooks.json`, `.codex/hooks.json`, and plugin `hooks/hooks.json`) are valid hook surfaces too, but they must be reviewed directly because they are outside `SKILL.md` frontmatter.

## Fixing a failing skill

1. Run the lint; fix ERRORs first — each message names the exact gap (missing file, bad frontmatter, escaped link, missing scheme script).
2. For WARNs, match the finding to a pattern and fix directly — the lint message quotes the exact offending text:

| Finding group | Fix |
|---|---|
| Length/duplication (`skill-too-long`, `reference-too-long`, `duplicate-content`) | move detail into (or between) `references/*.md`; cross-link instead of repeating |
| Routing (`*-routing`, `*-map-complete`, `route-description`, `link-no-condition`, `orphan-reference`) | add/complete a same-line purpose or load condition next to the reference/script link |
| README (`readme-*`) | add the missing section: overview, features, how-it-works, both audiences, `npx octocode skill --name <skill>` install |
| Scripts/hooks (`script-quality`, `deterministic-prose`, `hooks-*`) | add `--help`/flags, turn the procedure into a script, or route the hook to a bounded `scripts/`/`hooks/` helper with a `timeout:` |
| Frontmatter/metadata (`frontmatter-metadata`, `metadata-section`) | delete the agent-irrelevant key/heading; move it to the repo README if worth keeping |
| Prose quality (`rigid`, `verbose`, `clarity`, `weak-critical-language`, `ambiguous-action`, `decision-clarity`, `referential-ambiguity`, `tautology`, `contradiction`, `xml-overuse`, `low-density-section`) | edit the flagged line directly: direct verb, positive statement, named object, IF/THEN, or one canonical rule |
| Output/gates (`missing-output-format`, `gate-structure`) | add a concrete table/template, or complete the missing gate section(s) |
| `description-concise` | rewrite so chars 1–50 are `Use when <trigger>`; keep the rest trigger-rich, ≤ 1024 chars total |

3. Re-run until ERRORs clear; treat residual WARNs as a gated decision with the user.
