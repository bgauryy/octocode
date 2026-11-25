# Octocode skills

Choose a skill by the task you want your agent to complete. Run `npx -y octocode skill list` to see the bundled skills and which are installed.

| Skill | Use it to |
|---|---|
| [octocode-get-started](octocode-get-started/SKILL.md) | Install Octocode, connect your agent, sign in to GitHub, and verify a first tool call. |
| [octocode-research](octocode-research/SKILL.md) | Answer how code works or why it changed, with citations to source files or commits. |
| [octocode-debugging](octocode-debugging/SKILL.md) | Diagnose a crash, wrong result, hang, or slowdown using runtime evidence. |
| [octocode-graph](../packages/octocode-graph/skills/octocode-graph/SKILL.md) | Query dependencies, callers and change impact, build a stitched graph or explore a repository in the viewer. |
| [octocode-brainstorming](octocode-brainstorming/SKILL.md) | Compare ideas or challenge a proposal using verified evidence from the web and GitHub. |
| [octocode-engineer](octocode-engineer/SKILL.md) | Choose an implementation or architecture by comparing trade-offs against the workload. |
| [octocode-generate](octocode-generate/SKILL.md) | Scaffold a new app, service or library: interview, approved plan, version-checked stack, lint/test/CI templates and green quality gates. |
| [octocode-rfc-generator](octocode-rfc-generator/SKILL.md) | Write, revise, or review an RFC, design proposal, or migration plan, with each decision traced to tasks and checks. |
| [octocode-pr-review](octocode-pr-review/SKILL.md) | Review a GitHub pull request and trace each finding to the changed code. |
| [octocode-roast](octocode-roast/SKILL.md) | Get an explicitly requested blunt critique of code or design, with concrete fixes. |
| [octocode-test-plan](octocode-test-plan/SKILL.md) | Write a risk-ranked test plan with setup, expected results, and pass criteria for its executor. |
| [octocode-clean-agentic-code](octocode-clean-agentic-code/SKILL.md) | Remove verified dead code, duplicate helpers, debug leftovers, and unused agent instructions while preserving behavior. |
| [octocode-documentation](octocode-documentation/SKILL.md) | Write, shorten, or review technical docs and check their commands and links. |
| [octocode-scraping](octocode-scraping/SKILL.md) | Extract text, tables, or links from public pages, or save a site for repeated searches. |
| [octocode-chrome-devtools](../packages/octocode-chrome-devtools/skills/octocode-chrome-devtools/SKILL.md) | Operate a live Chrome page, inspect console or network activity, or capture a screenshot or trace. |
| [octocode-agentic](octocode-agentic/SKILL.md) | Design or debug an AI agent and evaluate its prompts, context, tool calls, and behavior. |
| [octocode-skills](octocode-skills/SKILL.md) | Find, author, audit, install, or sync Agent Skills and check their standalone references. |
| [octocode-workflows](octocode-workflows/SKILL.md) | Design or resume a multi-step process with handoffs, retries, and completion checks. |
| [octocode-orchestrator](octocode-orchestrator/SKILL.md) | Assign, track, and recover existing agents on one task, and bring their results together. |
| [octocode-messages-proxy](octocode-messages-proxy/SKILL.md) | Send a message to another agent and receive its reply. |

## Install a skill

From your project directory, install the research skill for Codex. Substitute a skill name from the table and use `skill install --help` to see supported platforms.

```sh
npx -y octocode skill install octocode-research --platform codex --project-dir .
```

Use `--global` for all projects or `--dry-run` to preview the installation. When you ask for a saved report, the usual location is `<workspace>/.octocode/<skill-name>/`. You can also name a destination.
