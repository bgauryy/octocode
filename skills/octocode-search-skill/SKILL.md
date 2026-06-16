---
name: octocode-search-skill
description: Use this skill when the user asks to find, evaluate, preview, install, rate, review, score, improve, refactor, or synthesize Agent Skills (the `SKILL.md` folder format) across GitHub, local skill folders, and skill marketplaces. Covers searching for a skill for a task, deep-diving a candidate, installing one or more skills into one or more agents at user or project scope, rating or reviewing an existing SKILL.md, refactoring a skill, or creating a new local skill from researched patterns. Do NOT activate for general package search (npm, cargo), general (non-skill) web search, or code research not involving SKILL.md files.
---

# Octocode Search Skill

Find, evaluate, improve, install, or synthesize Agent Skills by inspecting real skill files, comparing workflow quality, and gating every write or install action.

Agent Skills are folders with required `SKILL.md` frontmatter (`name`, `description`) plus instructions. They may include `scripts/`, `references/`, `assets/`, or other support files. Agents load them by progressive disclosure: metadata first, full `SKILL.md` on activation, bundled resources only when needed.

## Operating Model

Default flow:

```text
UNDERSTAND -> DISCOVER -> INSPECT -> JUDGE -> RECOMMEND -> USER GATE -> ACT -> VERIFY
```

Compress steps when the user names a specific source (`owner/repo path-to-SKILL.md` or a local path). Repeat steps when discovery returns weak or conflicting candidates.

Hard rules:

Recommend
- MUST recommend by task fit, workflow quality, safety gates, and portability; use `installs` count or GitHub stars only as a tiebreaker when two candidates are otherwise equal.
- MUST identify every remote candidate by `(owner/repo, path-to-SKILL.md)` and every local candidate by absolute or workspace-relative path.

Inspect
- MUST inspect actual `SKILL.md` content before recommending, adapting, installing, or quoting a candidate as a pattern.
- MUST inspect referenced files that affect behavior for strong, risky, or unclear candidates.
- MUST skip candidates lacking valid `name` and `description` frontmatter.

Gate
- MUST gate installs, file writes, local skill creation, target selection, config changes, overwrite decisions, and symlink decisions.

Forbidden
- FORBIDDEN: handing the user a raw search dump to rank. Filter first, explain tradeoffs, recommend a next step.
- FORBIDDEN: copying another skill wholesale unless the license and the user explicitly allow it.

Stop when any of these is true:

- One recommendation is justified by inspected content.
- Two or more High-quality candidates have been inspected and task fit is confirmed for the top pick.
- Three search angles have returned no new candidates not already examined.
- A user gate is awaiting an answer.

## Tool Routing

Use Octocode MCP for all research — locally and externally — and let user intent decide which side leads. Octocode MCP already documents its own tools and query schemas; rely on the active descriptors instead of duplicating them here.

- Lead local when the question is about the user's workspace: existing skills, custom paths, draft skills, repo conventions.
- Lead GitHub when the user is shopping for a skill, comparing options, or asking about something not present locally.
- Read code or files: `localGetFileContent` or `ghGetFileContent`.
- Download a remote skill folder before writing it locally: `ghGetFileContent(type="directory")` or `ghCloneRepo`.
- Web search: for every PUBLIC skill query, MUST also run the runtime's web search tool (e.g. `WebSearch`) in parallel with Octocode/GitHub and the skills.sh API. It catches skills surfaced in articles, awesome-lists, release notes, and registries outside the known set. Treat web-only mentions as LEADS, not recommendations — always resolve the real `(owner/repo, path-to-SKILL.md)` and confirm the actual `SKILL.md` via Octocode (`ghGetFileContent`) before recommending. Skip only for local-only or org/private scopes (Octocode tools only there).

Fallbacks:

- IF the runtime lacks Octocode MCP, map each verb (search, read, list, download) to the equivalent runtime tool and continue.
- IF GitHub research is required and unavailable, stop and ask whether to use a public web fallback.
- IF a marketplace surface (`skills.sh`, `claude-plugins.dev`, `aiskillstore.io`, `agentskills.me`) is unreachable or rate-limited, switch to GitHub topic search and `llms.txt` catalog snapshots (see `references/discovery-surfaces.md`); lower confidence and continue.
- IF the user requested local-only work, do not query remote sources.

