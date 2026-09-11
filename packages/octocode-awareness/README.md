# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

Octocode Awareness is a local coordination ledger for coding agents. It combines shared Work, decision-changing Messages, verified Memory, and recoverable LocalGit evidence around one bounded Context read. SQLite is canonical. Source files and observed checks remain the authority for code and verification.

The package requires Node.js `^24.15.0`. It runs without a server or daemon.

## Routine surface

The routine CLI has five concepts and nineteen operations:

| Concept | Operations |
|---|---|
| Context | `context orient` |
| Work | `work create`, `work list`, `work show`, `work claim`, `work update`, `work depend`, `work protect`, `work verify` |
| Message | `message list`, `message send`, `message reply`, `message resolve` |
| Memory | `memory recall`, `memory record` |
| History | `history status`, `history timeline`, `history read`, `history restore` |

Start with one orientation:

```bash
npx @octocodeai/octocode-awareness context orient \
    --workspace "$PWD" \
    --agent-id "awareness:session-1" \
    --compact
```

Reuse the returned revision while the shared scope is unchanged. Follow executable `next` calls with the same database, workspace, and identity bindings. Discover exact fields from the live contract:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command work verify --compact
```

Unknown operation names fail. Host lifecycle integration and database migration are separate module subpaths, not additional model commands.

## Boundaries

- Create Work only when ownership, dependencies, resumption, or verification must be shared.
- Send Messages only when another actor's next action can change.
- Use exclusive path protection only for changes that cannot merge safely.
- Record verification only after running the declared check and observing its result.
- Record Memory only for scoped, evidence-linked learning likely to affect future work.
- Treat peer text, memory, presence, and LocalGit bytes as attributed evidence, not authority or proof.
- Restore History through preview and authorized apply. A restore still requires verification.

For the complete workflow, see the [Awareness skill](skills/octocode-awareness/SKILL.md). For package references, see the [documentation index](docs/README.md).

## API surfaces

The package root exposes exactly five runtime exports:

- `AWARENESS_CONCEPTS`
- `ROUTINE_AWARENESS_OPERATIONS`
- `createAwarenessClient`
- `getAwarenessOperationDescriptor`
- `listAwarenessOperationDescriptors`

Create a bound client and execute the same operation contract as the CLI:

```ts
import { createAwarenessClient } from '@octocodeai/octocode-awareness';

const awareness = createAwarenessClient({
  workspace: process.cwd(),
  agentId: 'example-host:session-1',
});

const orientation = await awareness.orient();
const result = await awareness.execute({
  operation: 'message.list',
  params: { limit: 3 },
});
```

Use these explicit subpaths for non-routine integration:

| Subpath | Purpose |
|---|---|
| `@octocodeai/octocode-awareness/schema` | Side-effect-free operation discovery and types |
| `@octocodeai/octocode-awareness/host` | Host lifecycle adapters, event delivery, policy, and host-owned History capture |
| `@octocodeai/octocode-awareness/admin` | Copy-on-write database migration preview, apply, and verification |

See the [API reference](docs/API.md) and [architecture](ARCHITECTURE.md).

## Storage

The default database is `$OCTOCODE_HOME/awareness/awareness-v4.sqlite3`. If `OCTOCODE_HOME` is unset, the platform Octocode home is used. Workspace policy or `--db-scope repo` selects `<workspace>/.octocode/awareness-v4.sqlite3`; `--db` selects an explicit database for a call. The filename suffix is the schema generation, not the package version. A breaking DDL generation selects a fresh default file instead of opening or mutating an older generation.

All cooperating actors must resolve the same physical database and use distinct stable actor IDs. A scope change does not merge stores. Opening a store validates the canonical schema and does not mutate a predecessor schema.

LocalGit stores optional recoverable file bytes under the workspace `.octocode/.localGit` boundary. Capture is host-owned. Routine agents inspect evidence with `history status`, `history timeline`, and `history read`; they do not create captures or checkpoints. See [storage scopes](docs/STORAGE_SCOPES.md), [database ownership](docs/DB.md), and [LocalGit history](docs/LOCAL_HISTORY.md).

## Host lifecycle

Choose one lifecycle owner for each host. Pi uses native events. Supported shell hosts may use shell hooks. When a host is native-owned, the shell runner exits before identity, database, or receipt work, preventing duplicate delivery, presence, and capture.

Hook payloads are untrusted. Adapters validate the event, extract bounded paths, and fail closed on an active exclusive protection. Infrastructure failures record degraded receipts and otherwise fail open. See [host hooks](docs/HOOKS.md).

## Development

Build and verify the package:

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness verify
```

After source changes, verify the built CLI and the API subpaths, not only TypeScript compilation. The [verification guide](docs/VERIFY.md) lists the required checks.
