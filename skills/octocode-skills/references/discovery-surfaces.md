# Discovery Surfaces

Load when shopping beyond raw GitHub. Why: pick the right registry/API for the user goal.

Cross-check ≥2 surfaces; prefer activity evidence (installs, updates, audits).

| User goal | First surface |
|-----------|---------------|
| Keyword/topic search | skills.sh `/api/search?q=` (sort `installs`) ∥ GitHub ∥ web |
| Mentions outside registries | WebSearch → confirm `SKILL.md` via Octocode |
| Is X published / battle-tested? | `https://www.skills.sh/<owner>/<repo>/<skill>` |
| Most-installed now | `https://www.skills.sh` leaderboard |
| Claude Code plugin | `claude-plugins.dev` |
| Curated multi-agent browse | `agentskills.me`, `agentskills.io` |
| Capability tag search | `aiskillstore.io/v1/agent/search?capability=` |
| New-skill firehose | `aiskillstore.io/feed/new-skills.json` |
| Microsoft / Azure | `microsoft.github.io/skills` + `llms-full.txt` |
| Claude marketplace dirs | `claudemarketplaces.com`, `mcpmarket.com/tools/skills` |

## Per-skill check

Confirm on skills.sh before recommending. Page gives installs, install cmd, audit badge, siblings. 404 → source repo + lower confidence.

## Surface notes

- `agentskills.io` — standard hub; `/clients` + `llms.txt`.
- `skills.sh` — leaderboard + search API; `npx skills add <url> --skill <name>`.
- `claude-plugins.dev` — auto-index + REST + `npx skills-installer`.
- `aiskillstore.io` — capability search, trust levels, `/similar`, demand feed.
- Aggregators (moderate confidence): verify each entry; LobeHub = discovery-only (prompt-install risk).

Next: when parsing manifests/CLIs load `references/discovery-manifests.md`; when ranking load `references/quality-signals.md`; if a surface fails load `references/recovery.md`.
