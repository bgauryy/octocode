# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

Octocode Awareness gives coding agents a local ledger for self-observation and shared work. It combines attributed measurements and feedback, shared Work, decision-changing Messages, verified Memory, and recoverable LocalGit evidence around one bounded Context read. SQLite is canonical. Source files and observed checks remain the authority for code and verification.

The package requires Node.js `^24.15.0`. It runs without a server or daemon.

## Routine surface

The routine CLI has five concepts:

| Concept | Operations |
|---|---|
| Context | `context orient`, `context observe`, `context feedback` |
| Work | `work create`, `work list`, `work show`, `work claim`, `work update`, `work depend`, `work protect`, `work verify` |
| Message | `message list`, `message send`, `message reply`, `message resolve` |
| Memory | `memory recall`, `memory record`, `memory set`, `memory get`, `memory revalidate` |
| History | `history status`, `history timeline`, `history read`, `history restore`, `history experience` |

Start with one orientation:

```bash
npx @octocodeai/octocode-awareness context orient \
    --workspace "$PWD" \
    --agent-id "awareness:session-1" \
    --session-id "session-1" \
    --compact
```

Reuse the returned revision while the shared scope is unchanged. Follow executable `next` calls with the same database, workspace, and identity bindings. Discover exact fields from the live contract:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command work verify --compact
```

Unknown operation names fail. Host lifecycle integration and database migration are separate module subpaths, not additional model commands.

Any agent can use the CLI or imported client to submit measurements with `context observe`, read assessed state and advice with `context orient`, and report its response with `context feedback`. This also works for solo agents. The host owns measurement and execution; Awareness records evidence and advises. Missing measurements remain unknown, and an action report alone does not prove improvement.

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

The package root exposes exactly eight runtime exports:

- `AWARENESS_AGENT_INSTRUCTION_SECTIONS`
- `AWARENESS_CONCEPTS`
- `AWARENESS_MESSAGE_PARAMETER_GUIDANCE`
- `ROUTINE_AWARENESS_OPERATIONS`
- `createAwarenessClient`
- `getAwarenessAgentInstructions`
- `getAwarenessOperationDescriptor`
- `listAwarenessOperationDescriptors`

Import `getAwarenessAgentInstructions` to compose a host prompt from canonical guidance, or run `instructions` through the CLI. `AWARENESS_AGENT_INSTRUCTION_SECTIONS` lists the available sections; `AWARENESS_MESSAGE_PARAMETER_GUIDANCE` supplies the concise Message field contract for host tool prompts. See the [instruction API](docs/API.md#observations-and-feedback) for a selective import example.

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

LocalGit stores optional recoverable evidence under the workspace `.octocode/.localGit` boundary. File capture is host-owned. Routine agents inspect file evidence with `history status`, `history timeline`, and `history read`; they do not create file captures or checkpoints. Agents can also record and seal non-file investigation traces with `history experience`, and preserve keyed lessons, rationale, and typed anchors with `memory set`. See [experience and memory](docs/EXPERIENCE_MEMORY.md), [storage scopes](docs/STORAGE_SCOPES.md), [database ownership](docs/DB.md), and [LocalGit history](docs/LOCAL_HISTORY.md).

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

Builds emit stable, self-contained runtime entries. Lazy source modules are bundled into each entry, so rebuilding cannot remove a hashed executor file that a long-lived host has not imported yet.
