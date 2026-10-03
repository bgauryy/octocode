# Octocode Skills

Discover, evaluate, create, improve, install, synchronize, and verify standalone Agent Skill folders. The flow and routes live in `SKILL.md`.

```bash
npx -y octocode skill install octocode-skills
node scripts/skill-review.mjs <skill-or-collection>
node scripts/skill-review.mjs --self-test             # maintainer check
```

## Sources

This skill drew on skills.sh install rankings (`code review`, `skill search agent`, `find skills install`); `vercel-labs/skills` find-skills (discovery and gate UX); `anthropics/skills` skill-creator (creation flow); `obra/superpowers` brainstorming (research → recommend); and the agentskills.io, aiskillstore.io, claude-plugins.dev, and Microsoft Sensei surfaces. Authoring rules follow the agentskills.io specification and skill-creation guides, Anthropic's skill authoring best practices, and the Claude Code, Codex, Cursor, OpenCode, and Pi skills docs.
