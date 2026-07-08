# @octocodeai/octocode-awareness

Octocode Awareness is a local coordination and memory layer for AI coding agents.

If Cursor, Claude Code, Codex, Pi, and custom agents are all working on the same machine, they should not have to rediscover the same repo facts, overwrite the same files, or lose a handoff when a session ends. Awareness gives them one shared SQLite store under the global Octocode home, plus a CLI, Agent Skill, hooks, and library API for using it.

In practice, agents use it to:

- lock files before editing so concurrent work is visible,
- recall durable lessons and repo gotchas before planning,
- send typed messages and handoffs to other agents,
- keep verification obligations explicit,
- reflect on outcomes so future sessions improve,
- generate workspace `.octocode/` repo context that humans and LLMs can inspect.

The package is intentionally local-first: no server, no network, no daemon, and zero npm runtime dependencies for the library. SQLite is the broker. The CLI is the control plane. Hooks automate the easy-to-forget edges.

## Two `.octocode` Locations

Awareness uses two different `.octocode` locations. They have different jobs:

| Location | Scope | What lives there |
|---|---|---|
| Global Octocode home: `~/.octocode/` on macOS | Machine/user-wide | Config and durable data shared across workspaces. Awareness stores its canonical DB at `~/.octocode/memory/awareness.sqlite3` on macOS by default. Override the DB directory with `OCTOCODE_MEMORY_HOME`; other Octocode packages use `OCTOCODE_HOME` for broader config/env/data. |
| Workspace projection: `<repo>/.octocode/` | One repo/workspace | Generated LLM Wiki and repo-context files from `repo inject`: `AGENTS.md`, `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, CSV, HTML, manifest, and compact references. These files describe the repo; they are not the canonical DB. |

Rule of thumb: **global home stores source data; repo `.octocode/` publishes a repo-specific view of that data.** If a repo projection is stale, update the DB/source facts and regenerate it with `repo inject`.

It does not own the Octocode research tools, the `octocode` installer CLI, Pi's system prompt, or config/env loading. Those live in their own packages. This package only provides the shared awareness runtime and the skill/CLI surfaces that operate it.

## Why It Exists

Modern agent workflows are multi-session and increasingly multi-agent. The failure mode is not that agents cannot write code; it is that they lack shared situational awareness:

- one agent edits a file another agent is already changing,
- a lesson learned in yesterday's session disappears from today's prompt,
- a user asks for verification but the check is forgotten at stop time,
- a handoff lives only in chat history,
- self-improvement advice gets mixed with unreviewed automatic patching.

Awareness makes those concerns explicit data. It does not try to be the agent. It gives agents a small shared operating loop: attend, claim, work, verify, reflect, project, and hand off.

## Design Thinking

| Principle | What it means in this package |
|---|---|
| Local-first | A SQLite file under the global Octocode home is enough. No service has to be online for agents to coordinate. |
| Database canonical | Memories, locks, tasks, signals, refinements, and harness events live in SQLite; generated files are projections. |
| Agent-neutral | Codex, Claude Code, Cursor, Pi, and custom hosts use the same CLI/library semantics. |
| Verification-aware | A released lock is not automatically success; unverified work stays `PENDING`. |
| Human-reviewed self-improvement | Reflection can propose guidance, but it does not silently rewrite the harness. |
| Inspectable | `query`, workspace `.octocode/` projections, CSV, HTML, and docs make state readable without spelunking the DB. |

## Quick Start

### Easiest Installation

Tell your agent:

```bash
npx @octocodeai/octocode-awareness
```

That prints the command map and the next setup step. The important follow-up is installing the bundled Agent Skill so the agent knows the awareness operating loop:

```bash
npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common
```

If a registry/marketplace install is desired instead, use:

```bash
npx octocode skill --name octocode-awareness
```

Supported agents: **Codex**, **Claude Code**, **Cursor**, and **Pi**. Custom hosts can use the same CLI or import the library API.

How the pieces fit:

| Piece | Role |
|---|---|
| `npx @octocodeai/octocode-awareness` | Discovers the CLI and runs awareness operations without a global install. |
| `npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common` | Installs the bundled Agent Skill so agents know when and how to use the CLI. |
| Hooks | Optional lifecycle automation for Codex, Claude Code, and Cursor: pre-edit locks, post-edit pending verification, stop-time verify gate, smart briefing, and session capture. |
| Pi bridge | In-process lifecycle integration through `wirePiAwarenessHooks(pi)`, no shell hooks needed. |

### Manual Smoke Setup

```bash
# Initialize and smoke-test the local store
npx @octocodeai/octocode-awareness maintenance init --compact
npx @octocodeai/octocode-awareness maintenance self-test --compact

