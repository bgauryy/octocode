# Discovery

Load when you discover skill candidates, shop beyond raw GitHub, parse a marketplace manifest, or pick an installer CLI. Why: the right registry and angles answer faster.

## Surfaces

Batch reads. For web search, query the topic plus "agent skill" or "SKILL.md".

Angles: name (exact, hyphenated, aliases) · subject · workflow verbs · ecosystem (agent, IDE, language, MCP) · safety (gate, verify, scripts).

```bash
curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' --compressed \
  -H 'User-Agent: Mozilla/5.0'
```

Sparse results: seed from `topic:agent-skills` and `anthropics/skills`, `vercel-labs/skills`, `obra/superpowers`, `microsoft/skills`, `trailofbits/skills`.

| Goal | Surface |
|---|---|
| Keyword or topic search | skills.sh `/api/search?q=` ∥ GitHub ∥ web search |
| Published or battle-tested? | `https://www.skills.sh/<owner>/<repo>/<skill>` |
| Claude Code plugin | `claude-plugins.dev` (auto-index, REST) |
| Curated multi-agent browse | `agentskills.io` (`/clients`, `llms.txt`), `agentskills.me` |
| Capability tag search | `aiskillstore.io/v1/agent/search?capability=`; new skills: `aiskillstore.io/feed/new-skills.json` |
| Microsoft / Azure | `microsoft.github.io/skills` + `llms-full.txt` |
| Claude marketplace dirs | `claudemarketplaces.com`, `mcpmarket.com/tools/skills` |

Aggregators carry moderate confidence: verify each entry.

## Manifests

| Format | Where |
|---|---|
| `llms.txt` / `llms-full.txt` | agentskills.io, aiskillstore.io, microsoft.github.io/skills |
| `.claude-plugin/marketplace.json`, `.claude-plugin/plugin.json` | marketplace and plugin repositories |
| YAML frontmatter | inside `SKILL.md`: `name`, `description`, optional fields |

## Installer CLIs

| CLI | Pattern |
|---|---|
| `npx -y octocode skill` | `install --add <src> --platform <hosts>`; preferred here |
| `npx skills add` | `<gh-url> --agent <host> --skill <name>`; symlink by default |
| `npx skills-installer` | `install @owner/repo/skill --client <host>` |
| Claude Code native | `/plugin marketplace add`, then `/plugin install` |

Meta-skills (`find-skills`, `skills-discovery`, `skill-creator`) help mid-session; still gate writes.

Next: to judge or rank, load `references/quality.md`; if a surface fails, load `references/recovery.md`.
