# Octocode Documentation Skill

Evidence-backed documentation skill for humans and coding agents.

## Features

- Modes: agent-docs (`AGENTS.md`), human-docs (Diátaxis), ADRs, codebase-pack handoff
- Progressive refs for research, writing style, and verify gates
- Durable cross-refs — no brittle line citations or code dumps by default
- Outline gate before writes

## How it works

1. Choose mode (`references/modes.md`)
2. Research with durable evidence (`references/evidence-research.md`)
3. Classify and draft using mode refs + `references/agent-readable.md`
4. Gate outline, write, verify (`references/write-verify.md`)

## Audiences

| Audience | Use for |
|----------|---------|
| Users / maintainers | README, API docs, runbooks, ADRs, AGENTS.md index |
| Developers extending the skill | refs under `references/`, review via `octocode-skills` |
| Coding agents | activation via description triggers; follow lobby routes |

## Installation

```bash
npx octocode skill --add --path skills/octocode-documentation
```

Alternate: copy or symlink into `.cursor/skills/octocode-documentation` or `.agents/skills/octocode-documentation`.

This repo vendors the skill at `skills/octocode-documentation`.
