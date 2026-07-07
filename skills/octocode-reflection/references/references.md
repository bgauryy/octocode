# References

Sources consulted to research and create this skill.

## GitHub Sources Inspected

| File | Owner/Repo | Path | Quality | Notes |
|------|-----------|------|---------|-------|
| Hermes memory docs | NousResearch/hermes-agent | website/docs/user-guide/features/memory.md | High | Compared persistent memory, background review, and write approval patterns. |
| Hermes skills docs | NousResearch/hermes-agent | website/docs/user-guide/features/skills.md | High | Compared `/learn`, agent-managed skills, staged skill writes, and skill hub behavior. |
| Hermes kanban docs | NousResearch/hermes-agent | website/docs/user-guide/features/kanban.md | Medium | Used only to separate coordination from reflection/orchestration concerns. |

## Local Sources

| File | Path | Notes |
|------|------|-------|
| Octocode Awareness skill | /Users/guybary/Documents/octocode-mcp/packages/octocode-awareness/skills/octocode-awareness/SKILL.md | Source skill split into coordination and reflection triggers. |
| Awareness data model | /Users/guybary/Documents/octocode-mcp/packages/octocode-awareness/skills/octocode-awareness/references/data-model.md | Confirms shared SQLite tables for memories, tasks, signals, refinements, and audit. |
| Pi memory tools | /Users/guybary/Documents/octocode-mcp/packages/octocode-pi-extension/docs/TOOLS.md | Confirms Pi exposes memory/awareness tools through direct imports. |
| Pi reflection docs | /Users/guybary/Documents/octocode-mcp/packages/octocode-pi-extension/docs/REFLECT.md | Existing implementation detail for reflection, harness export, and memory cleanup. |
| Octocode skills rubric | /Users/guybary/Documents/octocode-mcp/skills/octocode-skills/references/agent-skills-guide.md | Skill split and progressive-disclosure rules. |