## Local References

All reference material lives under `references/`.

- Read `references/agent-skills-guide.md` when evaluating, improving, rating, or creating a skill, optimizing a description, deciding what belongs in `SKILL.md`, designing progressive references, or adding scripts/assets.
- Read `references/discovery-surfaces.md` when the user wants to shop for skills beyond raw GitHub search — marketplaces, leaderboards, registry REST APIs, manifest formats, and CLI installers.
- Read `references/install-reference.md` when the user chooses to install a skill or asks about install targets, destinations, scopes, or conflict behavior.
- Read `references/fetch-and-create-locally.md` when fetching a remote skill via Octocode into a local folder — whether to install verbatim or to adapt into a new local skill.

## Understand

Extract these facts before searching or editing:

- User goal: find, compare, preview, install, deep-dive, rate, improve, or create.
- Task/domain: coding, docs, data, design, security, research, planning, review, operations, or other.
- Target ecosystem: Claude Code, Claude Desktop, Cursor, Codex, OpenCode, custom agent, or unspecified.
- Source scope: local folders, named repo, marketplace, broad public search, or user-provided skill path.
- Constraints: language, framework, IDE, license, local-only, security posture, install target, no-web, org/repo limits.
- Quality preference: battle-tested, small, script-backed, enterprise-safe, example-rich, low-dependency, or strict-gated.

Ask one focused question only when the answer changes search scope, target ecosystem, or write/install behavior. Otherwise proceed with stated assumptions.

## Discover And Inspect

Set depth before searching:

- Quick answer: inspect enough to recommend one best candidate with caveats.
- Research request: compare broadly, preserve confirmed sources, stop when more search is unlikely to change the recommendation.
- Install request: inspect source, support files, target destinations, and conflict behavior before asking for approval.
- Improve, rate, or create request: inspect the target skill, adjacent local examples, and `references/agent-skills-guide.md` before writing.
- Weak results: broaden once, then report the gap and the next best action.

For every public skill query, fan out across three surfaces IN PARALLEL, then merge and dedupe by `(owner/repo, skill name)`:

1. Octocode/GitHub — code and path search for `SKILL.md` (see "Useful GitHub patterns").
2. skills.sh Registry API — install-ranked candidates (see "Skills.sh Registry API").
3. Web search — runtime web search tool (e.g. `WebSearch`) for the topic + "agent skill"/"claude skill"/"SKILL.md", to catch skills outside the known registries. Confirm each web lead's real `SKILL.md` via Octocode before recommending.

Search angles:

- Name: exact phrase, lowercase, hyphenated folder name, aliases.
- Subject: core domain terms.
- Workflow verbs: analyze, review, migrate, generate, install, optimize, debug, audit, benchmark, plan.
- Ecosystem: agent, IDE, language, framework, MCP server, CLI, or platform named by the user.
- Safety: gate, validation, rollback, verify, tests, prompt, scripts, permissions.

Useful GitHub patterns:

- Search body and frontmatter with `filename: "SKILL.md"` and `match: "file"`.
- Search likely folder names with `filename: "SKILL.md"` and `match: "path"`.
- Search composite filenames `*.skill.md` for skills that do not use the canonical `SKILL.md` name.
- Search frontmatter content with `filename: "SKILL.md" "name:" "description:"` to bias toward well-formatted skills.
- Discover repos via topics: `topicsToSearch: ["agent-skills"]`, `["claude-code-skills"]`, `["claude-skill"]`, `["cursor-skills"]`, `["codex-skills"]`. Combine with keywords like `agent`, `skills`, and `SKILL.md`.
- Inspect likely paths: `skills/<name>/SKILL.md`, `skills/<category>/<name>/SKILL.md`, `<name>/SKILL.md`, `.claude/skills/<name>/SKILL.md`, `.cursor/skills/<name>/SKILL.md`, `.codex/skills/<name>/SKILL.md`, `.opencode/skills/<name>/SKILL.md`, `.agents/skills/<name>/SKILL.md`.
- Probe plugin manifests: `.claude-plugin/marketplace.json`, `.claude-plugin/plugin.json`, and per-catalog `llms.txt` / `llms-full.txt` files for batch discovery.

