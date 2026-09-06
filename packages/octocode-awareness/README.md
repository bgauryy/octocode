# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

Local coordination for coding agents: shared work, messages, verification, and reusable learning. SQLite stores coordination state; repository files and observed checks establish truth. No server or daemon. Requires Node 22.13+.

## Start

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
npx @octocodeai/octocode-awareness attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness maintenance init --compact
```

The canonical skill source is
`packages/octocode-awareness/skills/octocode-awareness` in this repository. The
published package bundles that Awareness skill only. The separately owned
`octocode-orchestrator` skill remains in the sibling
[`octocode-agent` repository](https://github.com/bgauryy/octocode-agent/tree/main/skills/octocode-orchestrator).
Follow structured `attend.next`; discover one route with `schema command <noun> [action]`.
Results are JSON and `--compact` reduces output. [docs/SKILLS.md](docs/SKILLS.md).

Use **NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE → SETTLE/VERIFY → LEARN**. A meaningful shared-state signal earns one `attend`; unchanged state means continue authorized work. Declare work with paths and a check, run it, then end/submit, `verify mark`, and `verify audit`. `next` is read-first guidance: `{ action, reason, target?, command?: { name, args } }`. A command exists only when safe and available; it never claims work, marks verification, compacts context, or authorizes edits.

## Storage and routes

The default store is `$OCTOCODE_HOME/awareness/awareness.sqlite3` (`~/.octocode` when home is unset). Workspace policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`; `--db <path>` wins. Stores never merge, and Agent control and runtime databases remain separate. [Configuration](docs/CONFIGURATION.md) · [storage scopes](docs/STORAGE_SCOPES.md).

One root noun/action vocabulary owns the Awareness ledger: `plan`, `task`, `work`, `lock`, `verify`, `memory`, `agent`, and `signal`. `handoff`, `guide`, `instructions`, and `hooks pre-edit` remain specialized entry points. `coordination …`, `check`, and `message` are removed; use `verify` and `signal`. Inspect the live schema before acting. [Architecture](ARCHITECTURE.md) · [lifecycle](docs/HOW_IT_WORKS.md).

| Entry point | Capability |
|---|---|
| `attend` | Structured, bounded next action, operational state, and advisory regulation. |
| `status`, `query` | Read-only workboard, ownership, diagnostics, and exports. |
| `plan` | Durable objectives, members, documents, and lifecycle. |
| `task` | Acceptance, paths, dependency graph, claims, presence, submit, and verification. |
| `work` | Advisory work presence, leases, overlap inspection, and runs. |
| `lock` | Explicit exclusivity, bounded waits, renewal, release, and cleanup. |
| `verify` | Observed check receipts and debt audits. |
| `agent` | Identity registration and presence. |
| `signal` | Typed peer messages, threads, acknowledgements, and resolution. |
| `memory` | Scoped observations, lexical/semantic recall, provenance and expiry, evaluation, reindexing, and lifecycle. |
| `refinement`, `reflect` | Owned follow-up and concise reusable lessons. |
| `session capture` | Repository continuation context and session-linked learning. |
| `maintenance` | Initialization, diagnostics, stale-state cleanup, and self-tests. |
| `config`, `hooks`, `hook run` | Policy, host integration, health checks, and lifecycle receipts. |
| `docs`, `schema` | Reference navigation and command/entity discovery. |
| `handoff`, `guide`, `instructions export` | Continuation notes and host workflow instructions. |
| `database consolidate` | Explicit conversion into a new file; rejects collisions and incomplete source contracts. |
| Library continuity APIs | Ordered outbox, consumer cursors, acknowledgements, and redacted worker projections. |

Plans and tasks share run-owned claims, work presence, locks, and verification. Peer messages are signals; handoffs retain continuation notes. Host APIs and CLI commands read the same IDs.

`schema entities --compact` inventories entity owners and kinds. Presence, signals, and memory are leads; schemas, locks, and verification debt enforce their own boundaries. Expiry recovers coordination state but never proves completion or success.

## Agent physiology

Awareness reports observed verification debt, scoped contention, reference warnings, and omitted rows, then recommends bounded corrections. Native runtime context supplies token occupancy and, only for a fresh matching model input limit, input headroom and saturation. Unknown limits retain occupancy but omit normalized values. Tool failure guidance requests inspection; it cannot retry, compact, or choose a model. Generic task/spend budget remains unavailable without its own sensor. [Implemented behavior and limits](docs/AGENT_PHYSIOLOGY.md).

## Integrate and verify

In-process hosts import `openAwarenessStore` and external-agent helpers; unique continuity commands also expose `dispatchAwarenessCommand` from `@octocodeai/octocode-awareness`. The sibling native host imports that public package API through a local Yarn portal during development. Internal callers import the owning module. Shared entity types and embedding utilities belong to `@octocodeai/agent-contracts/entities` and `@octocodeai/agent-contracts/embed`; Awareness does not provide compatibility modules for them. Preview optional hooks before installation, then check the selected host:

```bash
npx @octocodeai/octocode-awareness hooks install --host <host> --profile coordination --dry-run
npx @octocodeai/octocode-awareness hooks check --host <host> --project-dir . --strict
yarn workspace @octocodeai/octocode-awareness verify
```

Edit the package-local [skill](skills/octocode-awareness/SKILL.md); a build refreshes published mirrors. For native host integration changes, run the sibling [native Agent checks](https://github.com/bgauryy/octocode-agent/tree/main/packages/octocode-agent) as well. [Documentation index](docs/README.md) · [docs/VERIFY.md](docs/VERIFY.md) · [Thesis](docs/THESIS.md) · [References](docs/REFERENCES.md).