# See current workspace state
npx @octocodeai/octocode-awareness workspace status --workspace "$PWD" --compact

# Install the bundled Agent Skill through the Octocode CLI
npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common

# Preview hooks before writing host config
npx @octocodeai/octocode-awareness hooks install --host codex --project-dir . --dry-run --compact
```

For the feature-by-feature docs map, start at [`docs/README.md`](docs/README.md). For the product/user guide, start at [`docs/SKILLS.md`](docs/SKILLS.md).

## Cross-Agent Coordination

All agents share the same store, scoped by `workspace_path` so projects stay isolated:

| Agent | Integration |
|---|---|
| Claude Code | Skill hooks via `.claude/settings.json`; `SKILL.md` frontmatter runs pre-edit, post-edit, stop-verify, session-end, and smart-briefing hooks automatically |
| Codex | Hook config via `.codex/hooks.json` or inline `[hooks]`; no `SKILL.md` frontmatter execution |
| Cursor | Hook config via `.cursor/hooks.json`; no `SKILL.md` frontmatter execution |
| Pi | In-process via `wirePiAwarenessHooks(pi)` — no shell hooks needed |
| Custom / library | Import `@octocodeai/octocode-awareness` directly and call runtime functions |

The SQLite DB is the broker — no server, no network, local-first. One agent locks a file; another sees the lock and waits. One agent records a lesson; the next session recalls it regardless of which tool ran last. Signals let agents post typed messages (claims, handoffs, questions, decisions) that persist until the recipient acts.

## What It Manages

One local SQLite store tracks workspace state across agents and sessions:

- memories and durable lessons,
- file locks and edit tasks,
- verification gates,
- agent registry and signals,
- refinements and handoffs,
- reflection records and harness proposals,
- hook/session audit events.

Default store: `~/.octocode/memory/awareness.sqlite3` under the global Octocode home. Override the awareness DB directory with `OCTOCODE_MEMORY_HOME`. The DB runs in WAL mode so concurrent agents can read and write safely. Set `OCTOCODE_AGENT_ID` to give a stable identity to agents whose host does not provide one.

Scope is always explicit where it matters: `workspace_path`, optional `artifact`, `repo`, `ref`, file paths, and agent id.

## Full Awareness Flow

Awareness is one feedback loop over one local store in the global Octocode home. The skill tells agents when to call it, hooks automate critical lifecycle edges, the CLI is the stable control plane, and generated workspace `.octocode/` files make selected DB state readable as repo context.

```text
ATTEND -> CLAIM -> WORK -> VERIFY -> REFLECT -> PROJECT -> HAND OFF
```

1. **Attend**: `workspace status` is the operational first command in a repo. Then `memory recall`, `refinement get`, and `signal list` surface prior lessons, handoffs, and messages before an agent plans.
2. **Claim**: `lock acquire` creates a task and file locks before writes. `lock wait` and signals handle conflicts instead of racing another agent.
3. **Work**: agents edit under the claim. Hooks can run `pre-edit` and `post-edit` automatically; manual CLI calls use the same store and semantics.
4. **Verify**: `verify mark` records the check that actually ran. `verify audit` and stop hooks keep pending verification visible.
5. **Reflect**: `reflect record` writes reusable lessons, failure signatures, optional repo-fix refinements, and harness log events. `reflect mine-weakness` clusters repeated failure signatures. `reflect export-harness` previews guidance candidates; a human still decides what to merge.
6. **Project**: `query <view>` reads normalized JSON/table/CSV/Markdown/HTML views. `repo inject` refreshes the optional workspace `.octocode/` LLM Wiki projections: generated AGENTS, memory, gotcha, learning, CSV, HTML, manifest, and reference files.
7. **Hand off**: `signal publish|reply|ack|resolve`, `refinement set|get`, and `session capture` preserve active work for the next agent or session.

The generated LLM Wiki is deliberately a projection, not the source of truth. The SQLite DB in the global Octocode home remains canonical; workspace `.octocode/` files are regenerated from it and should be treated as inspectable leads that still need live code verification.

## Self-Harness (Self-Improvement Loop)

The harness turns session outcomes into future behavior, under human oversight:

```text
reflect record  →  reflect mine-weakness  →  reflect export-harness
                                                      ↓
                                          human reviews candidates
                                                      ↓
                                        edits applied to skill/AGENTS.md
                                                      ↓
                                               loop ──┘
