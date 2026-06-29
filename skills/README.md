# Octocode Skills

Octocode Skills are the workflow layer on top of the Octocode tool catalog. The tools can search, read, inspect, and analyze code; the skills tell an agent when to use those abilities, how to keep evidence straight, and what kind of answer the user should receive.

Think of this directory as a set of technical operating modes. Each skill gives an agent a specific kind of judgment: quick lookup, durable awareness, idea validation, deep research, proposal writing, critique, skill authoring, or usage reporting.

## What These Skills Give Agents

Skills turn broad model capability into repeatable engineering behavior. A skill can make an agent remember prior work, search before deciding, compare options before proposing, or keep a code roast funny without becoming sloppy.

The important split is simple: `SKILL.md` is the compact contract the agent reads during a task, while each README explains the user-facing idea in human terms.

## Capabilities

| Skill | Agent capability | User result |
|---|---|---|
| `octocode` | Fast Octocode-powered lookup across local code, GitHub, packages, PRs, symbols, and artifacts. | A focused answer with paths, refs, and proof anchors. |
| `octocode-awareness` | Shared awareness across agents, runs, files, memories, locks, and verification state. | Safer collaboration in dirty or concurrent workspaces. |
| `octocode-brainstorming` | Evidence-grounded idea validation before a product or technical bet hardens. | A decision brief with framings, evidence, risks, and next step. |
| `octocode-research` | Evidence-first technical research, review, planning, change, and repeated investigation. | Proof-backed findings, plans, patches, or reviews. |
| `octocode-rfc-generator` | Research-backed RFC and implementation-plan generation before risky changes. | A reviewable proposal with alternatives, risks, rollout, and rollback. |
| `octocode-roast` | Sharp but cited code critique with tone and safety boundaries. | Memorable findings plus repair paths. |
| `octocode-skills` | Skill discovery, evaluation, creation, installation planning, and README quality work. | Better skills with clearer triggers, safer installs, and leaner agent instructions. |
| `octocode-stats` | Usage and savings interpretation from Octocode telemetry. | A local dashboard and concise readout of calls, savings, cache, and errors. |

## Operating Model

All skills follow progressive disclosure. First the agent sees the skill name and description. When a task matches, the agent reads that skill's `SKILL.md`. Only then does it open the specific references needed for the situation.

That keeps routine tasks light while preserving deeper behavior for fragile work. The user sees a concise result; the agent still has a structured path for evidence, gates, recovery, and handoff.

## Choosing A Skill

Start with `octocode` for bounded lookups. Move to `octocode-research` when the task needs investigation, review, refactor planning, implementation, or repeated evidence loops. Use `octocode-brainstorming` before the idea is fully shaped, and `octocode-rfc-generator` after enough evidence exists to write a proposal.

Add `octocode-awareness` whenever work is shared, stateful, long-running, cross-agent, or worth remembering. Use `octocode-skills` for work on this directory or third-party skills. Use `octocode-stats` only for telemetry and dashboards. Use `octocode-roast` when the user explicitly wants critique with bite.

## Installation

Install a published skill with the Octocode skill command, for example:

```bash
npx octocode skill --name octocode-research
```

Agent Skills are separate from MCP or IDE setup. Configure the Octocode MCP server for an editor or agent host only when you need tool access there.

## Maintainer Notes

Keep README files explanatory and user-facing. Keep activation-critical behavior in `SKILL.md`, deeper workflows in focused references, and deterministic implementation details behind the skill's internal helpers. A good README should help users understand why the skill exists before they ever inspect its internals.
