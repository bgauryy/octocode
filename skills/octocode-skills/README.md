# Octocode Skills

Discover, evaluate, create, improve, install, synchronize, and verify standalone Agent Skill folders.

**Use when** a `SKILL.md` trigger, workflow, route, hook, or install destination needs work; when you compare or install skills from a local path, repository, or registry; or when a skill folder needs structural review, cleanup, or publication checks.

**Not for** code logic owned by another skill, open ideation (`octocode-brainstorming`), or code architecture (`octocode-architect`).

The flow and routes live in `SKILL.md`; folder shape lives in `references/skill-anatomy.md`.

```bash
npx -y octocode skill install octocode-skills
node scripts/skill-review.mjs <skill-or-collection>   # errors block completion; warnings need a fix or a reason
node scripts/skill-review.mjs --self-test             # maintainer check
```

## Sources

This skill drew on skills.sh install rankings (`code review`, `skill search agent`, `find skills install`); `vercel-labs/skills` find-skills (discovery and gate UX); `anthropics/skills` skill-creator (creation flow); `obra/superpowers` brainstorming (research → recommend); and the agentskills.io, aiskillstore.io, claude-plugins.dev, and Microsoft Sensei surfaces.
