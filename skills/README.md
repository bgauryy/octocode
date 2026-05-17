# OctoCode Skills

Specialized AI agent skills extending OctoCode. 18 skills live under `skills/`.

---

## Skill Lookup

| Skill | Directory | Use For |
|-------|-----------|---------|
| **Install** | `octocode-install/` | Set up OctoCode, auth, IDE MCP, and skills. |
| **CLI** | `octocode-cli/` | Run Octocode tools from the shell. |
| **Researcher** | `octocode-researcher/` | Fast code search, symbol lookup, and file discovery. |
| **Research** | `octocode-research/` | Multi-phase investigations with checkpoints. |
| **Brainstorming** | `octocode-brainstorming/` | Validate ideas with prior art and market evidence. |
| **Plan** | `octocode-plan/` | Build evidence-backed implementation plans. |
| **RFC Generator** | `octocode-rfc-generator/` | Compare approaches and write technical proposals. |
| **Engineer** | `octocode-engineer/` | Understand, implement, refactor, and audit code. |
| **PR Reviewer** | `octocode-pull-request-reviewer/` | Review PRs or local diffs for defects and risk. |
| **Roast** | `octocode-roast/` | Brutal but actionable code-quality critique. |
| **Prompt Optimizer** | `octocode-prompt-optimizer/` | Harden prompts, skills, and agent instructions. |
| **Design** | `octocode-design/` | Generate design-system and UI architecture guidance. |
| **Doc Writer** | `octocode-documentation-writer/` | Produce comprehensive project documentation. |
| **News** | `octocode-news/` | Research recent AI, DevTools, web, and security updates. |
| **Search Skill** | `octocode-search-skill/` | Find, preview, and download agent skills. |
| **Chrome DevTools** | `octocode-chrome-devtools/` | Open Chrome via CDP, generate inspection scripts, trace errors to source. |
| **Agentic Flow Best Practices** | `agentic-flow-best-practices/` | Design agentic workflow patterns, agent collaboration, MCP/tool boundaries, context, memory scopes, caching, prompts, model settings, gates, and verification. |

---

## Skill Details

### Install
Guided setup for OctoCode CLI/MCP, authentication, IDE config, and skill installation.

### CLI
Terminal workflow for Octocode tools, including code search, file reads, repo search, PR search, and package lookup.

### Researcher
Default targeted research skill for local/GitHub code exploration, LSP navigation, callers, references, and package research.

### Research
Stateful deep research flow for broad, multi-step questions that need phases, checkpoints, and evidence synthesis.

### Brainstorming
Evidence-first idea validation across GitHub, package ecosystems, and web sources; outputs a decision brief.

### Plan
Turns researched context into concrete implementation steps, risks, tests, and execution order.

### RFC Generator
Creates technical decision docs with alternatives, trade-offs, recommendation, and rollout plan.

### Engineer
Architecture-aware engineering skill for exploration, coding, analysis, audits, refactors, and quality checks.

### PR Reviewer
Holistic review of remote PRs or local changes, focused on bugs, security, architecture, flow impact, and tests.

### Roast
Entertaining severity-ranked critique with concrete fixes for code smells, antipatterns, and maintainability issues.

### Prompt Optimizer
Improves long prompts and agent instructions with gates, failure-mode controls, and enforceable protocols.

### Design
Builds practical UI/design-system guidance for visual language, components, accessibility, performance, and responsiveness.

### Doc Writer
Documentation pipeline for onboarding, architecture, APIs, workflows, and validated developer docs.

### News
Scans recent AI, developer tooling, web platform, security, and notable-repo updates into a concise report.

### Search Skill
Searches GitHub for `SKILL.md` files, scores relevance, previews results, and downloads selected skill folders.

### Chrome DevTools
Opens Chrome with CDP WebSocket debugging, generates a custom inspection script per task, runs it live, and traces errors back to source code using Octocode local tools. Covers network, console, performance, DOM, CSS, screenshots, iframes, service workers, and any CDP domain. Includes script self-review, CDP error retry, and multi-target routing.

### Agentic Flow Best Practices
Designs practical agentic flows by choosing the simplest reliable pattern, shaping context packets, defining MCP/tool and skill boundaries, planning session/agent/shared memory scopes, tuning model configuration, and adding gates and verification where risk demands it.
