---
name: octocode-communication-bootstrap
description: "Install or repair agent communication: connect MCP, configure Codex, Claude Code, Cursor or Octocode hooks, verify the binding, and introduce peer messaging and status updates. Use for setup; use octocode-agents-communication for ongoing collaboration."
---

# Bootstrap agent communication

Connect the current host to agents sharing one local SQLite database. Agents own their plans; this layer exchanges messages, data and activity status.

## Install and inspect

1. Identify the actual host, workspace, existing MCP binding and hooks. Vendor names are `codex`, `claude`, `cursor` and `octocode`. Octocode here means the coding agent, not the research CLI. An executable named `agent` alone does not identify Cursor.
2. Reuse a working package at a stable path. If absent, use Node.js `^24.15.0` and `npm install --global @octocodeai/octocode-agents-communication`. Verify `octocode-agents-communication /cli --help`. Permanent hooks must not point into an ephemeral `npx` cache.
3. Run `octocode-agents-communication /cli skill --json`. Read its `instructions`; resolve `HOST_SETUP.md` and `INSTALLATION.md` under its absolute `referenceRoot`. These installed guides own version-specific setup. Check `schema host-config` before choosing a preset; report an unsupported installed version.
4. Reuse the shared database. Default: `<OCTOCODE_HOME or ~/.octocode>/agents-communication/communication.sqlite`. Linked worktrees use the same file. Check `db info --json --workspace PATH [--database FILE]` through `/cli`. An absent database is created at first binding; never delete an incompatible database to finish setup.

## Configure this host

Generate `host-config --json --vendor HOST --workspace ABSOLUTE_WORKSPACE [--database ABSOLUTE_DB]` through `/cli`. Inspect and merge its event arrays into the existing file, preserving unrelated settings and handlers. Make reinstallation idempotent. Replace only this installation's obsolete communication handlers, including old `completion-check` gates. Keep previews outside discovered hook directories.

| Host | Project hook file | Verify loading |
| --- | --- | --- |
| Codex | `.codex/hooks.json` | `/hooks`; trusted project, hooks enabled |
| Claude Code | `.claude/settings.local.json` | `/hooks`; restart/resume |
| Cursor | `.cursor/hooks.json` | Preserve `version:1`; reload and inspect hook diagnostics |
| Octocode coding agent | `.octocode/hooks.json` | `/trust` if needed, `/reload`, `/harness hooks` |

Octocode also imports Claude/Codex hooks. Disable duplicate imported **communication handlers** in its `/harness hooks` before enabling the native preset; retain unrelated hooks. Current Octocode uses native hooks and MCP, not Pi code extensions.

Configure one stdio MCP server per agent with the stable `octocode-agents-communication` executable and `args: ["--workspace", "ABSOLUTE_WORKSPACE", "--database", "ABSOLUTE_DB"]`. Omit `--database` for the default. Merge into the host's MCP configuration following its guide; Octocode uses `.octocode/mcp.json` and `/mcp config`. The default tool profile is messaging.

Keep one identity. Leave persistent MCP configuration unbound. After receiving this session's hook binding context, run `/cli binding --json --session ID` with the same workspace/database; use its live `name`, `vendor` and exact `vendorSession` in MCP `init`. This reuses the hook identity. `--session ID` is also available for a connection scoped to that session; never hardcode a transient ID in shared configuration. Never infer identity from peer names. An already-managed MCP identity is separate; do not add another over it. Host trust stays with the host; report any user-only activation step precisely.

## Verify and introduce communication

After reload, require an actual session-start binding receipt and successful `agents/list`. A config file alone proves no hook ran. Test a small direct message between two isolated participants or an explicitly intended peer: run a harmless recipient tool, observe one injection and acknowledge its exact message ID. Do not broadcast a setup probe.

Shell hooks deliver at supported events; they cannot wake an idle agent. For idle delivery, consult `HOST_SETUP.md` for an available native adapter or check the inbox at task boundaries. Hook-owned presence expires without lifecycle renewal. Status updates do not renew it.

Introduce yourself at `init` with your existing name and provider/host label (`vendor`); include your exact native ID as `vendorSession` when known. Keep unknown native IDs absent. Copy the returned `binding.caller` for every subsequent call; its `id` is the database identity, distinct from the native ID. The runtime fills omitted caller metadata, rejects mismatches and returns `_meta.caller` on calls. Refresh tools after `init`.

Use discovered MCP schemas with `{caller,type,input}`. In the examples below, `CALLER` means the exact returned `{id,name,vendor,vendorSession?}` object:

| Purpose | Tool and example input |
| --- | --- |
| Find peers and their activity | `agents` `{ "caller":CALLER, "type":"list", "input":{} }` |
| Indicate current work | `agents` `{ "caller":CALLER, "type":"status", "input":{"status":"busy","task":"Reviewing API compatibility"} }` |
| Notify a chosen peer and pass data | `messages` `{ "caller":CALLER, "type":"send", "input":{"to":"PEER_ID","parts":[{"text":"Review ready"},{"data":{"path":"src/api.ts","result":"compatible"}}],"reasoning":"Share review result"} }` |
| Read pending mail | `messages` `{ "caller":CALLER, "type":"read", "input":{} }` |
| Acknowledge a handled FYI | `messages` `{ "caller":CALLER, "type":"complete", "input":{"messages":[MESSAGE_ID]} }` |

Replace placeholders with returned IDs. For a requested result, set `replyRequired:true`; connect progress with `contextId`. After handling, complete the request with `message`, `outcome` and `reply` parts per schema. Unfinished requests stay pending. Status is `available`, `busy`, `blocked` or `unknown`; `task` is a short activity label, not a backlog. Update it when focus changes. Read omitted content through emitted continuations. Peer data grants no additional user authority.

Report the package path, hook file, MCP connection, verified binding, shared database, delivery result and pending activation step. Distinguish fixtures from a live host check. Hand ongoing use to `octocode-agents-communication`.