### Skills.sh Registry API

MUST run this in parallel with GitHub/Octocode search AND web search for every public skill query (the three-surface fan-out above). MUST NOT use for org-specific or private searches — use Octocode tools only for those.

```bash
curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' \
  --compressed \
  -H 'User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:150.0) Gecko/20100101 Firefox/150.0'
```

Response shape: `{"skills": [{"id": string, "skillId": string, "name": string, "installs": number, "source": "owner/repo"}, ...], "count": number}`

Popularity workflow — MUST follow this order:

1. Sort results by `installs` descending — highest install count = most battle-tested signal.
2. Take the top 5 candidates by installs as priority inspection targets.
3. In parallel with other searches, fetch each top candidate's `SKILL.md` via Octocode (`ghGetFileContent` using `source` as `owner/repo`; try paths `skills/<skillId>/SKILL.md`, `<skillId>/SKILL.md`, `.claude/skills/<skillId>/SKILL.md`).
4. Include install count in every result card as a quality signal.
5. MUST NOT blindly recommend the highest-install skill — inspect content and task fit first; use `installs` as a tiebreaker only when two candidates are otherwise equal.

Fallback: if the API is unreachable or rate-limited, switch to `https://www.skills.sh` leaderboard page and GitHub topic search; lower confidence and continue.

Marketplace and registry surfaces (see `references/discovery-surfaces.md` for the full list and APIs):

- Per-skill check: `https://www.skills.sh/<owner>/<repo>/<skill-name>` — install count, install command, security audit status.
- Leaderboard: `https://www.skills.sh` — install-count ranked, agent-filtered.
- Additional registries: `agentskills.io/llms.txt`, `aiskillstore.io/llms.txt`, `microsoft.github.io/skills/llms-full.txt`; `claude-plugins.dev` REST for Claude Code plugin search.

Seed only when discovery is sparse. Start from `topic:agent-skills` (or the narrower `topic:claude-code-skills`) on GitHub, then sample well-maintained collections such as `anthropics/skills`, `ComposioHQ/awesome-claude-skills`, `addyosmani/agent-skills`, `vercel-labs/skills`, `alirezarezvani/claude-skills`, `microsoft/skills`, `obra/superpowers`, `trailofbits/skills`, `wshobson/claude-code-workflows`, or any author-curated marketplace the user trusts.

## Judge Quality

For every plausible candidate, inspect enough `SKILL.md` content to understand behavior. For strong, risky, or ambiguous candidates, inspect full `SKILL.md` plus referenced scripts, templates, install docs, evals, or reference files that affect execution.

Evaluate:

- Trigger: clear activation conditions and non-activation boundaries.
- Workflow: ordered steps, decision points, recovery paths, and stop conditions.
- Evidence: real file contents, referenced resources, tests, examples, or scripts.
- Gates: validation, approval, preview, review, permissions, rollback, and install conflict handling.
- Output UX: concise results, useful comparison cards, explicit next-step gate.
- Specificity: domain knowledge an agent would not know by default.
- Portability: agent/runtime assumptions, hardcoded paths, external services, dependencies, secrets.
- Risk: unsafe commands, hidden network actions, missing referenced files, license ambiguity, stale docs, broad triggers.

Quality labels:

- `High`: direct match, clear trigger, executable workflow, useful resources and gates, and no obvious safety or portability red flags.
- `Medium`: partial match or adaptable, but missing some validation, UX, or domain detail.
- `Low`: keyword-only match, generic workflow, unclear trigger, stale pattern, or meaningful caveat.

For evidence-based quality signals beyond stars (install counts, recency, audit badges, capability overlap, demand signals), load `references/agent-skills-guide.md` §Quality Signals Beyond Stars and `references/discovery-surfaces.md` §Quality Signals Beyond Stars.

