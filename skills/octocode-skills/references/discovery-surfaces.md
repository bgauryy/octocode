# Discovery Surfaces

Use when the user wants to shop for skills beyond raw GitHub code search — marketplaces, registries, leaderboards, manifest formats, and CLI installers. Cross-check at least two surfaces before recommending a candidate, and prefer surfaces that show evidence of activity (installs, recent updates, audit status).

## Quick Routing

| User goal | Best first surface |
|-----------|--------------------|
| "Search skills by keyword or topic" | skills.sh Registry API (`/api/search?q=...`) — sort by `installs` — run in parallel with GitHub/Octocode + web search |
| "Find skills mentioned in articles / awesome-lists / outside known registries" | Runtime web search tool (e.g. `WebSearch`): topic + "agent skill"/"claude skill"/"SKILL.md"; confirm each lead's real `SKILL.md` via Octocode |
| "Is skill X published / battle-tested?" | `https://www.skills.sh/<owner>/<repo>/<skill-name>` |
| "What are the most-installed skills right now?" | `https://www.skills.sh` leaderboard |
| "Find a Claude Code plugin" | `claude-plugins.dev` (REST + UI) |
| "Browse curated skills across agents" | `agentskills.me`, `agentskills.io` |
| "Capability-based search (e.g. web_search, summarization)" | `aiskillstore.io/v1/agent/search?capability=` |
| "Subscribe to new skills" | `aiskillstore.io/feed/new-skills.json` (JSON Feed) or `.rss` |
| "Microsoft / Azure ecosystem skills" | `microsoft.github.io/skills` + its `llms-full.txt` |
| "Claude marketplaces aggregated" | `claudemarketplaces.com`, `mcpmarket.com/tools/skills` |

## Per-Skill Check

Before recommending a remote skill, confirm it on at least one public index:

```text
https://www.skills.sh/<owner>/<repo>/<skill-name>
```

When the repo is the canonical `skills` repo, the URL collapses to:

```text
https://www.skills.sh/<org>/skills/<skill-name>
```

Examples: `anthropics/skills/skill-creator`, `vercel-labs/skills/find-skills`, `obra/superpowers/brainstorming` (prefix `https://www.skills.sh/`).

What that page tells you (use as quality signals, not stars):

- Install count (battle-test signal stronger than GitHub stars).
- One-line install command (`npx skills add <repo> --skill <name>`).
- Security audit status (e.g. "Gen Agent Trust Hub: Pass").
- Related skills + sibling skills from the same repo.
- First-seen date and source repository link.

If the URL 404s, the skill is not in the public skills.sh index — fall back to the source repo, search alternative registries, and lower confidence.

## Marketplace Surfaces

Each surface is provider-agnostic unless noted. Treat install counts and audit badges as self-reported by the registry.

### `agentskills.io`

- Open standard hub originated by Anthropic.
- `/clients` enumerates ~40 compatible agents (Claude Code, Cursor, Codex, Gemini CLI, OpenCode, Goose, Copilot, Junie, Amp, Roo, Kiro, etc.) with `instructionsUrl` and source repo per agent.
- Agent-readable index: `agentskills.io/llms.txt`.

### `skills.sh` (Vercel Labs)

- Public install-count leaderboard.
- Search box, agent filter ("available for these agents"), per-skill page (see Per-Skill Check above).
- Install pattern: `npx skills add <github-url> --skill <skill-name>`.
- **Registry search API** (use for broad public skill searches, in parallel with GitHub):
  ```bash
  curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' \
    --compressed \
    -H 'User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:150.0) Gecko/20100101 Firefox/150.0'
  ```
  Response: `{"skills": [{"id": "owner/repo/skillId", "skillId": string, "name": string, "installs": number, "source": "owner/repo"}], "count": number}`
  Sort results by `installs` descending; top entries are the most battle-tested candidates.
  Skip for org-specific or private searches — use Octocode tools only for those.

### `claude-plugins.dev` (Kamalnrf)

- Auto-indexes every public Claude Code plugin and `SKILL.md` on GitHub.
- Web UI + REST API + interactive `npx skills-installer search`.
- Sortable: Relevance / Most Downloads / Most Stars; filterable by "With Skills".

### `agentskills.me`

- Curated 492+ skills directory across Claude Code, Cursor, OpenCode, Codex CLI, Gemini CLI.
- Sortable: Most Popular / Most Stars / Editor's Pick.
- Per-tool subpages.

### `aiskillstore.io` (USK v1.0)

