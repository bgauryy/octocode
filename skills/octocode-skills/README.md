# Octocode Skills

Discover, evaluate, create, improve, install, synchronize, and verify standalone Agent Skill folders.

## Use when

- A `SKILL.md` trigger, workflow, route, hook, or install destination needs work.
- You need to compare or install skills from a local path, repository, or registry.
- A skill folder needs structural review, cleanup, or publication checks.

## Folder contract

- `SKILL.md` owns the workflow, hard rules, stop conditions, and route table.
- References own one concept each and remain inside the skill folder.
- Keep every shipped file reachable and useful; remove duplicate, development-only metadata, probe, and scratch files.
- Use scripts for deterministic work, and route them from the lobby or import them from a used script.

## Workflow

```text
UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → CLEANUP → REVIEW → VERIFY
```

## Install

```bash
npx octocode skill install octocode-skills --platform codex
```

## Review a skill

```bash
node scripts/skill-review.mjs <skill-or-collection>
```

The review checks triggers, routes, internal-only references, whole-folder usage, portability, and navigation. Errors block completion; warnings require correction or explanation.

## Maintainer verification

```bash
node scripts/skill-review.mjs --self-test
node scripts/skill-review.mjs ..
```
