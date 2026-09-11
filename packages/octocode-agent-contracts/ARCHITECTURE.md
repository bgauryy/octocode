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
- `physiology.ts` owns strict Zod runtime observation schemas and their inferred
  types, published at `@octocodeai/agent-contracts/physiology`. The discriminated
  native/Pi variants validate bounded counters, session identity, timestamp order,
  and context arithmetic. Unknown fields are rejected; absent sensors remain
  absent. Awareness owns advisory thresholds and Pi owns its measurements and
  execution controls. Zod is an external runtime dependency of this package;
  consumers that bundle dependencies retain their existing packaging policy.
- `mcp-discovery.ts` and `mcp-state.ts` own persistent MCP discovery and override
  contracts.
- `agent-skills.ts` owns Agent Skill discovery and metadata contracts.
- `prompts/` owns shared system, plan, and subagent prompt fragments.
  Awareness owns `EXTERNAL_AGENT_AWARENESS_PROMPT` and its Pi alias; those are
  imported separately by Pi, once per composed system prompt. The complete
  Awareness guide and feature catalog remain on demand. See the
  [Awareness API reference](../octocode-awareness/docs/API.md).
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
  relations at the generation-versioned default
  `$OCTOCODE_HOME/awareness/awareness-v4.sqlite3` or the explicitly selected
  `<workspace>/.octocode/awareness-v4.sqlite3`.
- Fail closed on foreign database identities and unexpected relations before
  writing schema. Shared helpers must not relabel or migrate another owner's
  database.

MCP discovery and configuration-file admission are shared through the published
`agent-skills` subpath. Pi's discovery adapter supplies `extensionWorkspaceRoot`
through the explicit `workspaceRoot` policy; other hosts default to the Agent
workspace root. Vendor source enumeration, JSON/TOML normalization, provenance,
collision naming, and file admission remain canonical here. Foreign definitions
stay disabled until a host applies explicit enablement and workspace trust.
Active Pi configuration reads use the same regular-file, no-symlink, 1 MiB guard.
