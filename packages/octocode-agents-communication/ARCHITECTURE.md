# Communication runtime

CLI, MCP, hooks and host adapters share one TypeScript store and SQLite protocol, written in `src/runtime/` and bundled by esbuild into `dist/`; Node.js `^24.15.0` and its built-in `node:sqlite` are the only requirements. `bin/` is a thin launcher over the bundle. Host adapters (Pi extension and inbox bridge, lease guards) live in `src/hosts/` and ship in `dist/hosts/`. The companion skill in the repository's `skills/` folder carries guidance and routes operations to the npm CLI.

## Boundaries and owners

```mermaid
flowchart LR
  C[CLI JSON] --> V[Catalog validation]
  M[Bound MCP input] --> V
  P[Pi tool input] --> C
  V --> S[Store and capability modules]
  H[Host lifecycle hooks] --> S
  S --> D[(SQLite state and records)]
  D --> F[Scoped fetch or operator views]
  S --> O[Delivery owner]
  O --> T[Native transport or context output]
  T --> A[Recipient tools]
  A --> V
```

CLI and MCP converge at `Store.call`; MCP does not execute the CLI dispatcher.
Pi bound tools launch the CLI. Host hooks also use lifecycle and delivery methods directly.

| Owner | Responsibility and contract |
| --- | --- |
| `src/catalog/` (Zod sources), `assets/catalog.json` (generated), `catalog.ts`, `validation.ts` | Command inputs, record payload schemas, tool profiles, discovery, and validation (patterns run as JavaScript `RegExp` with the `u` flag) |
| `message-content.ts` | Message-part and outcome validation, parsing, and runtime notices; message payloads do not imply side effects |
| `cli.ts`, `mcp.ts`, `main.ts` | Bounded ingress, command routing, output framing, and bound MCP identity; managed MCP owns its lifecycle |
| `tool-groups.ts` | Five typed capability families, operation selection and executable continuation projection; reuses catalog schemas and store behavior |
| `mcp-binding.ts` | Pending, borrowed and managed connection lifecycle; exact host-session resolution is atomic with hook registration |
| `mcp-instructions.ts` | MCP initialization guidance from the actual binding, lifecycle mode and selected tools; no CLI setup or skill loading required |
| `store.ts` and capability modules (`sessions.ts`, `peers.ts`, `leases.ts`, `documents.ts`, `completion.ts`, `expiry.ts`, `health.ts`, `retention.ts`, `activity.ts`) | Workspace/identity checks, current state, messaging, leases, documents, health, and scoped history. Modules are free functions over a host interface (`store-types.ts`); none imports `store.ts` |
| `assets/schema.sql`, `database.ts`, `database-export.ts` | Storage identity/fingerprint, no-clobber snapshots (`db export`), transactions (explicit `BEGIN IMMEDIATE`), constraints, indexes, and atomic event triggers |
| `paths.ts`, `workspace.ts`, `text.ts`, `json-text.ts`, `unicode16.ts` | Canonical paths; workspace identity (coordination scope, current branch, registration); code-point ordering, truncation, whitespace and UTF-8 bounds (`text.ts`); byte-stable JSON text and strict UTF-8 decoding (`json-text.ts`); lease keys folded with the frozen Unicode 16 tables in `assets/unicode16.json` (never ICU) |
| `registry.ts` | Per-process registry entry for a non-default `--database`, which the Pi extension reads ([format](docs/DB.md#active-database-registry)) |
| `records.ts` | Unified envelopes, visibility, indexed filters, fixed-ceiling pagination, and generic memory/event writes |
| `dispatch.ts`, `dispatch-outcomes.ts`, `delivery-types.ts`, `loop-control.ts`, `transport.ts`, `transport-codex.ts`, `native-io.ts`, `owner-lock.ts` | Attempt tokens and delivery ownership; offer outcomes, bounded retry and usage audit; signal-owning delivery loops; native I/O and transport-specific receipts; the OS-released owner lock (Unix socket; Windows named pipe, untested) |
| `proxy.ts`, `vendor-workers.ts`, `wire.ts` | Requested worker lifecycle, the Codex, Pi and Claude worker protocols, host process supervision over newline-delimited JSON, and the parent-death watchdog |
| `host-hooks.ts`, `src/hosts/` | Host events, context budgets, durable Pi receipts, and optional structured-edit admission |
| `view.ts`, `view-data.ts`, `assets/dashboard/` | Local read-only operator views and bundled UI; no frontend build |

The Zod modules in `src/catalog/` (shared `defs.ts`, `commands-<group>.ts` with each command's input and result, `record-types.ts`, `tools.ts`) are the one source of public field definitions. `yarn catalog` (and `yarn build`, before anything reads it) renders them with `z.toJSONSchema` into `assets/catalog.json`, which the runtime reads; never edit the JSON. Zod is a devDependency used only by that build step and the declaration generator input: no runtime source imports it. Constructs Zod cannot state (`if/then`, `not`, `dependentRequired`, `minProperties`, the CLI flag rules) ride as `.meta()` JSON Schema fragments, and `src/catalog/json-schema.ts` maps Zod's output to the catalog's dialect. `yarn lint` and `yarn test` fail when the committed JSON differs from what the sources generate (`tests/catalog-source.test.ts`). `tool-groups.ts` owns the tool profile presets. `assets/schema.sql` owns persisted structure and invariants.
Discovery combines these sources; no interface maintains a second command or record schema.
Context has one owner per rule: server instructions cover cross-tool order and trust; family descriptions identify the job; operation descriptions explain behavior and next steps; field notes explain values, units and defaults. Validation keywords carry structural constraints. Keep these layers complementary instead of repeating the same directions at each layer.
`src/catalog-types.ts` compiles the catalog with `json-schema-to-typescript` into `src/runtime/catalog-types.generated.ts`: one `<Command>Input` per command that takes input (`CommandInputs`, `CommandInput<N>`) and one `<Type>Data` per record type (`RecordData`). `yarn types` and `yarn build` regenerate the committed file, and `yarn lint` fails when it is stale (`node src/emit-types.ts --check`). The generator drops what a schema cannot state locally (`not`, `if/then`, `dependentRequired`, cross-field `oneOf`), so a type narrows a shape, not a validated value: `catalog.command(name, value)` validates and is the one place that returns the typed `CommandCall`; `Store.call` dispatches through a handler table typed by it.
The top-level `results` object of the catalog, keyed by command name, states what each command prints once its null members are stripped (`Store.call` and the CLI strip them). The generator turns each entry into a `<Command>Result` and emits `CommandResults`, so `Store.call`, the handlers and the Pi bridge return typed results; `catalog.ts` refuses to load a catalog in which a command lacks a result schema, `schema` carries the schemas, and `tools/list` and `schema <command>` do not (their bytes are unchanged). `tests/command-results.test.ts` validates every command's real result against its schema. A database row is a `Row` read through the typed `row*` readers in `database.ts`, and JSON read from a wire or a column is `unknown` until `validation.ts` narrows it; no shape is untyped.
`src/build.ts` inlines `assets/` and `OPERATING.md` into the bundle, so `dist/` needs no other directory, and reads the version from `package.json`; CLI and MCP version reporting use that release version. It fails when a catalog `pattern` does not compile as a JavaScript `RegExp` with the `u` flag. The same build then emits declarations with `tsc` (`src/emit-types.ts`, `tsconfig.types.json`): every `src/entries/<name>.ts` is `dist/<name>.js` and every `src/hosts/<name>.ts` is `dist/hosts/<name>.js`, each with a `dist/…/<name>.d.ts` stub that the package `exports` name as `types`; `yarn typecheck` checks `src/` and `tests/` with `tsgo --noEmit` and emits nothing; only the build writes declarations.

## Three representative flows

### Request and final reply

`CLI/MCP input → catalog validation → parts → Store.send → one write transaction → message, recipient snapshot, and record triggers → send receipt`.

Messages carry `parts`, optional `replyRequired` (default false), optional `contextId`, and explicit delivery intent. Routing accepts one peer, `"*"`, or a topic. Payload data is opaque: the runtime does not derive a task contract, copy recipients, rewrite evidence references, or transfer leases from message content.
`complete` with an explicit outcome and reply parts creates the correlated final answer and parent acknowledgement atomically. A silent completion acknowledges handled informational messages without creating a reply. A failed transition rolls back. Transport submission, recipient acknowledgement, and request outcome remain separate facts.

### Delivery and handling

`delivery owner → prepare → stage/token commit → host I/O → submitted or uncertain receipt → recipient complete`.

External I/O occurs outside the staging transaction. One-time submission and recipient acknowledgement are separate states.
Ambiguous/staged attempts require inspected recovery; expiry or restart does not authorize replay.
Hooks emit full envelopes or fetch references within the host budget. Deferred rows remain eligible for later events.
Pi reconciles receipt tokens with its durable host ledger before confirmation.

### Scoped history and operator audit

`bound fetch → read snapshot → repository/participant visibility → indexed query → resolved envelope → executable continuation`.

Message parts and metadata live once in immutable `messages`. The SQL `message_payload` view owns the full projection used by both `records.ts` and the search index; message audit rows retain only `messageId`.
The append-only `records` stream uses `{recordId,path,from,to,type,timestamp,branch?,data}`.
Operational tables enforce current presence, leases, recipients, replies, and attempts; hot inbox checks require no event replay.
FTS5 is derived searchable content. Generic JSON nulls remain intact; shared records retain `to:null`.
Participant fetch and workspace-wide operator dashboards/exports have different visibility boundaries.

## Preservation and compatibility

Peer discovery, messages, documents and shared records use the repository `coordinationScope`; leases, Git activity and native receiver checks use the actual worktree ([scope rules](docs/DB.md#identity-and-discovery)).

Session IDs route cooperating same-user processes; they are not credentials or filesystem isolation. `peers`, message senders and lease owners show exact IDs, and a CLI caller supplies its own `--session`, so any process that holds a peer's ID acts as that peer. A per-session secret would sit beside the ID: the database file, `db export` and process arguments are readable by the same OS user, so it would add no boundary and would need every host, hook and the Pi lease reader to carry it. The boundaries that hold are pinned by `tests/identity-trust.test.ts`: no tool input names the acting session (bound MCP and Pi tools fix it at launch), an ID acts only in its own workspace and only while live, and native receipt and write checks compare the host session. Isolation between mutually distrusting agents needs separate OS users or databases.
Leases use canonical paths and frozen Unicode case folding. They are advisory; guards cover only reported operations.

Schema v8 refuses earlier database files unchanged; it has no migration or overwrite path. Keep the old database and use its compatible binary to drain or export it, then use a fresh explicit `--database` for v8. The delivery-owner lock is not shared across earlier runtimes: older releases used a file lock and this runtime a socket, so run one runtime per database while upgrading ([DB.md](docs/DB.md#cleanup-and-conformance)). Other differences that surface to users: `pathRegex` is a JavaScript `RegExp`, whole-number floats such as `5.0` are integers, database paths must stay within 504 bytes after resolving symlinks, and Windows has no Job Object for a killed worker ([limits](README.md#requirements-and-limits)).

The runtime opens only the exact current schema fingerprint and leaves any other database unchanged ([database identity](docs/DB.md#identity-and-discovery)).

## Documentation and verification

[README.md](README.md#documentation-map) maps every guide to its reader. [OPERATING.md](OPERATING.md) is the runtime operating guide: `skill` returns it and managed workers receive it, so edit it there only. `db protocol` serves [DB.md](docs/DB.md) verbatim.

The package manifest owns build, typecheck, regression and smoke commands; the repo-root `docs:verify` checks Markdown links and anchors. `src/release-smoke.ts` exercises real CLI/MCP replies, leases, export and restore against the packed package. Crash tests exercise real transaction and process boundaries; the opt-in `src/live.ts` harness (graders, worker mechanics and options in `src/live/`) runs real Pi, Claude and Codex workers (single-host trials and a three-host soak across request waves) ([operations](docs/OPERATIONS.md#verify-concurrent-native-workers)). Checks prove their named paths on the local platform, not arbitrary vendor versions or OS isolation.

## npm interfaces

Routes, input forms and lifecycle: [entry points](docs/INTERFACES.md). `octocode-mcp-cli` (bundled at build time, not a published dependency) supplies CLI validation, typed flags and help to both the `/cli` route and the `./cli` subpath. Bound MCP exposes five typed capability families; profiles reduce the enabled variants. An unbound connection exposes only `init`, then signals a discovery change. `tool-groups.ts` projects operation schemas from the command catalog and dispatches through the same validation/store path as the CLI. Single and multiple reservations both use the atomic path-set operation. Runtime-owned continuations are projected to callable family routes; user evidence is not recursively rewritten. Lower-level CLI operations remain available to hooks and operators.

`initialize.instructions` carries cross-tool workflow and trust rules plus only the selected tool routes. It distinguishes maintained and borrowed identities and explicitly states that stdio MCP cannot push mail or wake a host. The package borrows terminology and ideas from [A2A v1.0](https://a2a-protocol.org/v1.0.0/specification/): requests remain messages, recipients report outcomes in correlated messages, and named documents hold reusable artifacts. It exposes no separate Task or Artifact API and does not implement A2A's remote protocol, authentication, or AgentCard. `tests/tool-selection.test.ts` measures discovery size; it is a context regression check, never an output truncation limit.
