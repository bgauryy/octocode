# Awareness architecture

`@octocodeai/octocode-awareness` owns local multi-agent coordination, reusable memory, verification receipts, host lifecycle integration, and recoverable file evidence. It does not own code research, host UI, model execution, or Agent control databases.

## Public boundaries

Awareness exposes one routine operation model through both the CLI and `createAwarenessClient`:

```text
Context -> Work -> Message -> Memory -> History
   3         8         4          5          5
```

The package root has eight runtime exports: the client factory, two operation catalogs, two descriptor lookups, the agent instruction section catalog, the agent instruction builder, and the shared Message parameter guidance. Side-effect-free discovery is also available from `/schema`. Host integration is isolated under `/host`; copy-on-write database migration and whole-store retirement are isolated under `/admin`.

The CLI parses shell input and renders results. It does not own domain behavior. Operation descriptors validate parameters, select one domain route, execute it with trusted host bindings, canonicalize continuations, and enforce output budgets. Unknown operations fail instead of entering a compatibility dispatcher.

## Layer ownership

```text
CLI or host-bound client
  -> canonical operation descriptor
  -> domain owner
  -> SQLite transaction
  -> compact result with executable continuations

host lifecycle event
  -> /host adapter
  -> delivery, protection, receipt, or History capture owner
  -> the same SQLite store and optional LocalGit object store

database predecessor
  -> /admin preview
  -> copy-on-write destination
  -> independent verification
  -> explicit cutover with retained rollback source

operator maintenance
  -> dry report from domain-owned policies
  -> exact confirmation
  -> bounded atomic page with executable continuation

store retirement
  -> bound report and active-state checks
  -> exclusive writer fence
  -> recoverable quarantine of exact SQLite and LocalGit targets
```

The domain owners are:

- Work: plans, tasks, attempts, dependencies, file presence, exceptional protection, and verification.
- Message: addressed threads, replies, delivery state, and resolution.
- Memory: scoped evidence-linked lessons, keyed revisions with caller rationale, and typed anchors. Knowledge reuses the memory relations; native fingerprints assess declared file applicability without certifying a lesson.
- History: SQLite metadata and private LocalGit objects for exact file bytes and non-file experience evidence. Experience events and archive receipts remain in the existing event outbox and are excluded from ordinary delivery pruning.
- Context: attributed observations and intervention feedback, plus a bounded projection over decision-changing state. Observations and feedback use the existing event outbox; assessment does not create a second source of truth.

SQL stays with the module that owns each relation. Interface code must not duplicate schemas, query strings, lifecycle policy, or history writers.

## Lifecycle and cleanup

Every canonical entity declares its semantic owner, retention class, deletion semantics, and executable cleanup operation. `maintenance retention` composes Message, event, interaction, lock, run, Memory, and restore-preview owners. Reporting is read-only and aggregate; apply requires `--confirm apply-retention`, limits each owner to a deterministic batch, and returns a repeatable continuation with a fixed cutoff when more work remains. Stale ACTIVE run recovery is visible in reports but mutates only with the explicit `--fail-stale-active-runs` option.

Messages receive a non-null expiry from one per-kind policy when written. Expiry first resolves open messages; physical deletion waits for the grace cutoff and preserves a parent while any retained descendant still depends on it. Messages are coordination state, not durable Memory.

`maintenance store-retire` is separate from row retention. Its default report binds the store identity, workspaces, exact targets, and blockers. Confirmed apply refuses live lifecycle state or another SQLite writer, then renames the database, sidecars, and derived LocalGit roots into recoverable sibling quarantine paths. It never recursively targets a workspace, home directory, filesystem root, or shared `.localGit` parent.

## Agent-independent regulation

Any agent can submit measurements with `context.observe`, read assessed state and advisory actions with `context.orient`, and report its response with `context.feedback`. CLI and imported clients use the same validated operations. The Context domain owns observation validation, assessment, and feedback evidence; the host owns measurement, execution, compaction, and model selection.

