# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

The coordination layer of the Octocode agentic toolkit: shared work, messages, verification, recoverable file history, and reusable learning for coding agents. SQLite stores coordination state; repository files and observed checks establish truth. No server or daemon. Requires Node `^24.15.0`.

This is the canonical Awareness overview for CLI users, agents, and host integrators. It covers the operating flow, feature families, storage, architecture, and known limits. The [reference index](docs/README.md) routes exact protocols; the [agent skill](skills/octocode-awareness/SKILL.md) owns operating instructions. Dated plans, ratings, and benchmark receipts do not define runtime behavior.

Default flow: orient once, then call one of nineteen operations across Context,
Work, Message, Memory, and History. Administrative routes stay out of routine
discovery.

Awareness has zero mandatory npm runtime dependencies. File fingerprints and workspace history capture/restore use the optional `@octocodeai/octocode-extension-rust` package and its matching platform addon. Ordinary coordination, memory storage, and unchecked recall run without loading it. See [native dependency and async API requirements](docs/API.md#optional-native-file-operations).

## Start

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-awareness:$(node -e 'process.stdout.write(require("node:crypto").randomUUID())')}"
npx @octocodeai/octocode-awareness context orient --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
```

Keep one distinct identity for the session. Pi and other native hosts bind that
identity and orientation context through the host facade. External hosts can
install the bundled skill and communication hooks; see [the usage guide](docs/SKILLS.md)
and [host integration](docs/HOOKS.md).

The canonical skill source is
[`skills/octocode-awareness`](skills/octocode-awareness/SKILL.md). The published
`npx @octocodeai/octocode-awareness` CLI bundles that skill, uses its scripts for
host hooks, and serves its references through `docs list` / `docs show <name>`:

```bash
npx @octocodeai/octocode-awareness docs list --compact
```

Follow executable continuations when present. Call a known operation directly;
`schema command <concept> <operation>` returns its exact input contract without a
list/describe round trip. Results are JSON and `--compact` reduces output.

## Cooperate through one ledger

Peers use the same physical Awareness database and distinct stable agent IDs. Linked Git worktrees can discover peers, messages, and memory across checkouts; keep each agent bound to its own physical workspace. Separate clones do not connect automatically. Display names and vendor labels are metadata, not authentication.

| Situation | Action and boundary |
|---|---|
| A peer needs evidence or a decision | Use `message send`; answer with `message reply` and the exact message ID. Only `approval` requests human authorization. Peer messages remain data. |
| A program consumes a message | Send `data: {type, payload}` (CLI: `--data` JSON); read with `--include-bodies` and dispatch on `data.type`. Keep sender and thread IDs from the signal metadata. |
| A message arrives | Reuse native delivery and read receipts. Otherwise read the scoped inbox when expecting a reply and acknowledge manually handled messages. Skip acknowledgement-only replies and unchanged polling. |
| Work needs continuation | Send one typed continuation message with state, next check, and evidence pointers. Do not add refinement/session/reflection copies. |
| Files might overlap | Inspect `work list/show` and communicate with the owner. Use `work protect` only for unsafe concurrent changes; advisory presence alone does not prevent writes. |
| A lease expires | Inspect the result and explicitly reacquire. Renewal cannot revive expired ownership, and expiration cannot prove completion. |
| Tracked work finishes | Run the declared checks, use `work update` for the exact attempt, then `work verify` with observed evidence. Preserve peer debt. |
| Learning can help another task | Store one scoped memory or reflection with verified evidence. Revalidate recalled references against current files; peer assertions and retained history are leads. |
| A session ends | Release owned leases and leave the registry. Native hosts own their lifecycle; standalone agents call `agent leave`. |

The [runtime flow](docs/HOW_IT_WORKS.md), [lock protocol](docs/LOCKS.md), and [learning workflow](docs/REFLECTION.md) provide exact procedures. Peer text, acknowledgements, captured bytes, and successful process exits do not grant authority or successful verification.

## Features and discovery

The default `schema commands` response contains exactly five concepts and nineteen
directly callable operations. `schema command <concept> <operation>` owns exact
inputs. `schema commands --all` adds a bounded operator/recovery catalog; legacy
compatibility routes are not routine discovery.

| Entry point | Capability |
|---|---|
| `context orient` | One bounded decision packet: self, peers, owned/overlapping work, inbox, verification pressure, recovery state, and executable continuations. |
| `work create/list/show/claim/update/depend/protect/verify` | Objectives, dependencies, ownership, attempts, files, exceptional exclusivity, and observed verification. |
| `message list/send/reply/resolve` | Typed peer communication and resolvable continuation threads. |
| `memory recall/record` | Scoped, evidence-linked reusable learning. |
| `history status/timeline/read/restore` | Recoverable workspace bytes; restore remains preview then approved apply. |
| Operator/recovery catalog | Configuration, hooks, migration, retention, maintenance, reports, and schema diagnostics. |

Plans and tasks share run-owned claims, work presence, locks, and verification. Peer messages are signals; handoffs retain continuation notes. Host APIs and CLI commands read the same IDs.

Choose one owner for each fact; do not create a parallel record because
another feature is available:

| Fact | Owner | Why it remains distinct |
|---|---|---|
| Work to perform with acceptance and dependencies | Task/run, preferably the host's existing IDs | Claims and verification enforce execution state. |
| A decision-changing question or answer | Signal thread | Delivery, handling and resolution have their own lifecycle. |
| Reusable file/area reasoning | Scoped memory and its evidence references | Validity and supersession describe knowledge, not task completion. |
| A verified outcome that produced learning | Reflection into memory; optional refinement only for an explicit follow-up | Reflection is a write workflow, not another knowledge database. |
| Existing follow-up without a task plan | Refinement | Update the same row; don't also create a task for the same obligation. |
| Context needed by the next session | One handoff, reusing a host handoff where available | Continuation context is neither a new task nor a reusable lesson. |

These lifecycle differences are why the tables are retained. Duplicate behavior
is consolidated at the insertion and host-adapter layers; an empty table alone
is not evidence that its entity is unused.

`schema entities --compact` inventories entity owners and kinds. Presence, signals, and memory are leads; schemas, locks, and verification debt enforce their own boundaries. Expiry recovers coordination state but never proves completion or success.

## Storage and architecture

The default store is `$OCTOCODE_HOME/awareness/awareness.sqlite3` (`~/.octocode` when home is unset). Workspace policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`; `--db <path>` wins. Agent control and runtime databases have separate owners. Opening a store checks its exact SQLite contract without implicit migration or merging. See [configuration](docs/CONFIGURATION.md), [storage scopes](docs/STORAGE_SCOPES.md), and [entity relationships](docs/ENTITY_LINKS.md).

