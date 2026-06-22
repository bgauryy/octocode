# References

Sources actually consulted to research and create the `octocode-skills` skill.

## Skills.sh API Results

Live queries run during creation, sorted by installs descending.

**Query: `code review`** (used to validate API response shape)

| Skill | Source | Installs | Used for |
|-------|--------|----------|----------|
| requesting-code-review | obra/superpowers | 88,768 | Gate and output UX patterns |
| receiving-code-review | obra/superpowers | 70,328 | Workflow step structure |
| code-review-excellence | wshobson/agents | 18,559 | Card output format reference |

**Query: `skill search agent`** (used to survey the discovery-skill landscape)

| Skill | Source | Installs | Used for |
|-------|--------|----------|----------|
| parallel-deep-research | parallel-web/parallel-agent-skills | 8,186 | Parallel research pattern |
| parallel-web-search | parallel-web/parallel-agent-skills | 7,844 | Parallel search pattern |
| skills-search | daymade/claude-code-skills | 398 | Direct competitor — inspected for gap analysis |

**Query: `find skills install`** (used to find meta-skill patterns)

| Skill | Source | Installs | Used for |
|-------|--------|----------|----------|
| find-skills | bytedance/deer-flow | 1,349 | Meta-skill discovery pattern (duplicate of vercel-labs canonical) |
| installing-skills | oaustegard/claude-skills | 40 | Install flow reference |

## GitHub Sources Inspected

SKILL.md files read via Octocode during design.

| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| find-skills | vercel-labs/skills | skills/find-skills/SKILL.md | High | Discovery workflow and gate UX patterns |
| skill-creator | anthropics/skills | skills/skill-creator/SKILL.md | High | Creation flow and resource structure |
| brainstorming | obra/superpowers | brainstorming/SKILL.md | High | Research synthesis and recommend pattern |
| skills-search | daymade/claude-code-skills | skills-search/SKILL.md | Medium | Gap analysis — minimal workflow, no gates |

## Registry and Marketplace Surfaces

| Surface | Query or URL | Finding |
|---------|-------------|---------|
| skills.sh API | `/api/search?q=code+review&limit=10` | Confirmed response shape: `{"skills":[...], "count": N}` with `installs`, `source`, `skillId` |
| skills.sh API | `/api/search?q=skill+search+agent&limit=20` | Identified `daymade/claude-code-skills/skills-search` as closest existing competitor |
| skills.sh API | `/api/search?q=find+skills+install&limit=20` | Confirmed `find-skills` pattern; canonical at vercel-labs, widely forked |
| agentskills.io | `agentskills.io/llms.txt` | Agent-readable catalog format; confirmed compatible-agents list |
| aiskillstore.io | `aiskillstore.io/llms.txt` | USK v1.0 spec; capability tag taxonomy and API surface |
| claude-plugins.dev | Web UI | Auto-indexed GitHub SKILL.md discovery; confirm-before-install pattern |
| microsoft.github.io/skills | `llms-full.txt` | Sensei scoring rubric (triggers + anti-triggers + compatibility) |

## Local Sources

| File | Path | Notes |
|------|------|-------|
| agent-skills-guide.md | references/agent-skills-guide.md | Quality rubric, progressive disclosure pattern, description optimization |
| discovery-surfaces.md | references/discovery-surfaces.md | Marketplace surface catalog; routing table |
| install-reference.md | references/install-reference.md | 4-gate install protocol; provider matrix |
| fetch-and-create-locally.md | references/fetch-and-create-locally.md | Remote-to-local fetch and adapt workflow |
