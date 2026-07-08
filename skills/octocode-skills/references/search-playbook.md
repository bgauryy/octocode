# Search Playbook

Load when discovering skill candidates. Pairs with `discovery-surfaces.md` (the full registry/marketplace catalog) and `quality-rubric.md` (how to judge what you find).

## Set depth before searching

- Quick answer: inspect enough to recommend one best candidate with caveats.
- Research request: compare broadly, preserve confirmed sources, stop when more search is unlikely to change the recommendation.
- Install request: inspect source, support files, target destinations, and conflict behavior before asking for approval.
- Improve / rate / lint / create: inspect the target skill, adjacent local examples, and `agent-skills-guide.md` before writing.
- Weak results: broaden once, then report the gap and the next best action.

## Parallel three-surface fan-out

For every PUBLIC skill query, fan out across three surfaces IN PARALLEL, then merge and dedupe by `(owner/repo, skill name)`:

1. Octocode/GitHub — delegate code and path search for `SKILL.md` to `octocode-research` when installed.
2. skills.sh registry API — install-ranked candidates (below).
3. Runtime web search tool (e.g. `WebSearch`) — topic + "agent skill"/"claude skill"/"SKILL.md", to catch skills outside known registries. Confirm each web lead's real `SKILL.md` through `octocode-research` before recommending.

Skip the public surfaces only for local-only or org/private scopes; use `octocode-research` for Octocode-backed checks there.

## Search angles

- Name: exact phrase, lowercase, hyphenated folder name, aliases.
- Subject: core domain terms.
- Workflow verbs: analyze, review, migrate, generate, install, optimize, debug, audit, benchmark, plan.
- Ecosystem: agent, IDE, language, framework, MCP server, CLI, or platform named by the user.
- Safety: gate, validation, rollback, verify, tests, prompt, scripts, permissions.

## GitHub `SKILL.md` patterns

- Search body/frontmatter with `filename: "SKILL.md"` and `match: "file"`.
- Search folder names with `filename: "SKILL.md"` and `match: "path"`.
- Search composite filenames `*.skill.md` for skills not using the canonical name.
- Bias toward well-formatted skills: `filename: "SKILL.md" "name:" "description:"`.
- Discover repos via topics: `topicsToSearch: ["agent-skills"]`, `["claude-code-skills"]`, `["claude-skill"]`, `["cursor-skills"]`, `["codex-skills"]`; combine with `agent`, `skills`, `SKILL.md`.
- Inspect likely paths: `skills/<name>/SKILL.md`, `skills/<category>/<name>/SKILL.md`, `<name>/SKILL.md`, `.agents/skills/<name>/SKILL.md`, `.claude/skills/<name>/SKILL.md`, `.cursor/skills/<name>/SKILL.md`, `.opencode/skills/<name>/SKILL.md`, `.github/skills/<name>/SKILL.md`, `.gemini/skills/<name>/SKILL.md`.
- Probe manifests: `.claude-plugin/marketplace.json`, `.claude-plugin/plugin.json`, per-catalog `llms.txt` / `llms-full.txt`.

## Skills.sh registry API

MUST run in parallel with GitHub/Octocode + web search for every public skill query. MUST NOT use for org-specific or private searches (Octocode tools only).

```bash
curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' \
  --compressed \
  -H 'User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:150.0) Gecko/20100101 Firefox/150.0'
```

Response: `{"skills": [{"id", "skillId", "name", "installs": number, "source": "owner/repo"}], "count"}`.

Workflow:

1. Sort by `installs` descending — highest install count is the strongest battle-tested signal.
2. Take the top 5 as priority inspection targets.
3. In parallel, ask `octocode-research` to fetch each top candidate's `SKILL.md` using `source` as `owner/repo`; try `skills/<skillId>/SKILL.md`, `<skillId>/SKILL.md`, `.claude/skills/<skillId>/SKILL.md`.
4. Include install count in every result card.
5. MUST NOT blindly recommend the highest-install skill — inspect content and task fit first; installs are a tiebreaker only.

Fallback: if the API is unreachable/rate-limited, switch to the `https://www.skills.sh` leaderboard and GitHub topic search; lower confidence and continue (see `recovery.md`).

## Seed collections (only when discovery is sparse)

Start from `topic:agent-skills` (or narrower `topic:claude-code-skills`), then sample well-maintained collections: `anthropics/skills`, `ComposioHQ/awesome-claude-skills`, `addyosmani/agent-skills`, `vercel-labs/skills`, `alirezarezvani/claude-skills`, `microsoft/skills`, `obra/superpowers`, `trailofbits/skills`, `wshobson/claude-code-workflows`, or any author-curated marketplace the user trusts.