- Universal Skill Kit registry with full agent-facing REST API.
- Capability tag search: `/v1/agent/search?capability=<tag>` (e.g. `web_search`, `text_summarization`).
- Platform filter: OpenClaw / ClaudeCode / Cursor / GeminiCLI / CodexCLI.
- Trust levels: verified / community / sandbox.
- BM25 FTS5 full-text search.
- Similar skills: `/v1/agent/skills/{id}/similar`.
- Pre-flight: `/v1/skills/{id}/validate` for platform/Python compatibility.
- Demand signal: `/v1/demand/most-wanted` (zero-result queries — what does not exist yet).
- Subscriptions: `/feed/new-skills.json` (JSON Feed 1.1), `/feed/new-skills.rss`.
- Spec: `aiskillstore.io/llms.txt`.

### `microsoft.github.io/skills` + `microsoft/skills`

- Microsoft's curated skills index for Copilot CLI / VS Code; heavy on Azure SDK skills (Python, .NET, TS, Java, foundry, data, messaging, monitoring, entra, integration, compute, m365).
- "Sensei" frontmatter scoring rubric (Low / Medium / High based on triggers + anti-triggers + compatibility).
- Auto-generated `llms.txt` and `llms-full.txt` (daily-regenerated GitHub Pages).

### Aggregator directories (`moderate` confidence — verify each entry)

- `claudemarketplaces.com` — auto-updated directory of Claude marketplaces.
- `mcpmarket.com/tools/skills` — per-skill install snippets.
- `cursor.directory/plugins` — Cursor-native plugin/prompt directory.
- `LobeHub Skills Marketplace` — multi-language UI. Note: install pattern asks the agent to `curl` the skill and self-install; treat as discovery only, not a trusted install path (prompt-injection adjacent).

## Manifest Formats

| Format | Where | Use |
|--------|-------|-----|
| `agentskills.io/llms.txt` | Standard hub | Agent-readable doc index |
| `aiskillstore.io/llms.txt` | USK registry | Full v1.0 spec + endpoint catalog |
| `microsoft.github.io/skills/llms.txt` + `llms-full.txt` | Microsoft catalog | Daily catalog snapshot |
| `.claude-plugin/marketplace.json` | Per-marketplace repo | Anthropic native marketplace manifest |
| `.claude-plugin/plugin.json` | Per-plugin repo | Per-plugin manifest |
| `marketplace.json` (community) | Per-publisher repo | Community publish workflows |
| `feed/new-skills.json` | aiskillstore.io | JSON Feed 1.1 — new-skill firehose |
| `feed/new-skills.rss` | aiskillstore.io | RSS 2.0 alternative |
| YAML frontmatter (USK v3) | Inside `SKILL.md` | `name`, `description`, `license`, `compatibility`, `metadata.version`, `allowed-tools`, `interface`, `input_schema`, `output_schema`, `capabilities`, `permissions`, `platform_compatibility`, `tags` |

## CLI Installers

These tools combine discovery and install. Always run a safety scan on the source before allowing install.

| CLI | Install command | Notes |
|-----|-----------------|-------|
| `npx skills add` (Vercel Labs / `vercel-labs/skills`) | `npx skills add <gh-url> --agent <claude\|cursor\|codex\|opencode\|...> --skill <name>` | Symlink-by-default cross-agent installer. Reads `.claude-plugin/marketplace.json`. |
| `npx skills-installer` (Kamalnrf) | `npx skills-installer install @owner/repo/skill --client <claude-code\|cursor\|vscode>` | Pairs with `claude-plugins.dev`; interactive `search` TUI. |
| `npx claude-plugins` | `npx claude-plugins install/list/enable/disable` | Plugin marketplace mgmt. |
| `arctl` (Solo.io agentregistry) | `arctl skill init/publish/pull` | Registry stores skills as Docker Hub images. |
| `twg skills install` (Atlassian) | `twg skills install --global --agent claude` | Bundles to `~/.agents/skills` + per-agent dirs. |
| Anthropic native | `/plugin marketplace add owner/repo` then `/plugin install <name>@<marketplace>` | Canonical Claude Code surface. |
| `bunx skills add` (xixu-me) | `bunx skills add` | Alternate package-manager-style. |

For ranking by evidence beyond stars (install counts, recency, audit badges, demand signals), load `quality-signals.md`.

## Meta-Skills For Self-Bootstrapping

These let an agent search and install skills mid-session without a human:

- `find-skills` — discover and install from skills.sh inside an agent session.
- `agent-skill-discovery` / `skills-discovery` — local discovery helpers.
- `skill-creator` — Anthropic's create-and-iterate skill (anthropics/skills/skill-creator).

For surface-failure handling (404s, rate-limits, prompt-driven installers, missing manifests), load `recovery.md`.
