# Octocode Agent Communication

`octocode-agent-communication` gives agents a focused way to talk to each other inside a shared workspace. It covers identity lookup, targeted or broadcast messages, threaded replies, acknowledgements, resolution, and handoffs. Users get a smaller phrase to ask for when they want agents to coordinate; maintainers get a narrow package surface for communication behavior.

## Features

- Register and list known agents through the shared awareness `agents` table.
- Publish typed messages through the shared `signals` table.
- Read an inbox without confusing messages with memory, locks, or verification debt.
- Reply in threads and resolve them when the work is complete.
- Use hooks as delivery/reminder surfaces while keeping messages in SQLite.
- Map local communication to an A2A-style model without running a public protocol server by default.

## How It Works

The skill is package-owned by `packages/octocode-awareness`. It uses the same local SQLite database as `octocode-awareness` and `octocode-reflection`, so agent identities, messages, read receipts, locks, refinements, and lessons stay in one workspace-scoped store.

The core workflow is:

```text
REGISTER -> DISCOVER -> SEND -> RECEIVE -> REPLY -> ACK -> RESOLVE
```

`agent-registry` manages the identity layer. `agent-signal` manages the message layer. Hooks can surface unread messages at lifecycle moments, but they are not the message broker and they do not implicitly acknowledge work.

For maintainers, the important boundary is simple: add communication ergonomics here, keep storage and lifecycle mechanics in the awareness package, and do not add another message database.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-agent-communication
```

This skill is package-owned. In the monorepo, run:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

The build injects shared `awareness.mjs` and `schema.mjs` scripts into package-owned skills.
