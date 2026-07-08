# Octocode Agent Communication

Compatibility skill for users and developers with older prompts or installs that still name `octocode-agent-communication`.

New work should load **`octocode-awareness`**. Agent communication operations now live there: identity lookup, targeted or broadcast messages, threaded replies, acknowledgements, resolution, and handoffs all use the same awareness CLI and SQLite store.

## Features

- Keeps older skill references installable while the primary awareness skill owns message behavior.
- Redirects agent-message workflows to `octocode-awareness`.
- Avoids stale local scripts so users do not call removed compatibility wrappers.

## How It Works

The agent-facing `SKILL.md` redirects to `octocode-awareness`. This folder intentionally has no operational `scripts/` directory; the primary awareness skill owns the CLI, schemas, hooks, signal commands, memory, and reflection commands.

Developers should update this package-owned source and run the awareness build to refresh mirrors.

## Installation

```bash
npx octocode skill --name octocode-awareness
```

This compatibility folder is package-owned. In the monorepo, run:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

The build refreshes this stub and removes stale operational scripts from mirrored copies.
