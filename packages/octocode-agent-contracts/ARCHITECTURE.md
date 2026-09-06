# Shared contracts architecture

`@octocodeai/agent-contracts` owns small, host-independent contracts reused by
native, Pi, and Awareness. It is not an agent runtime or a UI composition root.

## Ownership

- `paths.ts` owns canonical Octocode home and agent-state paths by composing
  `@octocodeai/config`.
- `db.ts`, `schema.ts`, `sqlite.ts`, and `sqlite-version.ts` own Agent control
  database connections, identity, control tables, and version checks. They do
  not own Awareness relations or Rust runtime durability.
- `entities.ts`, `permissions.ts`, and `protocols.ts` own cross-host data and
  permission shapes.
- `mcp-discovery.ts` and `mcp-state.ts` own persistent MCP discovery and override
  contracts.
- `agent-skills.ts` owns Agent Skill discovery and metadata contracts.
- `prompts/` owns shared system, plan, and subagent prompt fragments.
- `embed.ts` owns the narrow embedding boundary used by shared discovery and
  memory features.

## Shared layers

| Layer | Modules | Consumers |
|---|---|---|
| Paths and configuration | `paths.ts` and `@octocodeai/config` | Native, Pi, and Awareness composition roots |
| Agent SQLite control data | `db.ts`, `schema.ts`, `sqlite.ts`, `sqlite-version.ts` | Shared discovery and Agent control-state adapters |
| Cross-host contracts | `entities.ts`, `permissions.ts`, `protocols.ts` | Host adapters and coordination packages |
| Discovery and prompts | `mcp-discovery.ts`, `mcp-state.ts`, `agent-skills.ts`, `prompts/` | Native and Pi projections |

## Dependency rules

- Do not add runtime lifecycle, policy evaluation, terminal rendering, Pi
  compatibility, or Awareness domain workflows.
- Do not duplicate environment parsing or Octocode home resolution; use
  `@octocodeai/config`.
- Export deliberate public subpaths. Internal consumers import the owning
  subpath instead of the aggregate package root.
- Keep `$OCTOCODE_HOME/agent/agent.sqlite3` control tables separate from the
  Rust runtime store at `$OCTOCODE_HOME/agent/core.sqlite3` and Awareness domain
  relations at `<workspace>/.octocode/awareness.sqlite3` or the explicitly
  selected `$OCTOCODE_HOME/awareness/awareness.sqlite3`.
- Fail closed on foreign database identities and unexpected relations before
  writing schema. Shared helpers must not relabel or migrate another owner's
  database.

Pi still contains a compatibility MCP discovery implementation under
`packages/octocode-pi-extension/src/tools/`. Treat it as convergence debt; new
cross-host discovery contracts belong here, and host packages should project
them rather than add another authority. Consumers use the published owning
subpath, including the discovery surface exposed through `agent-skills`; don't
invent an unexported subpath.

Program-level completion gates are in
[`DESIGN/LEFTOVERS.md`](../../DESIGN/LEFTOVERS.md).
