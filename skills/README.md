# Octocode Skills

This directory contains 10 user-installable Agent Skills for Octocode workflows. A README is for humans: it explains why a skill exists, when to ask for it, what result to expect, and which nearby skill to choose instead. A `SKILL.md` is for agents: it holds activation rules and execution steps.

Use this page as the chooser. Open an individual README when you want to understand what that skill does for you before installing or invoking it.

## Choose by user need

| You want the agent to... | Use | What it does for you |
|---|---|---|
| Use Octocode for a quick lookup | `octocode` | Routes the agent to Octocode MCP or CLI for focused search, reads, symbols, repos, packages, PRs, and artifacts. |
| Coordinate work across runs or agents | `octocode-awareness` | Adds memory, file locks, handoffs, peer messages, and verify-before-done discipline. |
| Explore whether an idea is worth pursuing | `octocode-brainstorming` | Turns a fuzzy idea into a decision brief using prior art, market/code evidence, and structured pushback. |
| Investigate, review, refactor, or implement code | `octocode-engineer` | Gives the agent architecture-aware code research and change workflow with exact evidence. |
| Iterate on a clear research question | `octocode-loop` | Keeps the agent in Act -> Observe -> Learn loops until evidence converges or a budget stops it. |
| Run broad technical research without editing | `octocode-research` | Maps, validates, investigates, or plans from local code, GitHub, npm, history, artifacts, and formal sources. |
| Write an RFC or implementation plan | `octocode-rfc-generator` | Converts evidence and alternatives into a reviewable technical decision document. |
| Get blunt code-quality critique | `octocode-roast` | Finds real code smells with humor, severity, citations, and repair paths. |
| Work on Agent Skills themselves | `octocode-skills` | Finds, evaluates, installs, creates, lints, and improves `SKILL.md` folders. |
| See Octocode usage savings | `octocode-stats` | Builds a local dashboard from Octocode MCP stats. |

## Smart routing

- Fuzzy idea or product hunch: start with `octocode-brainstorming`.
- Clear technical question with no edits requested: use `octocode-research`.
- Clear question that needs repeated evidence loops: use `octocode-loop`.
- Code change, review, refactor, bug hunt, or architecture investigation: use `octocode-engineer`.
- Decision needs a written proposal before coding: use `octocode-rfc-generator`.
- Quick one-off lookup: use `npx octocode`.
- Shared dirty repo, long task, handoff, or concurrent agents: add `octocode-awareness`.
- Skill authoring or skill cleanup: use `octocode-skills`.

## Install

List available skills:

```bash
npx octocode skill --list
```

Install one skill:

```bash
npx octocode skill --name octocode-engineer --platform codex
```

Common platform targets are `common`, `codex`, `cursor`, `claude`, `opencode`, and `all`. Agent Skills are separate from MCP or IDE setup; use `npx octocode install --ide <client>` for that.

## What good skill output looks like

The exact artifact changes by skill, but the standard is the same: a concise user-facing answer, evidence behind important claims, honest confidence, and a next step that fits the task. Raw tool output stays behind the curtain unless it is needed for proof.