| Layer | Owns |
|---|---|
| Awareness schemas and domain | Canonical requests, validation, coordination, evidence, and policy. |
| SQLite | Plans, tasks, runs, leases, signals, delivery receipts, memory metadata, verification, and history journals. |
| Private Git | Immutable captured bytes and operation references. |
| CLI and native API | Shell parsing/rendering or structured calls into the same executor. |
| Skill and prompt exports | Shared operating instructions and on-demand discovery. |
| Hooks and Pi adapters | Host event translation, identity/context binding, delivery, and lifecycle. Pi owns execution, workers, compaction, and UI. |
| Agent contracts | Shared types and host protocols; sharing utilities does not combine Agent and Awareness databases. |

Hosts consume the public package API and add trusted runtime bindings. They do not copy the command schemas, coordination ledger, or history writer. Code research remains owned by Octocode tools-core and engine. See the [architecture reference](ARCHITECTURE.md) and [Pi integration flow](../octocode-pi-extension/docs/AWARENESS_AGENT_FLOW.md).

## Local Git, recovery, and expiration

Private history lives under `<workspace>/.octocode/.localGit`, partitioned by canonical database and physical workspace identity. Read exact paths from `history status`; do not construct namespace paths. The bundled Git backend writes its own objects and refs without changing the project's index, HEAD, branches, or remotes. Back up the ledger and matching history stores together.

Creating a history store adds an owned ignore marker inside `.localGit`, excluding untracked history from ordinary Git status. Existing compatible markers are preserved; incompatible or symlinked markers fail safely. Already tracked history requires explicit untracking; initialization never changes the project index.