## Self-Improvement Mode

Use this mode when the user asks to rate, review, score, improve, or refactor a `SKILL.md` — yours or someone else's. Read `references/agent-skills-guide.md` before rating or rewriting.

Modes — pick one before starting. If the user's request is ambiguous (e.g., "check my skill"), present this gate before proceeding:

```text
Which mode?
1. Rate-only — score and report issues; no file edits.
2. Improve / refactor — fix issues and rewrite; gate before writing.
3. Fix all — apply fixes from a prior rating in this conversation; skip re-rating.
4. Cancel.
```

- `Rate-only` (rate, review, score, audit): stop after the REPORT step. MUST NOT edit files. End with a numbered next-action gate (apply fixes, show diff, cancel).
- `Improve` / `refactor` / `rewrite`: full flow including REWRITE and VALIDATE; gate before writing.
- `Fix all` / `apply fixes`: skip MAP INTENT and RATE ISSUES if a prior rating exists in the conversation; go straight to REWRITE → VALIDATE → REPORT.

Flow:

```text
READ -> MAP INTENT -> RATE ISSUES -> [REWRITE -> VALIDATE] -> REPORT
```

Read:

- Read the full target `SKILL.md` and all referenced files that affect behavior.
- Note purpose, line count, resources, gates, and output format.

Map intent:

- Preserve the skill's core job, trigger domain, and user-facing promises.
- Identify what behavior must become more reliable: activation, research quality, safety gates, tool routing, output shape, or recovery.

Rate issues:

- Check for weak rules in critical sections, vague actions, raw-search handoff, missing gates, unsafe writes, missing verification, stale references, and line-count bloat.
- Group findings by severity: `Critical`, `High`, `Medium`, `Low`. Cite `file:line` for each.
- Score per dimension using the §Judge Quality rubric (`High` / `Medium` / `Low`).

Rewrite (skip in Rate-only mode):

- Fix Critical and High issues first.
- Keep `SKILL.md` concise; target 300 lines or less unless the domain justifies more.
- Move long examples, schemas, or static references into `references/` only when that reduces active-context load.
- Keep `description` trigger-rich without keyword stuffing.

Validate (skip in Rate-only mode):

- Frontmatter has valid `name` and `description`.
- Workflow has clear steps, gates, recovery, and output UX.
- Referenced files exist or missing files are documented as risks.
- Critical actions use MUST/NEVER/FORBIDDEN where needed.
- No write/install action bypasses an explicit user gate.

Report — required output shape for `Rate-only`:

```text
Overall:        <score>/10 — <letter grade> (one-sentence summary).
Score card:     per-dimension High/Medium/Low using §Judge Quality.
Issues:         grouped Critical / High / Medium / Low, each with file:line.
Validation:     pass/fail per checklist item above.
Strengths:      2-4 bullets worth preserving.
Residual risk:  1-3 bullets.
Next action:    numbered choices ending with "Cancel".
```

Report — for `Improve` / `Fix all`:

- Summarize intent preserved, major fixes applied, validation result, and any residual risk.

## Present Results

Lead with the recommendation in one sentence. Then group results only when useful:

- `Best matches`
- `Useful alternatives`
- `Explore if...`

If results are few, show compact cards. If results are many, list confirmed names and sources compactly and provide detailed cards only for the strongest candidates.

Card shape (label layout, not literal Markdown):

```text
Name:            <skill-name>  - fit: High | Medium | Low
Source:          <owner/repo path-to-SKILL.md> or <local path>
What it does:    <one sentence in your own words>
Actual flow:     <2-4 short steps from inspected content>
Quality signals: <specific evidence>
Why it matches:  <tie to user's request>
Caveat:          <real risk, or "None obvious from inspected files">
```

Keep prose short. Do not paste raw search dumps or large excerpts.

End with a user gate that offers the real next branches — not just "install or cancel". Use a structured ask tool when the runtime provides one; otherwise present concise numbered choices and wait.

Gate example:

