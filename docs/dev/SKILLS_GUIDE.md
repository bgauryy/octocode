# Skills Guide

Skills are markdown instruction sets that teach AI coding clients how to perform specific tasks — code exploration, PR review, architecture audits, documentation, and more.

---

## Available Skills

| Skill | When to use |
|-------|-------------|
| `octocode-install` | Guided Octocode setup: CLI/MCP, auth, IDE config, skills |
| `octocode-cli` | Run Octocode tools directly from the shell |
| `octocode-researcher` | Everyday direct MCP code exploration — find, trace, definitions |
| `octocode-research` | Deep research via HTTP research server (sessions, checkpoints) |
| `octocode-brainstorming` | Validate ideas with prior art and market evidence |
| `octocode-plan` | Plan → implement → verify |
| `octocode-rfc-generator` | RFCs, design docs, options comparison |
| `octocode-engineer` | Architecture-aware implementation, refactors, audits |
| `octocode-pull-request-reviewer` | PR review (remote) or local/staged change review |
| `octocode-roast` | Brutally honest code critique |
| `octocode-prompt-optimizer` | Harden prompts, SKILL.md, and agent instructions |
| `octocode-design` | Design-system and UI architecture guidance |
| `octocode-documentation-writer` | Generate or refresh project documentation |
| `octocode-news` | Recent AI/devtools/web/security update research |
| `octocode-search-skill` | Find, preview, install, or review Agent Skills |
| `octocode-chrome-devtools` | Browser debugging through Chrome DevTools Protocol |
| `agentic-flow-best-practices` | Design/review agentic workflows and MCP/tool boundaries |
| `octocode-slides` | Generate polished HTML slide decks |
| `octocode-stats` | Render Octocode usage/statistics dashboards |

**researcher vs research:** use `octocode-researcher` for direct MCP tool exploration. Use `octocode-research` for the HTTP research-server workflow with sessions and checkpoints.

---

## Installation

Skills install into one or more client skill directories and are picked up automatically.

```bash
npx octocode-cli skills list                                         # check install status
npx octocode-cli skills install --skill octocode-researcher          # install one
npx octocode-cli skills install -k octocode-plan                     # short flag
npx octocode-cli skills install --skill octocode-researcher --force  # update
npx octocode-cli skills install                                      # prompt for targets/mode, then install all
npx octocode-cli skills install --targets claude-code,cursor,codex   # multi-target install
npx octocode-cli skills install --targets claude-code,cursor --mode symlink # symlink mode
npx octocode-cli skills remove --skill octocode-researcher --targets claude-code,cursor # remove from targets
```

`skills install` without `--targets`/`--mode` opens prompts to choose platforms and install strategy. Non-interactive runs default to `claude-code` with `copy` mode.

### Install destinations

| Target | Path (macOS/Linux) | Path (Windows) |
|-------|-------------------|----------------|
| `claude-code` (default) | `~/.claude/skills/` | `%APPDATA%\Claude\skills\` |
| `claude-desktop` | `~/.claude-desktop/skills/` | `%APPDATA%\Claude Desktop\skills\` |
| `cursor` | `~/.cursor/skills/` | `%USERPROFILE%\.cursor\skills\` |
| `codex` | `~/.codex/skills/` | `%USERPROFILE%\.codex\skills\` |
| `opencode` | `~/.opencode/skills/` | `%USERPROFILE%\.opencode\skills\` |

**Project-scoped install** — set `"skillsDestDir"` in `~/.octocode/config.json` before running `skills install`:

```json
{ "skillsDestDir": "/your/project/.claude/skills" }
```

`skillsDestDir` customizes the `claude-code` destination only.

Commit `.claude/skills/` (or your chosen target directory) to share skills with your team.

---

## Skills Marketplace

Access via the interactive menu: `npx octocode-cli install` → Manage System Skills → Browse Marketplace.

| Source | Description |
|--------|-------------|
| Octocode Official | Research, planning, review, code quality, docs, roast |
| Build With Claude | Largest collection — 170+ commands |
| Claude Code Plugins + Skills | Organized categories with tutorials |
| Claude Skills Marketplace | Git automation, testing, code review |
| Daymade Claude Skills | Production-ready development |
| Superpowers | TDD, debugging, git worktrees |
| Claude Scientific Skills | Scientific computing |
| Dev Browser | Browser automation with Playwright |
| Agent Skills | Web APIs and agent workflow skills |
| Everything Claude Code | Large cross-domain skill library |
| Antigravity Awesome Skills | Massive community skills collection |
| Obsidian Skills | Obsidian and Markdown agent skills |

---

## Skill Structure

```
{skill-name}/
├── SKILL.md          # Main reference (<500 lines)
└── references/       # Supporting docs (optional)
```

### SKILL.md format

```yaml
---
name: skill-name
description: Use when [specific triggers]...
---

# Skill Title

## Flow Overview
`PHASE1` → `PHASE2` → `PHASE3`

## 1. Agent Identity
<agent_identity>
Role: **Agent Type**.
**Objective**: What the agent does.
**Principles**: Core behaviors.
</agent_identity>

## 2. Scope & Tooling
<tools>
| Tool | Purpose |
|------|---------|
| `toolName` | When to use |
</tools>

## 3. Execution Flow
<key_principles>
Step-by-step lifecycle.
</key_principles>

## 4. Error Recovery
<error_recovery>
How to handle failures.
</error_recovery>
```

### Creating custom skills

| Do | Don't |
|----|-------|
| List trigger phrases in `description` | Describe the workflow in `description` |
| Use XML tags for sections | Use plain markdown headers only |
| Include confidence levels | Assume all findings are certain |
| Add user checkpoints | Execute major actions without confirmation |
| Cite `file:line` precisely | Give vague file references |

Keep SKILL.md under 500 lines. Use `references/` for extended content.

---

## Troubleshooting

**Skills not loading:**
1. `ls ~/.claude/skills/` (or target path) — verify the skill folder exists.
2. Check `SKILL.md` has valid frontmatter (`name` and `description` fields).

**Skill not triggering:** mention the skill name explicitly, or check that the `description` field matches your use case.

**Marketplace fetch errors:** GitHub API rate limits may apply — retry later or install from the bundled skills instead.

---

[CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) | [Claude Skills Documentation](https://support.anthropic.com/en/articles/10176498-how-to-use-custom-instructions-for-your-projects)
