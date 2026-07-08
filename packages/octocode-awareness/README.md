# @octocodeai/octocode-awareness

Shared awareness, memory, coordination, reflection, and hook runtime for AI coding agents.

This package owns the awareness system. It produces:

- a zero npm runtime dependency TypeScript library,
- the `awareness` CLI for every awareness operation,
- the package-owned Agent Skill sources,
- generated skill scripts for the primary `octocode-awareness` skill,
- host hook installers and hook runners,
- the Pi in-process awareness bridge.

It does not own the Octocode research tools, the `octocode` installer CLI, Pi's system prompt, or config/env loading. Those live in their own packages. This package only provides the shared awareness runtime and the skill/CLI surfaces that operate it.

## What It Manages

One local SQLite store tracks workspace state across agents and sessions:

- memories and durable lessons,
- file locks and edit tasks,
- verification gates,
- agent registry and signals,
- refinements and handoffs,
- reflection records and harness proposals,
- hook/session audit events.

Default store: `~/.octocode/memory/awareness.sqlite3`.

Scope is always explicit where it matters: `workspace_path`, optional `artifact`, `repo`, `ref`, file paths, and agent id.

## Primary Surfaces

### Library

```ts
import {
  getMemory,
  insertMemory,
  preFlightIntent,
  releaseFileLock,
  markVerified,
  agentSignal,
  reflect,
  wirePiAwarenessHooks,
} from '@octocodeai/octocode-awareness';
```

The exported API is defined in [`src/index.ts`](src/index.ts). It is used directly by [`@octocodeai/pi-extension`](../octocode-pi-extension) and can be used by other host integrations.

### CLI

The CLI is the canonical operational surface. Skills and hooks call it for operations; users and host integrations can call it directly. It takes flags, prints JSON, and hard-errors on unknown flags.

```bash
node dist/bin/awareness.js workspace status --workspace "$PWD"
node dist/bin/awareness.js memory recall --query "current task" --smart
node dist/bin/awareness.js lock acquire --agent-id agent-a --target-file src/foo.ts --rationale "edit foo"
node dist/bin/awareness.js verify mark --agent-id agent-a --all-pending --message "tests passed"
node dist/bin/awareness.js query gotchas --workspace "$PWD"
node dist/bin/awareness.js repo inject --workspace "$PWD" --mode local
```

Run `node dist/bin/awareness.js --help` or `node dist/bin/awareness.js <command> --help` for command-specific flags.

Main command groups:

- `memory record|recall|forget|index`
- `lock acquire|release|wait|prune`
- `verify audit|mark`
- `signal publish|list|reply|ack|resolve|prune`
- `agent register|list`
- `refinement set|get|delete`
- `reflect record|mine-weakness|export-harness`
- `query <view>`, `view`, `repo inject`
- `hooks install|check|remove`
- `hook run <event>`
- `schema list|json-schema|example|validate`
- `workspace status`, `session capture`, `docs staleness`, `maintenance digest|init|self-test`

Legacy flat names such as `get-memory`, `pre-flight-intent`, and `agent-signal` remain aliases. New docs should use the noun/verb form.

Repo context commands are generated projections over the same SQLite store:

- `query <view>` reads smart JSON/table/CSV/Markdown views for agents and scripts.
- `view` writes a static HTML browser view, usually `.octocode/awareness/index.html`.
- `repo inject` writes `.octocode/AGENTS.md`, `.octocode/MEMORY.md`, `.octocode/GOTCHAS.md`, `.octocode/LEARN.md`, CSV files under `.octocode/awareness/csv/`, and compact references under `.octocode/references/`.

`repo inject` reports gitignore/share-policy warnings but never edits `.gitignore`; each repo owner decides whether `.octocode` stays local or becomes shared.

### Skills

The package source of truth is [`skills/`](skills/):

- `octocode-awareness` is the primary skill and owns operational scripts.
- `octocode-reflection` is a compatibility stub that routes old reflection requests back to Awareness.
- `octocode-agent-communication` is a compatibility stub that routes old messaging requests back to Awareness.

Only `octocode-awareness/scripts/` contains generated runtime scripts. The stubs intentionally have no scripts and no operational reference docs.

Install the primary skill with the Octocode CLI:

```bash
npx octocode skill --name octocode-awareness
```

## Hooks

Shell-hook hosts use the generated skill CLI:

```bash
node skills/octocode-awareness/scripts/awareness.mjs hooks install --host codex --project-dir . --dry-run
node skills/octocode-awareness/scripts/awareness.mjs hooks check --host codex --project-dir .
node skills/octocode-awareness/scripts/awareness.mjs hooks remove --host codex --project-dir .
```

Supported hook install targets are Claude-style settings, Codex hooks, and Cursor hooks. `scripts/install-hooks.mjs` remains only as a compatibility wrapper.

Pi does not use shell hooks. It imports this package and wires `wirePiAwarenessHooks(pi)` in-process.

## Build Outputs

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness verify
```

Build does four things:

1. Compiles the library and bins into `dist/`.
2. Generates `awareness.mjs`, `schema.mjs`, `hook-runner.mjs`, and hook helpers into `packages/octocode-awareness/skills/octocode-awareness/scripts/`.
3. Removes operational scripts from the compatibility stubs.
4. Mirrors the package-owned skills into repo-root `skills/` and local `.agents/skills/`.

The Pi extension owns its own generated copy under `packages/octocode-pi-extension/skills/` through `yarn workspace @octocodeai/pi-extension build:skills`.

Edit package-owned skill sources only under `packages/octocode-awareness/skills/`. Root `skills/` and `.agents/skills/` are generated mirrors.

## Package Boundaries

This package owns:

- memory, lock, signal, refinement, verification, reflection, session, and hook runtime code,
- the `awareness` CLI and schemas,
- awareness skill sources and generated primary skill scripts,
- host hook install/check/remove logic,
- the Pi awareness bridge API.

Other packages own:

- `packages/octocode`: installing skills and other Octocode CLI features,
- `packages/octocode-pi-extension`: Pi system prompt, Pi tool registration, Pi packaging, and bundled skill sync,
- `@octocodeai/config`: Octocode home/env/config loading,
- `octocode-tools-core` and related packages: code research tools and MCP execution logic.

Keep new awareness behavior here. Keep host-specific packaging in the host package.

## More Detail

- [`docs/SKILLS.md`](docs/SKILLS.md): skill routing, CLI groups, lifecycle diagrams.
- [`docs/HARNESS.md`](docs/HARNESS.md): DB entities, hook lifecycle, staleness and harness flow.
- [`skills/octocode-awareness/SKILL.md`](skills/octocode-awareness/SKILL.md): agent-facing operating loop.