```text
Recommended: <skill-name> from <source>

Choose:
1. Install — fetch into one or more agent destinations the user picks (see references/install-reference.md and references/fetch-and-create-locally.md).
2. Create a local skill — adapt patterns from this candidate into a new local SKILL.md.
3. Explain — break down trigger, workflow, gates, and risks.
4. Show link — return the source URL or local path only, no write.
5. Compare — line up against another candidate.
6. Keep researching.
7. Cancel.
```

## Deep-Dive

When the user picks a skill:

1. Fetch full `SKILL.md`.
2. Fetch directly referenced files that affect behavior.
3. Summarize trigger, workflow, support files, validation and safety gates, strengths, gaps, and adaptation ideas.
4. Ask whether to install, adapt into a local skill, compare, or keep researching.

## Create A Local Skill From Research

Use this when the user chooses to create a skill from findings or asks to synthesize one. Read `references/agent-skills-guide.md` before planning. If the source is a remote skill being fetched into a local folder, also read `references/fetch-and-create-locally.md`.

Before writing files:

1. Build a research synthesis:
   - User need and constraints.
   - Inspected source skills and useful patterns.
   - Quality and UX gates to include.
   - Resources to create, if any.
   - Exclusions: copied, generic, risky, or unnecessary pieces.
2. Present a short plan:
   - Skill name and destination.
   - Trigger description draft.
   - Workflow outline.
   - Resources and validation plan.
3. Ask for approval with create, adjust, inspect more, or cancel options.

After approval, write the skill with concise purpose, workflow, tool and resource rules, gates, output UX, and recovery paths. Add `references/`, `scripts/`, or `assets/` only when they reduce repeated work or keep `SKILL.md` lean. Defer to a dedicated skill-creation skill when one is available.

MUST also create `references/references.md` inside the new skill folder using the shape in `references/references-template.md`. Populate it with every source actually consulted — do not list sources that were not checked. This file is a research audit trail, not a bibliography template.

## Install

Read `references/install-reference.md` before installing. Keep install behavior gated and verified. The reference is provider-agnostic; do not hardcode a destination.

Minimum install gates:

1. Normalize input to a skill folder containing a valid `SKILL.md`.
2. MUST ask the user where to install before writing anything. Cover all four destination questions: provider(s), scope per provider (user vs project vs custom path), project root if project scope, and install mode (copy vs symlink). Never assume one answer applies to every provider.
3. Inspect `scripts/`, install hooks, or executable helpers before copying third-party skills.
4. Per-destination conflict check; ask `Overwrite`, `Skip`, `Rename`, `Diff`, or `Cancel` for each conflict.
5. Show source, description, every resolved destination path, install mode, and conflict plan; require explicit confirmation.
6. Prefer copy; use symlink only for stable local sources when the user wants live source-tracking.
7. Verify installed `SKILL.md` exists in each destination and report per-destination success or failure.

For remote sources, follow the fetch-and-write workflow in `references/fetch-and-create-locally.md`.

## Recovery

- No results: broaden terms once, inspect repo roots, or fall back to seed collections.
- Too many generic results: narrow by domain, agent, tool, workflow verb, or safety requirement.
- Strong repo but no skill path: browse root, `skills/`, `.claude/skills/`, `.cursor/skills/`, then category folders.
- Missing frontmatter: skip the candidate.
- Missing referenced files: lower confidence and mention the gap.
- Unsafe behavior: do not recommend install; explain the risk and offer a safer adaptation.
- Marketplace per-skill URL 404 (e.g. `https://www.skills.sh/<owner>/<repo>/<skill-name>`): the skill is not in that public index. Fall back to the source repo and lower confidence.
- Registry API rate-limit or 5xx: switch to `llms.txt` / `llms-full.txt` snapshot or to GitHub topic search; see `references/discovery-surfaces.md` §Recovery.
- Manifest file expected but missing (`.claude-plugin/marketplace.json`, `llms.txt`): note the gap as a quality signal and continue from raw `SKILL.md` evidence.
- Tool or API unavailable: state what evidence is missing, map the failed verb to an alternative runtime tool when one exists, and ask the user whether to switch source, drop to a fallback, or stop.
