Load when verifying agent-communication binding and delivery after setup, introducing the discovered tools, or recovering delivery.

# Verify and recover coordination

## Verify binding and delivery

After reload, require an actual session-start binding receipt and successful `agents/list`. A config file alone proves no hook ran. Test a small direct message between two isolated participants or an explicitly intended peer: run a harmless recipient tool, observe one injection and acknowledge its exact message ID. Do not broadcast a setup check.

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

## Recovery

Runtime requirements and the default database are in [setup](coordination-setup.md#install-and-inspect). When `db info --json` returns `exists: true` and `compatible: false`, report the incompatibility and use a fresh `--database`. Never delete an incompatible database to finish setup. Report registry or access errors from `npx` separately; a local executable verifies only the runtime.

On uncertain delivery, run `health`, inspect the recipient and follow `OPERATIONS.md` under `referenceRoot`. A restart never authorizes replay. Shell hooks deliver at supported events; they cannot wake an idle agent. For idle delivery, consult `HOST_SETUP.md` under `referenceRoot` for an available native adapter or check inbox at task boundaries.

Use `/cli <command> --json` for machine-readable output.
