# Agent and Awareness storage

Status: Accepted

Agent runtime state and Awareness coordination state use different SQLite
stores. A process must never initialize one owner's schema in the other owner's
database.

```text
$OCTOCODE_HOME/
├── agent/                              Agent-owned
│   ├── agent.sqlite3                   control and discovery indexes
│   ├── core.sqlite3                    Rust runtime durability
│   ├── sessions/                       per-session Agent artifacts
│   ├── workspaces/                     workspace-keyed Agent configuration
│   ├── skills/
│   └── mcp/
├── awareness/                          default global Awareness scope
│   └── awareness.sqlite3
└── <other CLI- or MCP-owned data>

<workspace>/.octocode/
├── awareness.sqlite3                  explicit repository-scoped store
├── awareness.json                     workspace Awareness policy
├── <Awareness exports and plan docs>
└── <other CLI- or MCP-owned data>
```

`$OCTOCODE_HOME` is normally `~/.octocode`. Resolve it through
`@octocodeai/config`; don't duplicate home-directory logic.

## Ownership matrix

| Owner | Canonical path | Data |
|---|---|---|
| Agent control | `$OCTOCODE_HOME/agent/agent.sqlite3` | Agent settings, discovery/control indexes, and session index data. |
| Agent runtime | `$OCTOCODE_HOME/agent/core.sqlite3` | Sessions and events, effects, lifecycle records, automation leases, worker communication, dependency-work ledgers, revisions, and fencing state. |
| Agent artifacts | `$OCTOCODE_HOME/agent/sessions/` and other directories under `$OCTOCODE_HOME/agent/` | Session artifacts and encoded file-fallback records (`.json`, `.bak`, `.head`, `.segments/`), plus checkpoints, logs, worker handback, browser, media, and other Agent-owned files. |
| Awareness repository | `<workspace>/.octocode/awareness.sqlite3` | Explicit repository-scoped Awareness plans, tasks, claims, work presence, locks, verification, agents, signals, memory, and projections for that workspace. |
| Awareness policy | `<workspace>/.octocode/awareness.json` | Optional workspace-selected Awareness scope and hook policy; not runtime or coordination data. |
| Awareness global | `$OCTOCODE_HOME/awareness/awareness.sqlite3` | Default Awareness store for all selected workspaces; rows remain workspace-scoped where the entity contract requires it. |
| Other CLI/MCP owners | Their documented paths, including other files under `<workspace>/.octocode/` | Research indexes, caches, exports, and service-specific state. These aren't Agent or Awareness databases. |

Authoritative worker lifecycle and runtime durability are Agent-owned.
Awareness preserves recognized predecessor projections only long enough for an
explicit copy-on-write migration. It does not advertise or create a worker
lifecycle entity.

## Scope and overrides

Awareness defaults to the global store under `$OCTOCODE_HOME`:

```text
$OCTOCODE_HOME/awareness/awareness.sqlite3
```

Use `--db-scope repo` when collaborators deliberately need repository isolation.
It resolves to:

```text
<workspace>/.octocode/awareness.sqlite3
```

Use `--db <absolute-path>` for an isolated test, recovery operation, or managed
deployment. The explicit path applies to that invocation and still must identify
an Awareness database. `OCTOCODE_AGENT_DB_PATH` changes the Agent control store;
it must not redirect Awareness.

Changing scope changes the physical Awareness database. Agents that intend to
coordinate must select the same workspace and scope. `--db <absolute-path>` has
highest precedence for one call. Scope changes never merge existing databases;
the global Awareness store doesn't make Agent runtime state global or merge it
with Agent databases.

For signals, pass `--workspace` when a thread must use workspace isolation. Omitting
the workspace preserves compatibility for unscoped IDs; process cwd supplies
repository context and does not promise that signal isolation.

## Repository artifacts

Awareness SQLite is canonical for live coordination. Authored plan documents and
explicit `query` exports under `.octocode/` are files, not a second source of
live state. Don't hand-edit exports or infer current claims, locks, or
verification from them.

Other tools can own databases in the same `.octocode/` directory. Identify a
file by its documented name and database identity, not by its parent directory.

## Canonical store requirement

The CLI accepts only the exact current canonical Awareness schema. Older,
incomplete, drifted, and mixed Agent/Awareness databases are rejected without
rewriting them. Select a fresh Awareness store when the selected database does
not satisfy the current schema fingerprint.

`database consolidate` copies an exact current canonical store into a new file.
It does not migrate or repair any other schema. A source with local-history rows
is also rejected because its Git object store is bound to the database path.

## Operational checks

Use owner-specific commands rather than editing SQLite directly:

```bash
npx @octocodeai/octocode-awareness maintenance init --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact
```

For explicit repository scope:

```bash
npx @octocodeai/octocode-awareness maintenance init --db-scope repo --compact
npx @octocodeai/octocode-awareness status --workspace "$PWD" --db-scope repo --compact
```

For an isolated check:

```bash
npx @octocodeai/octocode-awareness maintenance init --db /absolute/path/awareness.sqlite3 --compact
```

The native Agent and Rust services live in the sibling `octocode-agent`
repository. Inspect host integration through the [native Agent README](https://github.com/bgauryy/octocode-agent/tree/main/packages/octocode-agent/README.md)
and [Rust core README](https://github.com/bgauryy/octocode-agent/tree/main/packages/octocode-agent-core-rust/README.md).
See [DB.md](DB.md) for schema ownership and fail-closed identity checks.
