# Octocode Skills

This directory contains 8 user-installable Agent Skills for Octocode workflows. `octocode-research` is the main skill for technical research, code work, review, refactor, architecture analysis, and evidence loops. A README is for humans: it explains why a skill exists, what features it provides, how it works for users and developers, and how to install it. A `SKILL.md` is for agents: it holds activation rules and execution steps.

Use this page as the chooser. Open an individual README when you want to understand what that skill does for you before installing or invoking it.

## Choose by user need

| You want the agent to... | Use | What it does for you |
|---|---|---|
| Use Octocode for a quick lookup | `octocode` | Routes the agent to Octocode MCP or CLI for focused search, reads, symbols, repos, packages, PRs, and artifacts. |
| Coordinate work across runs or agents | `octocode-awareness` | Adds memory, file locks, handoffs, peer messages, and verify-before-done discipline. |
| Explore whether an idea is worth pursuing | `octocode-brainstorming` | Turns a fuzzy idea into a decision brief using prior art, market/code evidence, and structured pushback. |
| Investigate, review, refactor, or implement code | `octocode-research` | Gives the agent architecture-aware code research and change workflow with exact evidence. |
| Iterate on a clear research question | `octocode-research` | Keeps the agent in Act -> Observe -> Learn loops until evidence converges or a budget stops it. |
| Run broad technical research or planning | `octocode-research` | Maps, validates, investigates, reviews, changes, or plans from local code, GitHub, npm, history, artifacts, and formal sources. |
| Write an RFC or implementation plan | `octocode-rfc-generator` | Converts evidence and alternatives into a reviewable technical decision document. |
| Get blunt code-quality critique | `octocode-roast` | Finds real code smells with humor, severity, citations, and repair paths. |
| Work on Agent Skills themselves | `octocode-skills` | Finds, evaluates, installs, creates, lints, and improves `SKILL.md` folders. |
| See Octocode usage savings | `octocode-stats` | Builds a local dashboard from Octocode MCP stats. |

## Smart routing

- Fuzzy idea or product hunch: start with `octocode-brainstorming`.
- Clear technical question, repeated evidence loop, code change, review, refactor, bug hunt, or architecture investigation: use `octocode-research`.
- Decision needs a written proposal before coding: use `octocode-rfc-generator`.
- Quick one-off lookup: use `npx octocode`.
- Shared dirty repo, long task, handoff, or concurrent agents: add `octocode-awareness`.
- Skill authoring or skill cleanup: use `octocode-skills`.

## Documentation standard

Every skill folder has a README with:

- High-level purpose and when to ask for the skill.
- User-visible features and expected result shape.
- How the skill works, including references, scripts, or hooks when they matter.
- Developer notes for keeping `SKILL.md`, `references/`, and `scripts/` aligned.
- Installation via `npx octocode skill --name <skill>`.

## Install

List available skills:

```bash
npx octocode skill --list
```

Install one skill:

```bash
npx octocode skill --name octocode-research
```

Agent Skills are separate from MCP or IDE setup; use `npx octocode install --ide <client>` for that.

## What good skill output looks like

The exact artifact changes by skill, but the standard is the same: a concise user-facing answer, evidence behind important claims, honest confidence, and a next step that fits the task. Raw tool output stays behind the curtain unless it is needed for proof.
