# Install and bind a participant

Load when the host has no communication binding. Why: setup belongs outside ordinary worker context.

`Install complete folder → choose shared DB → bind identity → verify discovery`.

## Install the CLI and companion skill

The default npm executable serves MCP. Use `/cli` for standalone operations.

```sh
npx -y @octocodeai/octocode-agents-communication --help
npx -y @octocodeai/octocode-agents-communication /cli --help
```

See [entry points](INTERFACES.md) for local archive use and host configuration.
The npm launcher needs Node.js 24.15+ (24.x) and Python 3.9+ with SQLite 3.42+ and FTS5; no pip packages are required.
Set `OCTOCODE_PYTHON` to an absolute interpreter path if Python is not on PATH.

The optional lean skill ships in the `octocode` package:

```sh
octocode skill install octocode-agents-communication --platform claude,codex --global --dry-run
```

Inspect the destination plan, then repeat without `--dry-run` and reload skill discovery.
The installed skill contains guidance only; operations run through the npm CLI.

For a source checkout, `yarn pack:runtime` creates a verified portable runtime archive in `out/`.
Extract it to a stable runtime directory and invoke `scripts/agents-communication` (`.ps1` on Windows), or `python3 -B scripts/communication.py`.
This archive contains `OPERATING.md` and `scripts/`; it is not an installable Agent Skill.
Direct Python CLI/MCP requires no Node.js. JavaScript host adapters and edit guards require Node.js.

## Reuse or create the binding

Reuse the identity from the host, managed MCP, or Pi extension.
A vendor thread/session ID is separate `vendorSession` metadata. Use the exact DB identity from the receipt.
Each independent child agent needs its own binding. A restricted tool profile is not permission to join another identity.
All participants use one database file. Linked worktrees use their own actual checkout as `--workspace`.
The runtime derives `coordinationScope` from Git's canonical common directory; non-Git scope is the actual workspace.
Independent clones and unrelated projects remain separate coordination scopes.

For raw CLI setup, replace all three paths below. Do not create one DB per participant.

```sh
COMMUNICATION_WORKSPACE=/absolute/repo-or-worktree
COMMUNICATION_DB=/absolute/shared/communication.sqlite
comm() { npx -y @octocodeai/octocode-agents-communication /cli "$@" --json --workspace-root "$COMMUNICATION_WORKSPACE" --database "$COMMUNICATION_DB"; }
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

On Windows, use the same npm CLI command, or the portable archive's PowerShell launcher.
Pass JSON on stdin with `-` (for example `'{"name":"x"}' | powershell … agents-communication.ps1 join -`): Windows PowerShell 5.1 strips inner quotes from JSON arguments.
`comm` means a workspace/DB-bound call. `agent` also supplies the current identity.
A command accepts a JSON argument, or `-` for stdin JSON.
`--help`, `schema`, and `skill` work before joining. `db info` creates nothing.
Bound MCP tools take only JSON input; identity and workspace flags stay in host configuration.

## Select host delivery

For managed MCP or native attachment, read [host setup](HOST_SETUP.md).
Managed MCP owns identity, presence, live lease renewal, and exit cleanup.
Plain MCP borrows a host identity whose owner maintains lifecycle.
Choose `messaging`, `review`, `editing`, or an explicit tool list.
`run` creates a new Claude/Codex/Pi worker only on explicit request.
A raw binding receives mail through explicit inbox reads or configured hooks.
Skill installation does not install hooks or edit guards.

## Attach an existing native receiver

Use a real endpoint and native session ID from the owning host:

```sh
agent attach '{"transport":"codex","endpoint":"REAL_ENDPOINT","vendorSession":"REAL_NATIVE_ID"}'
```

Transports are `raw`, `claude`, `codex`, `opencode`, and `grok`; Pi uses its extension bridge.
Inspect `comm schema attach` for transport-specific endpoint requirements.
Do not invent native IDs or start a replacement worker to deliver messages.
Use one delivery owner: listener, dispatcher, managed worker, or bridge. The runtime rejects concurrent dispatchers.

For command examples, read [the command reference](COMMANDS.md).
For sending and completing work, follow [the operating guide](../../OPERATING.md).
