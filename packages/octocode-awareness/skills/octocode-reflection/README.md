# Octocode Reflection

Compatibility skill for users and developers with older prompts or installs that still name `octocode-reflection`.

New work should load **`octocode-awareness`**. Reflection operations now live there: durable lessons, `memory_reflect`, stale-memory cleanup previews, pending-task maintenance, and staged harness improvements all use the same awareness CLI and SQLite store.

## Features

- Keeps older reflection skill references installable during the move to `octocode-awareness`.
- Redirects memory, reflection, cleanup, and finish-check workflows to the primary skill.
- Avoids stale local scripts so users do not call removed compatibility wrappers.

## How It Works

The agent-facing `SKILL.md` redirects to `octocode-awareness`. This folder intentionally has no operational `scripts/` directory; the primary awareness skill owns the CLI, schemas, hooks, memory, signals, and reflection commands.

Developers should update this package-owned source and run the awareness build to refresh mirrors.

## Installation

```bash
npx octocode skill --name octocode-awareness
```

This compatibility folder is package-owned. Run:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

The package source remains canonical. The build refreshes this stub and removes stale operational scripts from mirrored copies.