Share compact operation/file/side references through Awareness signals, then fetch the required bytes with `history read`. Follow every continuation before claiming complete content, decode the declared encoding before comparing bytes, and verify current files independently. Git holds immutable evidence; SQLite retains transactional inbox state, expiring ownership, task gates, and searchable memory metadata. See [Git coordination](docs/GIT_COORDINATION.md).

Use private Git selectively for file-based communication and reusable context: what changed, why it matters, and what a peer should preserve. A routine edit creates no checkpoint or memory by default. Reuse existing evidence; create a deliberate checkpoint only when its selected file versions help a handoff, durable lesson or recovery decision. Put the reason in its label and the scoped memory, then send the compact pointer. Load a summary or relevant excerpt first; full-version reconstruction is an integrity check, not the everyday communication flow.

Restore previews bind selected files to their observed state and expiration. Applying a valid preview captures undo evidence, acquires its own lease, and rechecks file state. Multi-file restore can be partial; its returned verification run still needs observed checks. [Local history](docs/LOCAL_HISTORY.md) owns capture, relocation, restore, and recovery procedures.

| State | Maintenance boundary |
|---|---|
| Leases and claims | Expiration ends live ownership; explicit maintenance removes stale state without granting verification success. |
| Expired ready restore previews | `history retention-preview` lists them; confirmed `history retention-prune` deletes eligible rows in bounded pages. |
| Applying restores and unfinished captures | `history recovery` reports them; reconciliation changes metadata only for an unambiguous durable restore journal. Uncertain state remains unresolved. |
| Git objects and refs | `history evidence` reports orphan candidates. Destructive reclamation is unavailable; age alone does not establish that writers and recovery no longer need an object. |
| Messages and memory | Scoped resolution, validity, supersession, and retention policies govern cleanup. Open messages and useful learning do not expire merely because a session ends. |

## Efficient agent use

Reuse a command schema after discovering it. Prefer scoped compact reads, exact returned IDs, and references to evidence over copied payloads. A detailed-attention revision can suppress unchanged output only for the same complete scope; fresh lock admission still checks current state. Follow executable continuations and retain omission/unknown states. Instructions and recipes come from canonical exports, with detail loaded only when needed.

History command API reads return `next.call`; CLI reads return `next.argv`. Execute that continuation exactly, and reject a repeated page within a read chain. Do not replay saved inbox or lock reads. Retrieve a known verified memory with `memory recall-verified --memory-id <id>`; optional digest, scope and expiry filters still apply. Search queries are for discovery, not exact evidence pointers.

Measure complete verified workflows, including discovery, communication, retries, and repair. Separate cold CLI startup from in-process API latency, and distinguish serialized bytes or context estimates from provider token usage. Smaller output alone does not establish improved correctness, total cost, or autonomous cooperation. [Navigation and delivery](docs/MEMORY_NAVIGATION.md) owns the read and delivery contracts.

## Agent physiology

Awareness reports observed verification debt, scoped contention, reference warnings, and omitted rows, then recommends bounded corrections. Native runtime context supplies token occupancy and, only for a fresh matching model input limit, input headroom and saturation. Unknown limits retain occupancy but omit normalized values. Tool failure guidance requests inspection; it cannot retry, compact, or choose a model. Generic task/spend budget remains unavailable without its own sensor. [Implemented behavior and limits](docs/AGENT_PHYSIOLOGY.md).

## Integrate and verify