```

- `reflect record` stores outcome, lesson, optional failure signature, and a staged improvement hint.
- `reflect mine-weakness` clusters repeated `failure_signature` values to surface recurring patterns.
- `reflect export-harness` previews AGENTS.md or skill guidance candidates from top-ranked memories.
- `docs staleness` flags docs that have drifted from source-file activity in `edit_log`.
- `maintenance digest` prunes stale memories, signals, refinements, and pending state.

`export-harness` output is a preview — it is never automatically merged into skill files. A human-reviewed edit applies it. See [`docs/HARNESS.md`](docs/HARNESS.md) for the system flow, [`docs/DB.md`](docs/DB.md) for stored entities, and [`docs/REFLECTION.md`](docs/REFLECTION.md) for the self-improvement loop.

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

Run `npx @octocodeai/octocode-awareness workspace status --workspace "$PWD" --compact` first when joining a repo. Run `schema commands --compact` for the agent command map, then `<command> --help --compact` for token-light usage. Local package development can call `node dist/bin/awareness.js ...` after `yarn workspace @octocodeai/octocode-awareness build`.

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

### LLM Wiki

The LLM Wiki is a generated projection of selected awareness data into the workspace `.octocode/` folder, making repo state readable by agents and humans without querying the DB directly:

- `query <view>` reads smart JSON/table/CSV/Markdown/HTML views for agents, scripts, and humans. Views: `memories`, `gotchas`, `lessons`, `tasks`, `locks`, `agents`, `signals`, `refinements`, `files`, `activity`, `repo-profile`, `all`.
- `repo inject` writes `<repo>/.octocode/AGENTS.md`, `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, CSV files under `awareness/csv/`, compact references under `references/`, `awareness/index.html`, and `awareness/manifest.json`.

SQLite in the global Octocode home is always canonical. Regenerate workspace projections instead of hand-editing them. `repo inject` reports gitignore/share-policy warnings but never edits `.gitignore`; each repo owner decides whether `<repo>/.octocode` stays local or becomes shared.

### Skills

The package source of truth is [`skills/`](skills/). There is one operational skill:

- `octocode-awareness` owns the CLI-first workflow, operational scripts, hooks, memory, signals, reflection, verification, and repo context.

Old prompts that mention `octocode-reflection` or `octocode-agent-communication` should load `octocode-awareness`; those legacy names are no longer shipped as separate skill folders.

Install the primary skill with the Octocode CLI:

```bash
npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common
```

Agents that already know the bundled skills directory should run the `--add --path` form. The path may point to the `octocode-awareness` folder itself or its `SKILL.md`; the CLI copies it into Octocode's canonical skill source and links it into the selected platform destination. A registry fallback is `npx octocode skill --name octocode-awareness`.

## Hooks

Shell-hook hosts can use the package CLI; standalone skill installs can use the generated skill script with the same commands:

