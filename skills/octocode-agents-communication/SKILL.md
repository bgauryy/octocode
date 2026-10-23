---
name: octocode-agents-communication
description: "Load this skill first whenever agents on different hosts (Claude Code, Codex, Pi, Octocode) or worktrees share files, message each other, hand off a path, or lose a delivery; good for peers, locks, messages and the communication CLI or MCP. Not for agents within one Octocode or Pi team, which coordinate natively."
---

# Agents communication

Coordinates agents through one local SQLite database. It runs the published `@octocodeai/octocode-agents-communication` package via `npx`; nothing is bundled or installed, and there is no remote cloud transport. Inside a Pi team use Pi's `coordinate`, `sendMessage` and `agent`; use this package to reach agents outside that team. Pi's file tools honor leases in the default database and in any `--database` file listed in the package's active-database registry (`DB.md`, under `referenceRoot`). Leases do not fence shell or OS writes.

```mermaid
flowchart TD
  A[Task] --> B{Other agents or shared files involved?}
  B -- no --> Z[Work alone]
  B -- yes --> J[Reuse the host binding, else join; peers]
  J --> E{Editing shared paths?}
  E -- yes --> L[lock; on conflict lock --wait true; still blocked: blocker]
  E -- no --> M[Send requests; read inbox at task boundaries; complete handled mail]
  L --> M
  M --> U[unlock; leave only an identity you own]
```

## Run the CLI

```sh
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli skill --json
```

- `skill --json` returns `instructions` (the operating guide), `packageRoot` and `referenceRoot`. Read `instructions` and follow it; this skill does not restate it. Its documentation filenames resolve under `referenceRoot`.
- Command inputs: `<command> --help` or `schema <command>`. Record formats: `schema types`, `schema type NAME` (no workspace, database or session flags).
- Both routes take `--workspace` (your checkout). `/cli` takes `--session ID` on every identity-bound call; without `/cli` the executable starts MCP, which binds the identity at launch with `--session ID` or `--managed --name NAME --vendor HOST`. A `--session required` error means neither was given.

## Situations the guide decides

| Situation | Guide section (`instructions`) |
|---|---|
| First call, finding peers, presence | Discover and choose work |
| Before editing a shared path | Edit with ownership |
| Sending, answering, handing off | Send and handle messages |
| Evidence, checkpoints, paging | Share evidence and continue reads |
| Delivery lost or uncertain | `OPERATIONS.md` under `referenceRoot` |

The guide owns the presence, lease, message and mail rules; read it before the first call. For uncertain delivery, run `health` and inspect the recipient first: a restart never authorizes replay.

## Commands

Every command with an example: `COMMANDS.md` under `referenceRoot`. `run` starts a worker (Claude, Codex, Pi) only when requested; `attach` joins an existing session. Grok: native session via `attach` and `listen`, or host hooks.

## Set up and recover

- Needs Node.js `^24.15.0` (its built-in SQLite, FTS5 included); no other interpreter, API key or `<HOME>/.octocode/.env` entry.
- No `--database`: the shared Octocode home database. Hooks and native delivery: the package's installation and host setup guides; this skill configures none.
- Incompatible store (`db info --json` shows `exists: true` and `compatible: false`): report it and use a fresh `--database`. `exists: false` is a database not created yet: `join` creates it. Never switch coordination tools silently.
- `npx` cannot resolve the package: report the registry or access error; a local executable proves only the runtime.

## Resources

| When | Read | Why |
|---|---|---|
| Render or list saved reports | [render-report.mjs](scripts/render-report.mjs) | HTML view, manifest |

## Related skills

- Use `octocode-agentic-prompts` when a handoff's wording or ownership is unclear.
- Use `octocode-research` when a handoff needs independent code evidence.
- Use `octocode-workflows` when an orchestrator selects actors, sequences work or merges evidence ([composition](../octocode-workflows/references/composition.md)).

## Output

Follow [output.md](output.md).
