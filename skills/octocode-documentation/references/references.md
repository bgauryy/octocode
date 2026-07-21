# References

Sources consulted to research and create this skill.

## Skills.sh API Results

| Skill | Source | Installs | Used for |
|-------|--------|----------|----------|
| documentation-writer | github/awesome-copilot | 23351 | Outline-approval gate |
| documentation-and-adrs | addyosmani/agent-skills | 14102 | ADR why/alternatives |
| create-agentsmd | github/awesome-copilot | 11737 | Section inventory; rejected verbose default |
| documentation | anthropics/knowledge-work-plugins | 7322 | Doc-type menus; too thin alone |
| agents-md | getsentry/skills | 4056 | Concise AGENTS.md + anti-patterns |
| documentation | mcollina/skills | 1243 | Diátaxis decision table |

## GitHub Sources Inspected

| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| agents-md | getsentry/skills | skills/agents-md/SKILL.md | High | Index-style agent docs |
| documentation | mcollina/skills | skills/documentation/SKILL.md | High | Type separation |
| documentation-and-adrs | addyosmani/agent-skills | skills/documentation-and-adrs/SKILL.md | High | ADR convention-first |
| documentation-writer | github/awesome-copilot | skills/documentation-writer/SKILL.md | Medium | Outline gate |
| create-agentsmd | github/awesome-copilot | skills/create-agentsmd/SKILL.md | Medium | Too verbose as default |

## Registry and Marketplace Surfaces

| Surface | Query | Finding |
|---------|-------|---------|
| skills.sh API | documentation, AGENTS.md, diataxis | Ranked by installs; filtered language/OCR noise |
| GitHub code search | filename SKILL.md + docs terms | Confirmed inspect paths |

## Local Sources

| File | Path | Notes |
|------|------|-------|
| octocode-skills | ~/.claude/skills/octocode-skills | Lobby ≤50, one-concept refs, review gate |
| octocode-prompt-optimizer | ~/.claude/skills/octocode-prompt-optimizer | Density, IF/THEN, no waste tokens |
| octocode-search-skill | ~/.cursor/skills/octocode-search-skill | Operating model + gates |
| octocode-documentation-writer | ~/.cursor/skills/octocode-documentation-writer | Evidence + codebase-pack handoff |
