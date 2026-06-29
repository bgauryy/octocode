# Octocode Skills

This directory contains the user-installable Agent Skills that turn Octocode from a tool catalog into task-specific workflows. Each skill is a self-contained folder with `SKILL.md` for agents, optional `references/`, `scripts/`, `assets/`, and this human-facing README pattern.

Use this page as the chooser. Open an individual skill README when you want the full user guide, install command, workflow, internals, and maintainer notes for that skill.

## Skills at a glance

| Skill | Use when | Main result |
|---|---|---|
| `octocode` | You need a quick Octocode-powered code, repo, package, PR, symbol, or artifact lookup. | A focused answer with paths, refs, and proof anchors. |
| `octocode-awareness` | Work happens in a dirty repo, across runs, or with multiple agents. | Memory, file locks, handoffs, peer messages, and verification records. |
| `octocode-brainstorming` | An idea needs validation, prior art, white-space mapping, or structured challenge. | A decision brief: build, prototype, narrow, park, or reject. |
| `octocode-research` | Technical work needs evidence-first research, code review, implementation, refactor, or loops. | Proof-backed findings, plans, patches, or reviews with verification. |
| `octocode-rfc-generator` | A risky or cross-cutting change needs a written RFC, design doc, migration, or implementation plan. | A reviewable proposal with evidence, alternatives, risks, and steps. |
| `octocode-roast` | The user explicitly wants blunt, entertaining code-quality critique. | Severity-ranked findings with humor, citations, and repair paths. |
| `octocode-skills` | The task is about Agent Skills, `SKILL.md`, installation, quality, or publishing. | Skill discovery, review, linting, creation, install, or refactor guidance. |
| `octocode-stats` | The user wants Octocode usage, savings, cache, error, or rate-limit stats. | A local HTML dashboard plus key numbers. |

## When to use which skill

- Start with `octocode` for quick, bounded lookups.
- Use `octocode-research` for normal engineering work: investigate, review, change, refactor, plan, or iterate until proof.
- Use `octocode-brainstorming` when the idea itself is still fuzzy or market/product-shaped.
- Use `octocode-rfc-generator` after enough evidence exists and the output should be a document.
- Add `octocode-awareness` whenever work is stateful, long-running, concurrent, or worth remembering.
- Use `octocode-skills` for this directory, third-party skills, install targets, linting, trigger tuning, and README quality.
- Use `octocode-stats` only for stats dashboards and telemetry interpretation.

## Installation

List skills:

```bash
npx octocode skill --list
```

Install a published skill:

```bash
npx octocode skill --name octocode-research
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-research
```

Agent Skills are separate from MCP or IDE setup. Use `npx octocode install --ide <client>` when you want to configure the Octocode MCP server for an editor or agent host.

## How the skills work internally

All skills follow progressive disclosure:

1. The agent sees only skill names and descriptions during discovery.
2. When a user task matches a description, the agent reads that skill's `SKILL.md`.
3. `SKILL.md` routes conditional detail into focused `references/` files.
4. Deterministic or repeatable work lives in `scripts/`.
5. The final response gives the user a concise result, evidence, confidence, and a practical next step.

This split keeps activation context lean while still shipping deep workflows, examples, evals, hooks, and deterministic helpers.

## README standard

Every skill README should explain:

- What the skill does and who should use it.
- When to use it, and when to choose a different skill.
- User-visible features and expected output.
- The internal flow the agent follows.
- Installation with `npx octocode skill ...`.
- Benefits for users.
- Notes for developers and maintainers.

## Publishing notes

The README is the user guide. `SKILL.md` is the agent contract. Keep duplicated facts minimal: put user-facing concepts here, activation-critical rules in `SKILL.md`, detailed workflows in `references/`, and mechanical checks in `scripts/`.
