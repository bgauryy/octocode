---
name: octocode-skills
description: Use when finding, evaluating, previewing, installing, rating, reviewing, improving, refactoring, linting, or synthesizing Agent Skills (`SKILL.md` folders) across GitHub, local skill folders, and skill marketplaces. Covers deep-diving a candidate, installing skills into agents, rating or linting an existing skill, updating one in place, or creating a local skill from researched patterns. Skip for general package search (npm, cargo), non-skill web search, or code research not involving SKILL.md files.
---

# Octocode Skills

Search for, evaluate, create, and update Agent Skills — powered by the Octocode CLI (`npx octocode`) for all code research. Find and judge skills by inspecting real skill files, compare workflow quality, author new skills from researched patterns, update existing ones in place, and gate every write or install action. A skill is a folder with `SKILL.md` (`name`, `description`, instructions) plus optional `scripts/`, `references/`, `assets/`. Agents load them by progressive disclosure — so keep recommendations and any skill you author lean.

Octocode transport reference: read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

## Operating Model

Flow: `UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY`. Compress steps when the user names a source (`owner/repo path` or local path); repeat when candidates are weak or conflicting.

Hard rules:

- Inspect actual `SKILL.md` content (and behavior-affecting referenced files for strong/risky/unclear candidates) before recommending, adapting, installing, or quoting it. Skip candidates lacking valid `name`+`description`.
- Identify every candidate by `(owner/repo, path-to-SKILL.md)` or absolute/workspace path. Recommend by task fit, workflow quality, safety gates, and portability; use installs/stars only as a tiebreaker.
- Gate installs, file writes, local skill creation, target selection, config changes, overwrites, and symlinks behind explicit user approval.
- Do not hand the user a raw search dump to rank; filter, explain tradeoffs, and recommend a next step. Copy a skill wholesale only when license and user allow it.

Stop when: one recommendation is justified by inspected content; or two High-quality candidates are inspected and the top pick fits; or three search angles return nothing new; or a user gate is pending.

## Understand

Extract before acting: user goal (find / compare / preview / install / deep-dive / rate / lint / improve / create); task domain; target ecosystem (Claude Code, Cursor, Codex, OpenCode, custom, unspecified); source scope (local, named repo, marketplace, broad public, user-provided path); constraints (language, license, local-only, no-web, install target, security posture); quality preference. Ask one focused question only when the answer changes search scope, target ecosystem, or write/install behavior; otherwise proceed on stated assumptions.

## Tool Routing

Use **Octocode for all research**, local and external — **via the Octocode MCP server if it's installed, otherwise the CLI (`npx octocode`)**. Same capabilities either way; the table below gives the CLI form (read live flags with `npx octocode <command> --help`, or `npx octocode tools <name> --scheme` for raw tools — don't duplicate schemas here). With the MCP server, call the matching tool (`localSearchCode`/`ghSearchCode`, `ghSearchRepos`, `localFindFiles`, `localViewStructure`/`ghViewRepoStructure`, `localGetFileContent`/`ghGetFileContent`, `ghCloneRepo`) instead. Lead local for the user's workspace (existing/draft skills, custom paths, repo conventions); lead GitHub when shopping, comparing, or asking about something not present locally.

Verb → CLI command (the quick commands auto-route: a **local path** hits the workspace, `owner/repo` hits GitHub) · MCP tool:

| Need | CLI command | MCP tool |
|------|-------------|----------|
| Search code / find `SKILL.md` | `npx octocode search "<kw>" owner/repo` · local: `… <path>` | `ghSearchCode` / `localSearchCode` through OQL |
| Discover repos by topic | `npx octocode search <keywords> --target repositories` | `ghSearchRepos` |
| Find files by name/path | `npx octocode search "<query>" [owner/repo\|path] --search path` | `localFindFiles` through OQL `target:"files"` |
| List a folder / repo tree | `npx octocode search owner/repo/path --tree` · local: `… <path> --tree` | `ghViewRepoStructure` / `localViewStructure` |
| Read an exact file | `npx octocode search owner/repo/path/SKILL.md --content-view exact` · local: `… <path> --content-view exact` | `ghGetFileContent` / `localGetFileContent` |
| Download a skill folder | `npx octocode clone owner/repo/path[@branch]` | `ghCloneRepo` |
| Schema-exact / bulk | `npx octocode tools <name> --scheme` then `--queries '<json>'` | call the tool directly |

For every PUBLIC skill query, also run the runtime web search tool in parallel (catches skills in articles, awesome-lists, registries) — treat web mentions as LEADS and confirm the real `SKILL.md` via Octocode (`npx octocode search <path> --content-view exact` or `ghGetFileContent`) before recommending.

Fallbacks: if neither the MCP server nor `npx octocode` is available (no network / not installed), map each verb (search/read/list/download) to the equivalent runtime tool; if a marketplace surface is unreachable, switch to GitHub topic search and `llms.txt` snapshots and lower confidence; if the user requested local-only, do not query remote sources.

## References

Load on demand; each states when to read it. References cross-link each other, so load only the next one you need.

- `references/search-playbook.md` — when discovering candidates: depth settings, the parallel three-surface fan-out, search angles, GitHub `SKILL.md` patterns, and the skills.sh registry API.
- `references/discovery-surfaces.md` — when shopping beyond raw GitHub: marketplaces, leaderboards, registry REST APIs, manifest formats, CLI installers, self-bootstrap meta-skills.
- `references/quality-rubric.md` — when judging a candidate: dimensions to evaluate and High/Medium/Low labels.
- `references/quality-signals.md` — when ranking by evidence beyond stars: install counts, recency, audit badges, demand signals.
- `references/output-format.md` — when presenting results, result cards, the next-step gate, or running a deep-dive.
- `references/agent-skills-guide.md` — when evaluating, improving, or authoring a skill: structure, progressive disclosure, context discipline, instruction patterns, scripts.
- `references/description-tuning.md` — when optimizing a `description` trigger with eval queries.
- `references/self-improvement.md` — when the user asks to rate, review, score, improve, refactor, or lint a `SKILL.md`.
- `references/skill-lint.md` — when linting/auditing skill structure, and after creating or editing a skill; documents `scripts/skill-lint.mjs`.
- `references/install-reference.md` — when installing a skill or choosing targets, scopes, destinations, or conflict behavior.
- `references/fetch-and-create-locally.md` — when fetching a remote skill via Octocode into a local folder, verbatim or adapted.
- `references/create-local-skill.md` — when creating or synthesizing a new local skill from research.
- `references/recovery.md` — when a search, fetch, install, or marketplace surface fails or returns nothing.

## Acting Gates

Install (detail in `references/install-reference.md`): normalize input to a folder with a valid `SKILL.md`; ask provider(s), scope per provider, project root, and copy-vs-symlink before writing; inspect `scripts/`/hooks before copying third-party skills; per-destination conflict check (Overwrite/Skip/Rename/Diff/Cancel); confirm the full plan; verify each destination and report per-destination result.

Create a local skill (detail in `references/create-local-skill.md`): synthesize research, present a short plan, gate, then write lean files and a `references/references.md` audit trail (shape in `references/references-template.md`). Run `scripts/skill-lint.mjs` before reporting any created or edited skill as done.
