# Octocode Reflection

Post-task learning and memory maintenance for Octocode agents.

## Features

- Records verified durable lessons with `memory_record`.
- Reflects on completed work with `memory_reflect`.
- Reviews stale, duplicate, wrong, expired, or superseded memories.
- Stages skill, harness, AGENTS.md, and repo-instruction changes for human approval.
- Shares the awareness SQLite store with `octocode-awareness` while keeping live coordination in the awareness skill.

## How It Works

Use `octocode-awareness` while thinking, planning, editing, locking files, signaling, and verifying claims. Use `octocode-reflection` after an outcome is known or when the user asks to learn, prune, approve, or improve guidance.

The skill routes all state changes through `scripts/awareness.mjs` and its schema helper. It does not install edit hooks or own live workspace locks.

## Installation

```bash
npx octocode skill --name octocode-reflection
```

This skill is package-owned. Run:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

The package source remains canonical. The build refreshes scripts here and removes any stale root `skills/octocode-reflection` copy.
