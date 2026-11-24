Load when installing agent communication, wiring MCP and host hooks, repairing binding, or verifying delivery.

# Set up local agent coordination

Connect the current host to agents sharing one local SQLite database. Agents own their plans; this layer exchanges messages, data and activity status. [Choose the interface](coordination.md#choose-the-interface) and [Caller identity and binding](coordination.md#caller-identity-and-binding) own the shared MCP, init and identity rules; [Mail and evidence](coordination.md#mail-and-evidence) owns messaging outcomes and continuations.

## Install and inspect

1. Identify the actual host, workspace, existing MCP binding and hooks. Vendor names are `codex`, `claude`, `cursor` and `octocode`. Octocode here means the coding agent, not the research CLI. An executable named `agent` alone does not identify Cursor.
2. Reuse a working package at a stable path. If absent, use Node.js `^24.15.0` and `npm install --global @octocodeai/octocode-agents-communication`. Verify `octocode-agents-communication /cli --help`. Permanent hooks must not point into an ephemeral `npx` cache.
3. Run `octocode-agents-communication /cli skill --json`. Read its `instructions`; resolve `HOST_SETUP.md` and `INSTALLATION.md` under its absolute `referenceRoot`. These installed guides own version-specific setup. Check `schema host-config` before choosing a preset; report an unsupported installed version.
4. Reuse the shared database. Default: `<OCTOCODE_HOME or ~/.octocode>/agents-communication/communication.sqlite`. Linked worktrees use the same file. Check `db info --json --workspace PATH [--database FILE]` through `/cli`. An absent database is created at first binding. For incompatible databases follow [Requirements and recovery](coordination.md#requirements-and-recovery).

## Configure host hooks and MCP

Generate the hooks through `/cli` and merge them into the host's project hook file (table below) with [hook-merge.mjs](../scripts/hook-merge.mjs), previewing first:

```bash
octocode-agents-communication /cli host-config --json --vendor HOST --workspace ABSOLUTE_WORKSPACE [--database ABSOLUTE_DB] \
  | node <skill-folder>/scripts/hook-merge.mjs --file HOOK_FILE --add - --replace 'octocode-agents-communication|communication\.js host-hook|completion-check' --dry-run
```

Read the preview's `settings` and `removed`: `--replace` must match only this installation's communication handlers, including old `completion-check` gates; narrow the regex when it removes anything else. Rerun without `--dry-run` to write. It keeps unrelated settings and handlers, skips identical entries (`skipped`), so reinstalling is idempotent, and never writes a preview file into a hook directory. Exit 1 `versionConflict` or 2 invalid JSON leaves the file unchanged.

| Host | Project hook file | Verify loading |
|---|---|---|
| Codex | `.codex/hooks.json` | `/hooks`; trusted project, hooks enabled |
| Claude Code | `.claude/settings.local.json` | `/hooks`; restart/resume |
| Cursor | `.cursor/hooks.json` | Preserve `version:1`; reload and inspect hook diagnostics |
| Octocode coding agent | `.octocode/hooks.json` | `/trust` if needed, `/reload`, `/harness hooks` |

Octocode also imports Claude/Codex hooks. Disable duplicate imported **communication handlers** in its `/harness hooks` before enabling the native preset; retain unrelated hooks. Current Octocode uses native hooks and MCP, not Pi code extensions.

Configure one stdio MCP server per agent with the stable `octocode-agents-communication` executable and `args: ["--workspace", "ABSOLUTE_WORKSPACE", "--database", "ABSOLUTE_DB"]`. Omit `--database` for the default. Merge into the host's MCP configuration following its guide; Octocode uses `.octocode/mcp.json` and `/mcp config`. The default tool profile is messaging.

Follow [Caller identity and binding](coordination.md#caller-identity-and-binding) to reuse the hook identity without hardcoding a transient session or duplicating an already-managed identity. Host trust stays with the host; report any user-only activation step precisely.

## Verify binding and delivery

After reload, require an actual session-start binding receipt and successful `agents/list`. A config file alone proves no hook ran. Test a small direct message between two isolated participants or an explicitly intended peer: run a harmless recipient tool, observe one injection and acknowledge its exact message ID. Do not broadcast a setup check.

For idle delivery, presence renewal and uncertain delivery follow [Requirements and recovery](coordination.md#requirements-and-recovery) and [Peers and leases](coordination.md#peers-and-leases).

## Introduce and use discovered tools

Initialize and refresh discovery using [Caller identity and binding](coordination.md#caller-identity-and-binding). Use discovered MCP schemas with `{caller,type,input}`. In these examples, `CALLER` means the exact returned `binding.caller` object, not a literal placeholder to send:

| Purpose | Tool and example input |
|---|---|
| Find peers and their activity | `agents` `{ "caller":CALLER, "type":"list", "input":{} }` |
| Indicate current work | `agents` `{ "caller":CALLER, "type":"status", "input":{"status":"busy","task":"Reviewing API compatibility"} }` |
| Notify a chosen peer and pass data | `messages` `{ "caller":CALLER, "type":"send", "input":{"to":"PEER_ID","parts":[{"text":"Review ready"},{"data":{"path":"src/api.ts","result":"compatible"}}],"reasoning":"Share review result"} }` |
| Read pending mail | `messages` `{ "caller":CALLER, "type":"read", "input":{} }` |
| Acknowledge a handled FYI | `messages` `{ "caller":CALLER, "type":"complete", "input":{"messages":[MESSAGE_ID]} }` |

Replace placeholders with returned IDs. For requested results, progress, completion, pending requests and the authority boundary, follow [Mail and evidence](coordination.md#mail-and-evidence); for activity labels follow [Peers and leases](coordination.md#peers-and-leases).

## Setup result

Report the package path, hook file, MCP connection, verified binding, shared database, delivery result and pending activation step. Distinguish fixtures from a live host check. Hand ongoing use to [Coordinate local agents](coordination.md); follow the [saved-report contract](../output.md#saved-report).
