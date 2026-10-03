# Discovery

Load when you discover skill candidates, shop beyond raw GitHub, parse a marketplace manifest, or pick an installer CLI. Why: set depth and angles first; the right registry answers faster.

## Depth

| Request | Depth |
|---|---|
| Quick | enough to recommend one best candidate with caveats |
| Research | compare broadly; stop when more search does not change the pick |
| Install | inspect source, support files, destinations, conflicts before approval |
| Improve, rate, review, create | inspect the target, local examples, and `references/skill-anatomy.md` first |

With weak results, broaden while another source or query could resolve the gap. Stop when more search is unlikely to change the decision.

## Surfaces

Start with the source most likely to answer. Add independent sources to compare or close a gap; batch reads and dedupe by `(owner/repo, skill name)`. Cross-check at least two surfaces. Confirm a real `SKILL.md` through Octocode before you recommend.

1. Octocode/GitHub through `octocode-research`.
2. skills.sh API (install-ranked, below).
3. Web search: topic + "agent skill" or "SKILL.md".

For local or org-private scope, use Octocode only.

Angles: name (exact, hyphenated, aliases) · subject · workflow verbs · ecosystem (agent, IDE, language, MCP) · safety (gate, verify, scripts).

```bash
curl 'https://www.skills.sh/api/search?q={{SEARCH_KEY}}&limit=100' --compressed \
  -H 'User-Agent: Mozilla/5.0'
```

Fetch each fitting candidate's `SKILL.md` through Octocode. If the API is down, use another surface and report the gap (`references/recovery.md`).

Sparse results: seed from `topic:agent-skills` and `anthropics/skills`, `vercel-labs/skills`, `obra/superpowers`, `microsoft/skills`, `trailofbits/skills`.

| Goal | Surface |
|---|---|
| Keyword or topic search | skills.sh `/api/search?q=` ∥ GitHub ∥ web search |
| Published or battle-tested? | `https://www.skills.sh/<owner>/<repo>/<skill>`; 404 → source repository, lower confidence |
| Claude Code plugin | `claude-plugins.dev` (auto-index, REST) |
| Curated multi-agent browse | `agentskills.io` (`/clients`, `llms.txt`), `agentskills.me` |
| Capability tag search | `aiskillstore.io/v1/agent/search?capability=`; new skills: `aiskillstore.io/feed/new-skills.json` |
| Microsoft / Azure | `microsoft.github.io/skills` + `llms-full.txt` |
| Claude marketplace dirs | `claudemarketplaces.com`, `mcpmarket.com/tools/skills` |

Aggregators carry moderate confidence: verify each entry. LobeHub is discovery-only (prompt-install risk).

## Manifests

| Format | Where |
|---|---|
| `llms.txt` / `llms-full.txt` | agentskills.io, aiskillstore.io, microsoft.github.io/skills |
| `.claude-plugin/marketplace.json`, `.claude-plugin/plugin.json` | marketplace and plugin repositories |
| YAML frontmatter | inside `SKILL.md`: `name`, `description`, optional fields |

## Installer CLIs

Safety-scan the source before any install (`references/install.md`).

| CLI | Pattern |
|---|---|
| `npx -y octocode skill` | `install --add <src> --platform <hosts>`; preferred here |
| `npx skills add` | `<gh-url> --agent <host> --skill <name>`; symlink by default |
| `npx skills-installer` | `install @owner/repo/skill --client <host>` |
| Claude Code native | `/plugin marketplace add`, then `/plugin install` |

Meta-skills (`find-skills`, `skills-discovery`, `skill-creator`) help mid-session; still gate writes.

Next: to judge or rank, load `references/quality.md`; if a surface fails, load `references/recovery.md`.
