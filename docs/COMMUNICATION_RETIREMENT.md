# Communication runtime and Awareness retirement

## Ownership

`@octocodeai/octocode-agents-communication` owns shared session identities, advisory
path leases, messages, subscriptions, dispatch receipts and their SQLite audit.
Any vendor can use its skill and Rust CLI; a conforming SQLite client is the fallback.

The Awareness package, CLI, installed repository skill and repository hook
registrations are removed. Its duplicate plan projections, work/verification
ledger, shared memory and automatic history capture are retired. `/rewind` depended
on that history store and is removed.

Existing Awareness SQLite files and `.octocode/.localGit` archives are left on
disk. They are not imported into communication or the new Pi interaction store.
The removed package's local `.octocode` directory was moved to
`.octocode/retired/octocode-awareness-2026-09-25/data`; all 143 files were checked
against their pre-move SHA-256 digests recorded in `preservation.json`.
Old pending interactions and authorization receipts are not transferred: reissue
unfinished prompts in the current session. Do not reuse an old database path for
either new store. Repository removal does not uninstall other checkouts or global
skill installations.

## Context and trigger policy

| Event | Host work | Model work |
| --- | --- | --- |
| Session opens | Register/resume identity; expose stable tool catalog; establish presence | Identity/workflow once in retained context |
| Heartbeat or idle polling | Renew presence and inspect new delivery state | None |
| Passive message or default broadcast | Store once with intent and audit | Wait for the next authorized turn in managed workers |
| Action message | Stage a bounded batch, including queued notices | One managed turn for the batch; peer text grants no new authority |
| Attached Pi receives a message | Persist custom-message receipt before confirming dispatch | No automatic turn; next user/host-authorized turn handles it |
| Message handled | Explicit acknowledgement | No acknowledgement reply unless requested |
| Edit/delete targets a leased path | Check current owner, path overlap and expiry | Ask for handoff or choose independent work; reads stay available |
| Measured context pressure or repeated failure | Pi retains its local host controls | A relevant intervention; no model polling for telemetry |

Use `wake: "passive"` for informational direct messages and `wake: "action"` for
requests that need a managed turn. Broadcasts default to passive. Wake intent is
scheduling metadata, not authority. Native attached hosts control whether their
transport starts a turn; the managed-worker policy does not override vendor APIs.
Pi can keep a fresh session only in memory until its first assistant message.
Until a receipt exists in its actual session file, the transport remains staged.
If that process exits first, the DB message remains unacknowledged and recoverable;
resuming a nonexistent session file does not guarantee the same Pi identity.

Keep task-invariant instructions and tool definitions stable. Append only new
message IDs, sender, brief intent and content; put large evidence in workspace
communication documents. Query activity only when recent files or Git events
would change the next decision. File status is a current observation, not a full
history of edits or shell commands.

Provider prompt caches reward a stable prefix, but cached history still occupies
context. The communication runtime cannot configure undocumented Codex/Claude CLI
cache controls. See [OpenAI caching](https://developers.openai.com/api/docs/guides/prompt-caching)
and [Claude caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
for API contracts; those controls are not automatically CLI options.

## Database and verification

Only the current development schema is supported. Use a fresh database after schema
changes; see the [database protocol](../skills/octocode-agents-communication/scripts/docs/DB.md).

The previous six-worker activity exercise reported 93% cached cumulative input.
Two acknowledgement-only turns accounted for 73,482 input tokens (69,632 cached),
showing a concrete reason to avoid unnecessary turns. Those are observed prior
turns, not a claimed savings estimate or a controlled before/after benchmark.

Current regression evidence is recorded under
`.octocode/octocode-eval-benchmark/communication-retirement/`. Count actual turn
starts/completions separately from DB messages and diagnostic events. Report
application-injected bytes separately from provider input, cache and output usage;
never sum cumulative Codex usage snapshots.

## Observed results — 2026-09-25

- Two Codex Luna, two Claude Haiku and two Pi Haiku managed workers received a
  passive broadcast with zero additional model turns or provider usage during
  the four-second observation window. A subsequent action request reached each
  worker with the queued notice in exactly one batch. All six replied once and
  acknowledged both IDs; the six batches completed in 10.539 seconds wall time.
- Real Pi SDK restart probes passed 20 assertions across fresh and established
  sessions, with zero model calls. They exposed and verified the fix for premature
  delivery confirmation while the session existed only in memory.
- Communication validation passed 74 JavaScript tests and 20 Rust tests, including
  lease conflict/expiry, wake scheduling, schema rejection and receipt recovery checks.
  Release build, formatting and Clippy passed on the local macOS ARM64 host.
- Pi's normal test command passed all 2,245 tests in 208 files with its default
  worker and timeout settings. Build, type checking, lint, documentation
  verification and workspace dependency checks passed. The bundled communication
  CLI exposed the current wake/reasoning contract successfully.

The subsequent native rebuild and dependency refresh resolved the core/native
contract mismatch. Root and Pi-bundled CLI smoke checks, a real local search, and
immutable dependency installation now pass without a drift bypass. The default
communication DB path does not yet exist on the checked host; no default user DB
was changed. Existing stores must match the current development schema.

These results test delivery and scheduling behavior, not universal exactly-once
execution: uncertain transport outcomes still require inspection before retry.
They do not establish an overall production token reduction. The skill is 43 lines and
7,632 bytes; Pi's 13 additional communication schemas cost 9,584 serialized
characters, separately budgeted from its existing tool schemas. Stable schemas
can be cached, but remain context.

A follow-up matched benchmark
ran 12 workers under both wake policies. Handling turns fell from two to one in
every pair; startup-adjusted input fell 40.2% for Codex, 60.0% for Claude and 49.6%
for Pi, including cached input. Completion latency increased for all three
vendors. This is a small exploratory messaging comparison, not a general cost or
speed guarantee. Both the initial failed behavior run and the corrected run remain
available; the reusable telemetry/preflight checks subsequently passed six tests.

Follow-up lifecycle fixes prevent delivery after shutdown, reject stale or revoked
Pi contexts, and cancel delayed registration without leaving an active identity.
Mutation checks now validate the bound session/workspace. The real Pi restart
probe was rerun against the final adapter: 20 checks passed across four processes,
all exited normally, and no model turn started. Evidence is under
`.octocode/benchmarks/pi-durability/results/2026-09-25-lifecycle/`.

Focused follow-up checks cover shell copy target-directory options, move/delete
source paths and permitted reads of leased sources. The full suite passed before
the final copy-option cases; those changes passed a separate 31-test run.
