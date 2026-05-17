---
name: octocode-search-skill
description: Use this skill when the user asks to find, evaluate, preview, install, rate, review, score, improve, refactor, or synthesize Agent Skills (the `SKILL.md` folder format) across GitHub, local skill folders, and skill marketplaces. Covers searching for a skill for a task, deep-diving a candidate, installing one or more skills into one or more agents at user or project scope, rating or reviewing an existing SKILL.md, refactoring a skill, or creating a new local skill from researched patterns.
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
- MUST recommend by task fit, workflow quality, safety gates, and portability; use stars or popularity only as a tiebreaker.
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
- Further search is unlikely to change the recommendation.
- A user gate is awaiting an answer.

## Tool Routing

Use Octocode MCP for all research — locally and externally — and let user intent decide which side leads. Octocode MCP already documents its own tools and query schemas; rely on the active descriptors instead of duplicating them here.

- Lead local when the question is about the user's workspace: existing skills, custom paths, draft skills, repo conventions.
- Lead GitHub when the user is shopping for a skill, comparing options, or asking about something not present locally.
- Read code or files: `localGetFileContent` or `githubGetFileContent`.
- Download a remote skill folder before writing it locally: `githubGetFileContent(type="directory")` or `githubCloneRepo`.

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

Marketplace and registry surfaces (see `references/discovery-surfaces.md` for the full list and APIs):

- Per-skill check: `https://www.skills.sh/<owner>/<repo>/<skill-name>` — confirm a skill exists in the public index, see its install count, install command, and security audit status. Common shape when the repo is named `skills`: `https://www.skills.sh/<org>/skills/<skill-name>`.
- Leaderboard: `https://www.skills.sh` — install-count ranked, agent-filtered.
- Registry APIs: `agentskills.io/llms.txt`, `aiskillstore.io/llms.txt`, `microsoft.github.io/skills/llms-full.txt` for catalog snapshots; `claude-plugins.dev` REST for sortable Claude Code plugin search.

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

Modes — pick one before starting and confirm with the user if ambiguous:

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