Observations and feedback are attributed to the bound workspace, actor, and session. They reuse `event_outbox` without changing database DDL or introducing a second telemetry store. Missing or expired measurements remain unavailable; they do not indicate recovery failure. Advice is advisory, and reporting an action is not proof that it improved an outcome.

Reusable agent instructions are package-owned exports. Hosts import the instruction builder instead of duplicating operating guidance; CLI discovery and bundled skills direct agents to the same contract.

## Trust and evidence

SQLite is canonical for coordination state. Source files are canonical for current code. Observed check receipts are canonical for verification. LocalGit is immutable recovery evidence. These sources answer different questions and cannot substitute for one another.

Keep the following evidence levels separate:

1. An implemented contract exists in source and tests.
2. A host configuration selects an integration.
3. A running host invokes that integration.
4. The selected store contains the resulting receipt.

A passing adapter test establishes the first level, not universal host activation. Likewise, presence, a message, an expired lease, or captured bytes do not prove work completion.

## Storage identity

The physical SQLite path and normalized workspace identity define a coordination boundary. Linked Git worktrees can share repository-scoped discovery in one store while retaining physical workspace keys for protection, authorization, verification, and recovery. Unrelated clones or databases do not coordinate automatically.

The default global store is `$OCTOCODE_HOME/awareness/awareness-v<schema-version>.sqlite3`. Repository scope uses `<workspace>/.octocode/awareness-v<schema-version>.sqlite3`. The current generation is `v5`. One generation constant owns both the metadata version and filename, so a breaking DDL change selects a fresh default store instead of opening an older generation. An explicit database path has the highest precedence for a call and remains subject to exact fingerprint checks. Scope or generation changes never merge databases implicitly.

Opening a store accepts the exact canonical fingerprint. Migration code recognizes only explicit predecessor contracts and writes a different destination file. It verifies schema, integrity, foreign keys, counts, event order, and available LocalGit reachability before an operator selects the destination. The source remains available for rollback.

## LocalGit

History metadata lives in SQLite. File bytes live in a private store under `<workspace>/.octocode/.localGit`, partitioned by store and physical workspace identity. `history status` reports resolved storage paths; callers must not construct them.

File capture is a host responsibility exposed through `/host`. Routine file operations report status, list a bounded timeline, read one version, and preview or apply one bound restore. `history.experience` also records attributed investigation events without requiring source changes. Sealing freezes the canonical SQLite trace before publishing an optional immutable archive under its own LocalGit ref namespace; failed archival can be retried. The LocalGit backend does not change the repository index, `HEAD`, branches, remotes, or hooks.

Restore is deliberately not proof of correctness. Preview binds the intended operation, side, paths, digests, and expiry. Apply rechecks those bindings, acquires protection, records undo evidence, and returns verification debt.

## Host lifecycle

Workspace policy selects a hook profile and exactly one owner, `native` or `shell`, for each host. Pi claims native ownership. When policy marks a host native-owned, the shell runner becomes inert before opening a database or writing a receipt.

Native and shell adapters translate host events into the same domain owners. They do not infer goals, authorize edits, mark checks successful, or create routine model commands. Hook infrastructure errors record degraded receipts and fail open; an active exclusive protection remains a real admission failure.

Ordered event consumers acknowledge only after the host persistence boundary accepts delivery. Database and WAL watchers provide wake hints, not message bodies or authoritative state. The next drain re-reads SQLite.

## Dependency rules

- Do not import the Agent runtime, Pi UI, or model execution policy into Awareness.
- Keep `bin/` responsible for argv, stdout, stderr, and process exit only.
- Keep routine discovery limited to the canonical operation catalog.
- Keep host-only capabilities under `/host` and migration under `/admin`.
- Treat executable continuations as part of every bounded result contract.
- Emit stable, self-contained runtime entries so a rebuild cannot strand a live host on a removed lazy chunk.
- Treat ordinary overlap as advisory and exclusive protection as exceptional.
- Record only observed verification results.
- Build the package before verifying its CLI, subpaths, or generated skill assets.

For deeper references, see [the API](docs/API.md), [database ownership](docs/DB.md), [host hooks](docs/HOOKS.md), and [LocalGit history](docs/LOCAL_HISTORY.md).