```bash
npx @octocodeai/octocode-awareness hooks install --host codex --project-dir . --dry-run --compact
npx @octocodeai/octocode-awareness hooks install --host cursor --global --compact
npx @octocodeai/octocode-awareness hooks check --host codex --project-dir . --strict --compact
npx @octocodeai/octocode-awareness hooks remove --host codex --project-dir . --compact
```

Supported hook install targets are `claude`, `codex`, and `cursor`. Use `--global` to install at user scope instead of project scope. `scripts/install-hooks.mjs` remains only as a compatibility wrapper.

| Hook event | Script | What it does |
|---|---|---|
| `UserPromptSubmit` | `notify-deliver.sh` | Smart briefing — registers/touches the agent and injects unread signals and context before each prompt |
| `PreToolUse` (write) | `pre-edit.sh` | Claims file locks via `lock acquire` |
| `PreToolUse` (write) | `harness-guard.sh` | Blocks skill self-edits unless `OCTOCODE_ALLOW_HARNESS_APPLY=1` and on a non-main branch |
| `PostToolUse` (write) | `post-edit.sh` | Releases locks as `PENDING` verification |
| `Stop` / `SubagentStop` | `stop-verify.sh` | Audits unverified work and blocks exit if pending tasks remain (opt-out: `OCTOCODE_NO_VERIFY_GATE=1`) |
| `SessionEnd` / `PreCompact` | `session-end.sh` | Captures session handoff refinement from lock and git state |

Pi does not use shell hooks. It imports this package and wires `wirePiAwarenessHooks(pi)` in-process.

## Config & DB

| Setting | Default | Override |
|---|---|---|
| Global Octocode home | `~/.octocode/` on macOS; platform default elsewhere | `OCTOCODE_HOME` for broader Octocode config/env/data |
| Awareness DB path | `~/.octocode/memory/awareness.sqlite3` on macOS; platform memory default elsewhere | `OCTOCODE_MEMORY_HOME` (directory containing `awareness.sqlite3`) |
| Workspace projection | `<repo>/.octocode/` | `repo inject --out <dir>` |
| Agent identity | Host-provided or auto-generated | `OCTOCODE_AGENT_ID` |
| Lock TTL | 10 minutes | — |
| Verify gate | enabled | `OCTOCODE_NO_VERIFY_GATE=1` to disable |
| Harness apply gate | blocked | `OCTOCODE_ALLOW_HARNESS_APPLY=1` + non-main branch |

The DB runs in WAL mode. Concurrent agents read and write safely without file-level locking. `maintenance init` creates the schema on first use; `maintenance self-test` smoke-tests DB operations and is safe to run at any time.

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

- [`docs/README.md`](docs/README.md): feature coverage matrix and reading paths.
- [`docs/SKILLS.md`](docs/SKILLS.md): user guide, skill routing, CLI groups, and common recipes.
- [`docs/HARNESS.md`](docs/HARNESS.md): top-level harness flow and documentation map.
- [`docs/DB.md`](docs/DB.md): stored entities, relationships, query views, indexes, and SQL patterns.
- [`docs/LOCKS.md`](docs/LOCKS.md): file lock lifecycle, task states, verification, TTL, and conflicts.
- [`docs/REFLECTION.md`](docs/REFLECTION.md): reflection records, weakness mining, harness candidates, and review boundaries.
- [`docs/WIKI.md`](docs/WIKI.md): workspace `.octocode/` LLM Wiki projections, generated files, query views, and share policy.
- [`docs/HOOKS.md`](docs/HOOKS.md): host hooks, Pi bridge, smart briefing, harness guard, and env controls.
- [`skills/octocode-awareness/SKILL.md`](skills/octocode-awareness/SKILL.md): agent-facing operating loop.
- [`skills/octocode-awareness/references/full-flow.md`](skills/octocode-awareness/references/full-flow.md): technical reference for CLI, skill, hooks, locks, LLM Wiki projections, reflection, and handoffs.
