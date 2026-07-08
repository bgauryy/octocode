# @octocodeai/octocode-awareness

Shared awareness, memory, coordination, reflection, and hook runtime for AI coding agents.

This package owns the awareness system. It produces:

- a zero npm runtime dependency TypeScript library,
- the `octocode-awareness` CLI for every awareness operation,
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

## Full Awareness Flow

Awareness is one feedback loop over one local store. The skill tells agents when to call it, hooks automate critical lifecycle edges, the CLI is the stable control plane, and generated `.octocode/` files make selected DB state readable as repo context.

```text
ATTEND -> CLAIM -> WORK -> VERIFY -> REFLECT -> PROJECT -> HAND OFF
```

1. **Attend**: `workspace status`, `memory recall`, `refinement get`, and `signal list` surface active locks, prior lessons, handoffs, and messages before an agent plans.
2. **Claim**: `lock acquire` creates a task and file locks before writes. `lock wait` and signals handle conflicts instead of racing another agent.
3. **Work**: agents edit under the claim. Hooks can run `pre-edit` and `post-edit` automatically; manual CLI calls use the same store and semantics.
4. **Verify**: `verify mark` records the check that actually ran. `verify audit` and stop hooks keep pending verification visible.
5. **Reflect**: `reflect record` writes reusable lessons, failure signatures, optional repo-fix refinements, and harness log events. `reflect mine-weakness` clusters repeated failure signatures. `reflect export-harness` previews guidance candidates; a human still decides what to merge.
6. **Project**: `query <view>` reads normalized JSON/table/CSV/Markdown/HTML views. `repo inject` refreshes the optional `.octocode/` "auto wiki" projections: generated AGENTS, memory, gotcha, learning, CSV, HTML, manifest, and reference files.
7. **Hand off**: `signal publish|reply|ack|resolve`, `refinement set|get`, and `session capture` preserve active work for the next agent or session.

The generated repo context is deliberately a projection, not the source of truth. The SQLite DB remains canonical; `.octocode/` files are regenerated from it and should be treated as inspectable leads that still need live code verification.

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

The CLI is the canonical operational surface. Skills and hooks call it for operations; users and host integrations can call it directly. It takes flags, prints JSON, and hard-errors on unknown flags. The npm package stays scoped as `@octocodeai/octocode-awareness`, publishes publicly, and exposes the `octocode-awareness` bin.

```bash
npx @octocodeai/octocode-awareness workspace status --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness memory recall --query "current task" --smart --compact
npx @octocodeai/octocode-awareness lock acquire --agent-id agent-a --target-file src/foo.ts --rationale "edit foo" --compact
npx @octocodeai/octocode-awareness verify mark --agent-id agent-a --all-pending --message "tests passed" --compact
npx @octocodeai/octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact
npx @octocodeai/octocode-awareness repo inject --workspace "$PWD" --mode local --compact
```

Run `npx @octocodeai/octocode-awareness schema commands --compact` for the agent command map, then `npx @octocodeai/octocode-awareness <command> --help --compact` for token-light usage. Local package development can call `node dist/bin/awareness.js ...` after `yarn workspace @octocodeai/octocode-awareness build`.

Main command groups:

- `memory record|recall|forget`
- `lock acquire|release|wait|prune`
- `verify audit|mark`
- `signal publish|list|reply|ack|resolve|prune`
- `agent register|list`
- `refinement set|get|delete`
- `reflect record|mine-weakness|export-harness`
- `query <view>`, `repo inject`
- `hooks install|check|remove`
- `hook run <event>`
- `schema commands|list|json-schema|example|validate`
- `workspace status`, `session capture`, `docs staleness`, `maintenance digest|init|self-test`

Repo context commands are generated projections over the same SQLite store:

- `query <view>` reads smart JSON/table/CSV/Markdown/HTML views for agents, scripts, and humans.
- `repo inject` writes `.octocode/AGENTS.md`, `.octocode/MEMORY.md`, `.octocode/GOTCHAS.md`, `.octocode/LEARN.md`, CSV files under `.octocode/awareness/csv/`, and compact references under `.octocode/references/`.

`repo inject` reports gitignore/share-policy warnings but never edits `.gitignore`; each repo owner decides whether `.octocode` stays local or becomes shared.

### Skills

The package source of truth is [`skills/`](skills/). There is one operational skill:

- `octocode-awareness` owns the CLI-first workflow, operational scripts, hooks, memory, signals, reflection, verification, and repo context.

Old prompts that mention `octocode-reflection` or `octocode-agent-communication` should load `octocode-awareness`; those legacy names are no longer shipped as separate skill folders.

Install the primary skill with the Octocode CLI:

```bash
npx octocode skill --name octocode-awareness
```

## Hooks

Shell-hook hosts can use the package CLI; standalone skill installs can use the generated skill script with the same commands:

```bash
npx @octocodeai/octocode-awareness hooks install --host codex --project-dir . --dry-run --compact
npx @octocodeai/octocode-awareness hooks check --host codex --project-dir . --compact
npx @octocodeai/octocode-awareness hooks remove --host codex --project-dir . --compact
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
3. Prunes retired legacy skill mirrors.
4. Bundles package-owned skills into `dist/skills/` and mirrors local installs into `.agents/skills/`; repo-root `skills/` is not a mirror target.

The Pi extension owns its own generated copy of `octocode-awareness` under `packages/octocode-pi-extension/skills/` through `yarn workspace @octocodeai/pi-extension build:skills`.

Edit package-owned skill sources only under `packages/octocode-awareness/skills/`. Generated copies live in `dist/skills/` and `.agents/skills/`.

## Package Boundaries

This package owns:

- memory, lock, signal, refinement, verification, reflection, session, and hook runtime code,
- the `octocode-awareness` CLI and schemas,
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
- [`skills/octocode-awareness/references/full-flow.md`](skills/octocode-awareness/references/full-flow.md): technical reference for CLI, skill, hooks, locks, auto wiki projections, reflection, and handoffs.
