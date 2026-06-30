# Octocode Harness

> The Octocode team's preferred and recommended harness for coding agents.
> Use it when you want agents to research with evidence, edit conservatively,
> verify their work, and preserve important context across long tasks.

The Octocode harness is a user-facing setup for making coding agents behave like
careful engineering partners instead of loose terminal wrappers. It pairs a lean
agent host, preferably Pi, with Octocode's evidence tools, skills, and memory
workflow:

```text
orient -> search/read exact evidence -> decide or plan -> patch when asked
       -> verify -> remember or hand off what matters
```

Pi is the primary recommended host because its upstream docs describe it as a
minimal terminal coding harness extended through skills, extensions, prompt
templates, themes, and packages. Pi edits and orchestrates; Octocode supplies
the research, planning, review, memory, and verification discipline.

Use this overview to understand the harness at a glance. For copy-paste setup,
open the Pi setup guide. For the actual prompt rules loaded by Pi, open the
append-system prompt.

## Who This Is For

Use the Octocode harness when you want a coding agent to:

- investigate local code, GitHub, packages, history, binaries, AST, and LSP with
  exact evidence;
- plan risky changes before editing;
- remember durable lessons and hand off long-running work;
- use delegated Pi workers, a subagent extension, or `pi -p` for broad
  research, review, and summarization without flooding the main context;
- verify before claiming completion.

## Canonical Documents

Keep these documents separate so the same content does not drift in multiple
places:

| Document | Role | Owns |
|---|---|---|
| [docs/PI/APPEND_SYSTEM.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md) | Runtime prompt addendum | The actual Pi system-prompt rules: evidence discipline, context management, compaction, delegation, subagents, verification, and safety. |
| [docs/PI/PI_SETUP_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md) | User cookbook | Installation, adding the prompt to Pi, installing skills, optional MCP setup, model setup, and practical Pi prompts. |
| This file | Harness overview | Why the harness exists, what changed, how the pieces fit, and which detailed doc to open next. |

Rule of thumb: if a user would copy it into Pi, it belongs in
`APPEND_SYSTEM.md`; if a user would run it in a shell, it belongs in
`PI_SETUP_GUIDE.md`; if it explains the architecture of the harness, it belongs
here.

## What Changed

The current harness update splits one large duplicated guide into focused
owners:

- **Prompt hardening moved to `APPEND_SYSTEM.md`.** The prompt now treats
  context as scarce working memory, writes durable state to files before
  compaction or delegation, uses delegated workers for token-heavy research and
  review, and keeps verification-before-done explicit.
- **Pi onboarding moved to `PI_SETUP_GUIDE.md`.** The guide is now a cookbook:
  install Pi, authenticate Octocode, add the prompt, install Octocode skills,
  optionally configure MCP, and use the skill cookbook.
- **This harness doc is now an overview.** It avoids repeating install commands,
  MCP JSON, or the system prompt. It points to the source documents instead.

## Harness Components

| Component | Responsibility | Details live in |
|---|---|---|
| Pi | Minimal terminal agent, project context, slash commands, skill loading, shell-native execution, and extension points for optional workflows. | [docs/PI/PI_SETUP_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md) |
| Octocode CLI | Reproducible research and setup surface for Pi and scripts. | [docs/OCTOCODE_CLI.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) |
| Octocode MCP | Typed tool surface for MCP-native clients. Optional for Pi. | [docs/OCTOCODE_MCP.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) |
| Octocode tools | GitHub, npm, local search, AST, LSP, PR/history, clone, and binary/artifact research. | [docs/OCTOCODE_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) |
| Octocode skills | On-demand operating modes for research, awareness, brainstorming, RFCs, critique, and skill work. | [docs/PI/PI_SETUP_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md) |

## Skill Stack

Use the smallest skill that matches the task. The Pi setup guide owns install
commands and user-facing prompt examples; this table only states the harness
roles.

| Skill | Harness role |
|---|---|
| `octocode` | Quick transport and lookup layer for Octocode research surfaces. |
| `octocode-research` | Default technical-work mode: local/external research, implementation, review, debugging, PR/history, binary inspection, AST, and LSP. |
| `octocode-awareness` | Memory, file locks, handoffs, notifications, verification records, and learning capture. |
| `octocode-brainstorming` | Idea exploration and prior-art validation across articles, code, GitHub, packages, and web evidence. |
| `octocode-rfc-generator` | Pre-implementation RFCs, migration plans, architecture proposals, and option comparisons. |
| `octocode-roast` | Evidence-backed code critique when the user explicitly asks for a hard review. |
| `octocode-skills` | Search, compare, lint, create, and update skills and skill resources. |

## Operating Model

- Prefer Pi skill + Octocode CLI for Pi.
- Prefer MCP tools for MCP-native hosts.
- Treat MCP, subagents, plan mode, permission gates, and sandboxing as optional
  Pi extensions or external wrappers, not assumptions about Pi core.
- Read live schemas/help before raw tool calls.
- Treat search results as leads; exact reads, tests, schemas, and runtime output
  are proof.
- Keep active context small; write durable handoffs and research receipts to
  files before compaction or delegation.
- Use `octocode-awareness` when work is long-running, concurrent, dirty, or
  worth remembering.
- Route risky or cross-package work through `octocode-rfc-generator` before
  implementation.

## Pi Claim Check

Research against upstream Pi docs and source supports these claims:

- Pi is a minimal terminal coding harness designed to stay small at the core and
  be extended through skills, TypeScript extensions, prompt templates, themes,
  and Pi packages.
- Pi loads context files, `APPEND_SYSTEM.md`, skills, slash commands,
  non-interactive print mode, custom models, and compaction.
- Pi does not include built-in MCP, subagents, permission popups, plan mode,
  to-dos, background bash, or a default permission sandbox. Those are extension
  or environment choices.
- Pi has extension examples for subagents, plan mode, permission gates,
  protected paths, tool routing, and sandboxing. The Octocode harness should
  recommend them as optional upgrades, not baseline requirements.

Primary references:

- [Pi docs overview](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/index.md)
- [Pi usage and context files](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/usage.md)
- [Pi skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)
- [Pi subagent extension example](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/examples/extensions/subagent/README.md)
- [Pi permissions and containerization](https://github.com/earendil-works/pi/blob/main/README.md#permissions--containerization)

## Where To Go Next

- Setup or teach a user the harness:
  [docs/PI/PI_SETUP_GUIDE.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/PI_SETUP_GUIDE.md)
- Inspect or edit the prompt that Pi should load:
  [docs/PI/APPEND_SYSTEM.md](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md)
- Understand the CLI:
  [docs/OCTOCODE_CLI.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
- Configure MCP:
  [docs/OCTOCODE_MCP.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
- Review the tool catalog:
  [docs/OCTOCODE_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- Browse installable skills:
  [Octocode skills on skills.sh](https://www.skills.sh/bgauryy/octocode-mcp)
