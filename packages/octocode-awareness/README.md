# Octocode Awareness

Shared plans, file awareness, verification, memory, bookkeeping, housekeeping, and
handoffs for coding agents. It is local-first: one SQLite database, one CLI/runtime,
optional host hooks, and no server or daemon.

```text
agents -> CLI / hooks / Pi bridge -> ~/.octocode/memory/awareness.sqlite3
                                      |-> live query/workboard
                                      `-> optional .octocode/ projections
```

Requires Node.js 22 or newer. The runtime library has zero npm runtime dependencies.

## Install

Install the persistent CLI, initialize its local store, and install the Awareness
skill from that exact package copy:

```bash
npm install --global @octocodeai/octocode-awareness
octocode-awareness maintenance init --compact
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

The bundled `octocode-skills` skill is optional and intended for agents that install,
review, or improve skills:

```bash
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-skills" \
  --platform common --force
```

Use a host-specific platform (`claude`, `codex`, `cursor`, `pi`, or `all`) when
needed. Do not substitute an unrelated registry copy for these package paths.

Optional host hooks:

```bash
octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --dry-run --compact
octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --compact
octocode-awareness hooks check --host <claude|codex|cursor> --project-dir . --strict --compact
```

Pi uses `wirePiAwarenessHooks(pi)` or `@octocodeai/pi-extension`, not shell-hook
installation.

## Start

The examples below use the globally installed `octocode-awareness` binary. For a
one-off command, replace it with `npx @octocodeai/octocode-awareness`. In this
monorepo after build, use `node packages/octocode-awareness/dist/bin/awareness.js`.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
```

Follow `attend.next`. Use `schema commands --compact` only when the command is
unknown, `<command> --help` only for its flags, and `schema json-schema <name>` only
for a machine contract.

## Collaboration Model

```text
Plan -> Task -> TaskRun -> RunFile (mandatory advisory presence)
                        `-> Lock (optional exclusive protection)

Standalone WORK -> TaskRun(origin=WORK) -> same file/lock model
Hook fallback -> TaskRun(origin=HOOK) -> isolated verification item
```

- A plan owns objective, lead agent, members, lifecycle, and documents under
  `.octocode/plan/<timestamp-name>/`.
- A task is durable selectable work with reasoning, acceptance, paths, priority, and
  dependencies. It is the only queue; plan Markdown never copies live task status.
- A run is one execution/verification attempt.
- `run_files` records which paths are actively under work. Multiple agents may work
  on the same ordinary file and can see each other's task/reason.
- A lock is exclusive and reserved for sensitive work. It cannot be acquired while
  another agent has live presence on that path.
- `edit_log` is completed-edit history. Presence expiry only clears stale
  coordination; it never marks work successful.

## Everyday Flow

Shared plan work:

```bash
octocode-awareness task ready --plan-id <plan> --compact
octocode-awareness task claim --task-id <task> --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks declare each edited file under the claimed run; without hooks:
octocode-awareness work start --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --file src/a.ts --compact
octocode-awareness task submit --task-id <task> --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
# run acceptance checks
octocode-awareness verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "tests passed" --compact
```

Standalone WORK:

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file README.md --rationale "clarify install" --test-plan "review diff" --compact
# edit; add/heartbeat files with work start --run-id or work touch
octocode-awareness work end --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> --compact
octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> \
  --message "diff reviewed" --compact
```

For sensitive work, add `--exclusive` to `work start`, or use task/run-aware
`lock acquire`. `lock wait` observes until clear but does not claim.

Inspect live collaboration:

```bash
octocode-awareness work list --workspace "$PWD" --compact
octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact
octocode-awareness query workboard --workspace "$PWD" --format table --limit 10
```

## Hooks

Hooks automate the same runtime operations:

| Edge | Behavior |
|---|---|
| Prompt/session start | Deliver only changed briefing state; do not mark signals read. |
| Before write | Guard harness edits, then declare advisory file work; block only exclusive conflict. |
| After write | Log/heartbeat. Keep task/explicit work active; end automatic fallback as `PENDING`. |
| Stop | Show at most three verification items plus omitted count; block/remind by host capability. |
| End/compact | Deduplicate handoff capture and close stale session coordination. |

Claude can use skill frontmatter while the skill is active. Codex and Cursor require
installed hook config. Always smoke the host because “installed” does not prove the
host executes hooks.

## Compact Context

- Normal edits inject no context.
- Peer context appears only when the peer set changes.
- Briefings are fingerprinted per agent/session/scope.
- `attend --compact` returns an action-oriented packet under the tested byte budget;
  full organ/drive/evidence detail remains available without compact mode.
- `verify audit --compact` keeps three rows per debt class plus totals/omitted count;
  omit compact only when every historical row is required.
- Workboard rows group peers by file and cap detail with omitted counts.
- Query/recall default to lean projections; `docs list --compact` returns name/title
  routing only. Use explicit full/detail flags only when needed.

SQLite records all coordination even when prompts stay silent.

## Storage And Projections

The canonical store is `~/.octocode/memory/awareness.sqlite3`, or
`$OCTOCODE_MEMORY_HOME/awareness.sqlite3`. Rows use `workspace_path` first, then
optional artifact/repo/ref scope.

`<workspace>/.octocode/` is not a second database. `repo inject` creates bounded
Markdown, CSV, HTML, and manifest projections. Plan documents are authored files in
`.octocode/plan/**`; live plan/task state remains in SQLite.

Memories, signals, peers, and projections are leads. Current user instructions,
source, tests, and fresh verification win.

## Documentation

- [How it works](docs/HOW_IT_WORKS.md)
- [Database](docs/DB.md)
- [File work, locks, verification](docs/LOCKS.md)
- [Hooks](docs/HOOKS.md)
- [Compact navigation](docs/MEMORY_NAVIGATION.md)
- [User guide](docs/SKILLS.md)
- [Reflection](docs/REFLECTION.md)
- [Repo context](docs/WIKI.md)

Agent references live in `skills/octocode-awareness/references/` and are discoverable
through `docs list|show`.

## Development

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness verify
```

Canonical sources are `src/**`, `bin/**`, `scripts/schema.mjs`, package docs, and
`skills/octocode-awareness/**`. Build regenerates `dist/**`, compiled skill scripts,
and `.agents/skills/**`; never hand-edit those mirrors.
