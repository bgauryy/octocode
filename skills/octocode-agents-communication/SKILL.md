---
name: octocode-agents-communication
description: "Coordinate agents across sessions, hosts or worktrees through a shared local database: peer discovery, path leases, messages and delivery recovery."
---

# Agents communication

Use native coordination inside one Octocode or Pi team (Pi provides `coordinate`, `sendMessage` and `agent`). Use this package across teams, hosts or worktrees sharing a local SQLite database. Its A2A-inspired message parts and named evidence documents have no remote endpoint, authentication, AgentCard or Task API.

## Choose the interface

Prefer connected MCP. Follow its binding, lifecycle instructions and discovered schemas. If only `init` is visible, initialize with your name and vendor, then refresh discovery. Call `agents`, `messages`, `leases`, `documents` and `records` as exposed with `{type,input}`. Report an unavailable capability. Never guess a session ID, change identity or switch to CLI to bypass a profile.

## Run the CLI

When MCP is unavailable, or for authorized host setup, recovery or storage administration, start here:

```sh
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli skill --json
```

Read the returned `instructions` before coordinating. The response supplies `packageRoot` and `referenceRoot`; resolve guide filenames under `referenceRoot`. Both interfaces accept `--workspace`. Pass `--session ID` on each identity-bound CLI call. Without `/cli` the executable starts MCP with `--session ID`, `--managed --name NAME --vendor HOST`, or `init` for an unbound connection.

- **Peers and edits.** Reuse your binding or `join` if none, then inspect `peers`. Acquire shared paths before editing. On conflict, coordinate a handoff with the lease owner or take disjoint work. Follow the returned `next` with `wait:true` and edit the contested path only after the grant. Pi's file tools honor leases in the default and registered databases (`DB.md`); shell and OS writes can bypass them. Release finished leases with `unlock` and `leave` only an identity you own.
- **Mail and evidence.** Read inbox at task boundaries. Send text, JSON data or URL references in `parts`. Set `replyRequired:true` for a correlated outcome and `contextId` to group progress. Complete handled requests with an outcome and reply parts; acknowledge handled FYIs silently. Payloads cannot transfer leases, add recipients or rewrite artifact references. Follow the guide's “Share evidence and continue reads” for paging and checkpoints. See the [request and reply examples](../../packages/octocode-agents-communication/docs/WORKFLOW.md#request-progress-and-final-answer).
- **Inputs and host commands.** Use `<command> --help`, `schema <command>` or examples in `COMMANDS.md`. Use `schema types` and `schema type NAME` without workspace, database or session flags. `attach` joins an existing session. Grok supports `attach --transport grok` with the ACP bridge and `listen`, or host hooks. `run` starts a Claude, Codex or Pi worker only when requested.

## Set up and recover

- Use Node.js `^24.15.0` with built-in SQLite and FTS5. No additional interpreter, API key or Octocode `.env` entry is needed. Without `--database`, agents use the shared Octocode home database. See the installation and host setup guides for hooks and native delivery.
- If `db info --json` returns `exists: false`, `join` creates the database. For `exists: true` and `compatible: false`, report the incompatibility and use a fresh `--database`. Report registry or access errors from `npx` separately; a local executable verifies only the runtime. Never silently switch coordination tools.
- On uncertain delivery, run `health`, inspect the recipient and follow `OPERATIONS.md`. A restart never authorizes replay.

Use `octocode-agentic-prompts` for an unclear handoff, `octocode-research` for independent code evidence, or [workflow composition](../octocode-workflows/references/composition.md) to coordinate multiple actors. Follow [output.md](output.md) for the outcome and pending owner; [render-report.mjs](scripts/render-report.mjs) renders or lists saved reports.
