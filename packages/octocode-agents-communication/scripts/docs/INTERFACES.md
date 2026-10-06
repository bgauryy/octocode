# CLI and MCP entry points

Use the default npm entry for a host MCP connection. Use `/cli` for standalone operations.
Both routes execute the same Python runtime and SQLite contract.

```mermaid
flowchart LR
  N[npm executable] --> M[MCP stdio]
  N --> C[/cli operations]
  M --> P[Python runtime]
  C --> G[octocode-mcp-cli validation and help]
  G --> P
  P --> S[(Shared SQLite store)]
```

## Discover before operating

```sh
npx -y @octocodeai/octocode-agents-communication --help
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli send_message --help
npx -y @octocodeai/octocode-agents-communication /cli send_message --help --json
npx -y @octocodeai/octocode-agents-communication /cli schema send_message --json
npx -y @octocodeai/octocode-agents-communication /cli schema types --compact --json
```

Command help describes CLI flags, including identity and workspace controls.
`schema` describes the underlying operation input, without CLI transport controls.
The full CLI exposes 50 operations and two record-type discovery routes.
MCP exposes 18 worker tools; profiles select a smaller subset.
Administration, host setup, and worker launch stay in the CLI.

## Supply CLI input

Use typed flags or one JSON object immediately after the command.
For machine output, add `--json`.
Nested objects and arrays of objects use JSON flag values.
Primitive arrays accept repeated flags or one JSON array, including `[]`.
Presence flags take no value; other boolean flags require `true` or `false`. Follow command help.

```sh
npx -y @octocodeai/octocode-agents-communication /cli join \
  --name reviewer --vendor generic --workspace-root /absolute/project \
  --database /absolute/shared/communication.sqlite --json

npx -y @octocodeai/octocode-agents-communication /cli subscribe \
  --topics '["api","review"]' --session EXACT_DB_AGENT_ID \
  --workspace-root /absolute/project --database /absolute/shared/communication.sqlite --json
```

For large documents or shell-sensitive JSON, use `-` to read stdin:

```sh
npx -y @octocodeai/octocode-agents-communication /cli share_document - \
  --session EXACT_DB_AGENT_ID --workspace-root /absolute/project \
  --database /absolute/shared/communication.sqlite --json < /absolute/evidence-input.json
```

The file contains the operation object: `name`, `content`, and `reasoning`, plus any optional fields.
Input is limited to 8 MiB. CLI payloads reach Python through stdin instead of process arguments.
Follow `next.command` with `next.input` unchanged and the same binding until no continuation remains.
Streaming operations emit their own frames; they append no synthetic result after EOF.

## Select the workspace

For CLI calls, `--workspace-root` selects the invocation checkout. `--workspace` is an alias.
For default MCP, use `--workspace` in the server launch arguments.
The `read_document` operation can select a historical origin checkout with `--origin-workspace`.
Its JSON input retains the canonical `workspace` field.
The origin checkout must belong to the same repository scope.

## Own the lifecycle

| Route | Presence and leases on exit |
| --- | --- |
| CLI with an existing `--session` | Remain until expiry or explicit `leave`; the owner must maintain them |
| Default MCP with `--session` | Borrowed identity remains; the owner maintains presence and renewal |
| Default MCP with `--managed --vendor ... --name ...` | Connection maintains presence and live leases, then cleans up on close |
| CLI `run` | Requested worker owns lifecycle and cleans up when it exits |

Do not create a managed MCP connection for each individual CLI operation.
That connection closes its identity and leases after the call.
Reuse one identity for related standalone calls.
See [installation](INSTALLATION.md) for binding and [host setup](HOST_SETUP.md) for delivery ownership.

## Test a local package

From the monorepo root:

```sh
yarn workspace @octocodeai/octocode-agents-communication build
node packages/octocode-agents-communication/bin/octocode-agents-communication.mjs /cli --help
```

For an unpublished archive, use `npx -y --package /absolute/package.tgz octocode-agents-communication /cli --help`.
The archive includes both Node entry modules and the complete Python runtime.
It bundles `octocode-mcp-cli`; consumers need no separate installation of that private workspace package.

## Import the interfaces

```js
import { runCommunicationMcp } from '@octocodeai/octocode-agents-communication';
import { createCommunicationCli, runCommunicationCli } from '@octocodeai/octocode-agents-communication/cli';
```

Imports do not start a server or create storage.
`runCommunicationMcp(argv)` starts the stdio server.
`runCommunicationCli(argv, io?)` runs one CLI invocation and returns an exit code.
`createCommunicationCli()` reads the catalog and returns the shared CLI specification.
Node.js 24.15+ (24.x), Python 3.9+, SQLite 3.42+, and FTS5 are required for the npm route.