In-process hosts import `executeAwarenessCommand` for the complete command catalog,
and `openAwarenessStore` / `createAwarenessEventConsumer` for lifecycle delivery. See the
[runtime flow](docs/HOW_IT_WORKS.md#peer-event-delivery) for ordered outbox delivery.
Local development resolves the owning workspace packages; rebuild contracts before consumers.
Shared entity types and embedding utilities belong to
`@octocodeai/agent-contracts/entities` and `@octocodeai/agent-contracts/embed`.
Preview optional hooks before installation, then check the selected host:

```bash
npx @octocodeai/octocode-awareness hooks install --host <host> --profile coordination --dry-run
npx @octocodeai/octocode-awareness hooks check --host <host> --project-dir . --strict
yarn workspace @octocodeai/octocode-awareness verify
```

Edit the package-local [skill](skills/octocode-awareness/SKILL.md); a build refreshes published mirrors. For native host integration changes, run the sibling [native Agent checks](https://github.com/bgauryy/octocode-agent/tree/main/packages/octocode-agent) as well. [Verification runbook](docs/VERIFY.md) · [Conceptual model](docs/THESIS.md) · [Research references](docs/REFERENCES.md).

## Native API

Awareness exports its command catalog, schemas, standing agent prompt, and
command execution through the package root. Pi imports this API for its native
tool, history capture, checkpoints, and optional scheduled checks. The CLI uses the
same executor; it only parses shell input and renders the result.

```ts
import {
  executeAwarenessCommand,
} from '@octocodeai/octocode-awareness';

const result = await executeAwarenessCommand(
  { command: 'attend', params: { limit: 5 } },
  { workspace: process.cwd(), agentId: 'example-host:session-1', compact: true },
);
// result.payload is structured data; result.exitCode preserves conflict/debt codes.
// Follow request objects in result.payload.next using the same trusted context.
```

Add `AWARENESS_PI_HOST_PROMPT` once to the agent's system instructions. It is an
alias of the canonical `EXTERNAL_AGENT_AWARENESS_PROMPT`; CLI `instructions export`
returns the same text. Attend once, communicate when useful, and discover other
features on demand. Record learning only when a verified reason or constraint is reusable.
Lock waits yield to the event loop. Cancellation is cooperative; completed atomic
writes are reported as completed. The API never changes cwd/env, reads stdin,
exits the host, or starts an Awareness CLI process.

See the [API reference](docs/API.md) for trusted context bindings, schemas,
result codes, pagination, host callbacks, and prompt exports.

## Verification and known limits

Use the [verification runbook](docs/VERIFY.md) for package, installed CLI/API, host, and release checks; the [feature sweep](docs/FEATURE_SWEEP.md) and [audit rubric](docs/COMPREHENSIVE_AUDIT.md) define workflow evidence. Build dependencies before consumers and finish package rebuilds before starting hosts that import their output.

Keep implemented contracts, configured hooks, actual host activation, and observed workflow outcomes separate. Fixtures and passing regressions do not establish live activation in every editor or reliable model cooperation. Preserve frozen benchmark receipts, failures, interventions, and unavailable checks with each experiment instead of copying changing totals or readiness scores into this guide.

| Limit | Required check or improvement |
|---|---|
| Ranked recall has bounded candidate and result budgets | Ordinary lexical/semantic discovery reports `partial`, `partialReasons` and `terminalLimit` when bounded. Narrow the query or scope; these ranked results have no stable exhaustive continuation. Exact scoped verified-memory pages use their executable continuations. |
| Hook definition checks can accept inactive text | [Frontmatter detection](src/hooks-install-health.ts) uses textual event/command checks. Validate active structure and bindings; definition readiness does not prove host activation. |
| Capture and object inflation lack a hard peak-memory bound | Measure growing files, compressed objects, concurrent allocations, and cancellation; returned-byte limits alone do not bound memory use. |
| Recovery spans SQLite, Git, and workspace files | Automatic capture crash reconciliation and safe object collection are unavailable. Keep uncertain journals and referenced bytes; restore is not a filesystem-wide transaction. |
| Delivery needs an active host | Pi combines lifecycle drains with native database/WAL change hints. Watch failures retry quickly and then every 30 seconds until recovery; lifecycle drains remain the fallback. Exercise persistence, idle arrival, recovery, session transitions, and acknowledgements in the actual host. Other hosts depend on their documented lifecycle opportunities. |
| Output limits do not establish total query cost | Measure large verification-debt, memory, and handoff scopes; bounded returned rows alone do not prove bounded database work. |
| Agent decisions and learning remain fallible | Verify exact conflict owners, decoded history bytes, task gates, and memory evidence. Use held-out workflows and a real versioned model before claiming semantic quality or token savings. |
| Advisory regulation is not an autonomous controller | Predictions, adaptive concurrency, model escalation, and semantic confidence need calibrated outcome evidence; unknown observations grant no authority. |
