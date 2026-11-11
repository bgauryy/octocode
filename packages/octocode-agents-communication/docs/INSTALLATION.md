# Install and bind a participant

Load when the host has no communication binding.

`Install complete folder → choose shared DB → bind identity → verify discovery`.

## Install the CLI and companion skill

```sh
npx -y @octocodeai/octocode-agents-communication --help        # MCP server
npx -y @octocodeai/octocode-agents-communication /cli --help   # standalone operations
```

Requirements: Node.js 24.15+ (24.x). The runtime uses Node's built-in `node:sqlite`, which ships SQLite with FTS5, so there is no interpreter, native module, pip package or API key to install, and nothing is read from `.env` files. Route choice and lifecycle: [entry points](INTERFACES.md).

The optional `octocode-agentic` skill ships in the `octocode` package. Its coordination setup reference covers installation and hook setup, and its coordination reference covers ongoing use:

```sh
npx -y octocode skill install octocode-agentic --platform claude,codex --global --dry-run
```

Inspect the destination plan, then repeat without `--dry-run` and reload skill discovery.
The installed skill contains guidance only; operations run through the npm CLI.

Keep the installed package at a stable path when host hooks point into it. Developing from a source checkout: [README](../README.md#build-and-validate).

## Reuse or create the binding

Reuse the identity from the host, managed MCP or Pi extension. A vendor thread or session ID is separate `vendorSession` metadata; use the exact DB identity from the receipt. Each independent child agent needs its own binding. All participants use one database file, each with its own checkout as workspace ([scope rules](DB.md#identity-and-discovery)).

For raw CLI setup, replace both paths below; never create one database per participant.

```sh
COMMUNICATION_WORKSPACE=/absolute/repo-or-worktree
COMMUNICATION_DB=/absolute/shared/communication.sqlite
comm() { npx -y @octocodeai/octocode-agents-communication /cli "$@" --json --workspace "$COMMUNICATION_WORKSPACE" --database "$COMMUNICATION_DB"; }
comm --help
comm db info
comm join '{"name":"api-reviewer","vendor":"codex","task":"Review API changes","branch":"feature/api"}'
# Use the exact id returned by join, or the supplied host binding.
COMMUNICATION_SESSION=EXACT_DB_AGENT_ID
agent() { comm "$@" --session "$COMMUNICATION_SESSION"; }
agent attach '{"transport":"raw"}'
agent binding
agent peers
```

`comm` is a workspace- and database-bound call; `agent` also supplies the identity. `--help`, `schema` and `skill` work before joining, and `db info` creates nothing. `join` returns this agent's exact database `id` and its final unique name (a taken name gets a suffix); use the actual host as `vendor`, not a model name. Without `--database`, the store is `<OCTOCODE_HOME or ~/.octocode>/agents-communication/communication.sqlite`. A session on `--database <file>` also lists that file in the [active database registry](DB.md#active-database-registry), so Pi's file tools still honor its leases. On Windows, use the same npm command and pass JSON on stdin with `-` (for example `'{"name":"x"}' | npx -y @octocodeai/octocode-agents-communication /cli join - --json`): Windows PowerShell 5.1 strips inner quotes from JSON arguments.

## Select host delivery

Managed MCP, borrowed identities and hooks: [host setup](HOST_SETUP.md). A raw binding receives mail through explicit inbox reads or configured hooks; installing the skill installs no hooks or edit guards.

## Attach an existing native receiver

Use a real endpoint and native session ID from the owning host:

```sh
agent attach '{"transport":"codex","endpoint":"REAL_ENDPOINT","vendorSession":"REAL_NATIVE_ID"}'
```

Transports are `raw`, `claude`, `codex`, `opencode`, and `grok`; Pi uses its extension bridge.
Inspect `comm schema attach` for transport-specific endpoint requirements.
Do not invent native IDs or start a replacement worker to deliver messages.
Use one delivery owner: listener, dispatcher, managed worker, or bridge. The runtime rejects concurrent dispatchers.

Next: [command reference](COMMANDS.md) and [the operating guide](../OPERATING.md).
