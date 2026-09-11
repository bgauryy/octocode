# How Octocode Awareness works

This is the canonical end-to-end lifecycle for Awareness. It owns how an agent enters
through host instructions, uses the native API or CLI to inspect and change live state,
receives hook automation, verifies work, records learning,
and exits or hands off. Command recipes live in [SKILLS.md](SKILLS.md); host wiring
lives in [HOOKS.md](HOOKS.md); schema detail lives in [DB.md](DB.md).

Awareness is a coordination runtime over an Awareness-only SQLite database. It
defaults to `$OCTOCODE_HOME/awareness/awareness.sqlite3`; explicit repository
scope uses `<workspace>/.octocode/awareness.sqlite3` through policy or
`--db-scope repo`. Agent
databases under `$OCTOCODE_HOME/agent/` have separate ownership and identities.
No server or broker is required. See [storage scopes](STORAGE_SCOPES.md).

## Authority chain

```text
Host starts
  -> AGENTS.md / CLAUDE.md        (entry + router; short and always loaded)
  -> Agent Skills                (policy + judgment; loaded when task matches)
  -> Awareness CLI / library     (control plane + executable contracts)
  `-> $OCTOCODE_HOME/awareness/ -> awareness.sqlite3
       |                         (plans, tasks, work, locks, checks, coordination)
       `-> <workspace>/.octocode/ (explicit repo store, exports, plan documents)

$OCTOCODE_HOME/agent/ ----------> separate Agent control/runtime stores and artifacts

Host hooks ---------------------> same package contracts and selected store (edge automation)
```

Each layer has one job:

1. **`AGENTS.md` is the starting point.** It tells every repository agent to activate
   Awareness for non-trivial work, identifies package-specific guides, and links here.
   It routes; it does not duplicate the operating manual or command inventory.
2. **Agent Skills own judgment.** `octocode-awareness` decides when to attend, plan,
   claim, declare, coordinate, lock, verify, remember, clean, or project.
   Other workflow skills can add focused research, evaluation, or skill-lifecycle
   guidance when installed separately. Skills do not own live coordination state.
3. **The Awareness package owns the shared control plane.** Native tools call
   `executeAwarenessCommand`; the CLI uses the same executor. It creates and changes plans, tasks, runs, file presence, locks, verification,
   signals, refinements, Awareness-specific session captures, memory,
   maintenance, and queries (with optional read-only exports). CLI help and JSON
   schemas own exact flags and payloads. See the [API reference](API.md).
4. **Host hooks automate deterministic edges.** They call the same library used by
   the CLI. They can register sessions, declare writes, heartbeat,
   roll back failed writes, finalize fallback runs, deliver changed context, and
   surface verification debt. They never choose goals, create a plan, decide a lock
   is warranted, mark tests successful, or turn memory into authority.
5. **Human docs and explicit exports are read surfaces.** Authored plan documents explain
   intent. Generated output files are bounded snapshots; they are never canonical state.

Authority descends from current user instructions and current source/tests, to live
SQLite state and fresh command evidence, to verified memory/signals, and finally to
read-only query exports. A lower layer cannot override a higher one.

Rows are isolated by normalized `workspace_path` and optional artifact/repo/ref
scope within the selected Awareness database.

The default flow is one peer briefing plus useful communication. Plans, work,
locks, verification, memory, history, and maintenance are available on demand.
`schema commands --compact` groups core and advanced nouns; `schema command <noun> [action]`
returns one action contract with router-injected fields removed. Locks are normally
requested through `work start --exclusive`. Raw lock, hook, maintenance, refinement, session, docs, and schema
commands remain available when the lifecycle requires them.

## Agent decision contract

Attend once per workspace/session, or reuse a host-provided briefing. Default
`attend` reads only registered peers, with bounded pages and executable continuations.
It does not query work, memory, or verification. Registration and last-seen timestamps
do not prove that a process is live. Inspect a message when it changes the next action;
unchanged or non-actionable state requires no extra coordination call.
Select the store and stable identity before reading shared state. An explicit
workspace selects isolation; cwd supplies repository context.

Explicit `attend --details` or task/file filters select the detailed observer.
Its `attend.next` includes advisory guidance. In CLI output:

```text
{ action, reason, target?, command?: { name, args } }
```

The API converts suggested commands to `{ command, params }` requests and Pi
wraps them as native tool calls. In CLI output, `name` and `args` identify a canonical operation without shell
interpolation. When a durable command is safe to suggest, its arguments retain the
selected database, workspace, agent, and artifact. Advice remains advisory and
read-first:

- `verify_owned_work` points to `verify audit` and the relevant run; it never marks
  verification.
- `inspect_lock` and `inspect_overlap` point to `work show`; `inspect_inbox` points
  to `signal list`.
- `resume_owned_task` returns the owned task/run IDs without heartbeating; a ready
  task points to `task show`, not an automatic claim.
- `continue` has no command. Memory and runtime pressure return bounded guidance.

In-memory stores expose an action and target but omit a misleading subprocess
command. Re-observe after a material state change, not on a timer. A command result
or declared check receipt proves only its stated fact; a signal, memory, expiry, or
receipt assertion remains evidence to inspect.

## Bootstrap lifecycle

```text
HOST INTEGRATION -> STABLE IDENTITY -> ATTEND ONCE -> USEFUL COMMUNICATION
```

1. Install the package and the `octocode-awareness` skill for the host.
2. Select the shared store. Commands open/check it as needed; `maintenance init`
   is an explicit initialization diagnostic, not a per-session requirement.
3. Set one stable `OCTOCODE_AGENT_ID` for the main agent. Host-provided child IDs keep
   subagents distinct while the parent CLI and hooks share one identity.
4. Choose one hook surface: Claude skill frontmatter or Claude settings, or
   Codex/Cursor project settings. Never install both Claude surfaces.
5. Preview configuration writes, install after approval, then run strict config
   health. Strict success proves exact entries and existing script targets, not that
   the host executed them or delivered context.
6. Smoke peer registration and message delivery on the real host. For guard/full,
   also check write, failure, stop, and supported compaction boundaries. Only runtime
   evidence upgrades configuration health to operational trust.

Pi already supplies native lifecycle events and the Awareness tool; it needs no
shell-hook install. External hosts can install the default coordination profile
or read the inbox explicitly when hooks are absent. Missing global feature
configuration uses lean defaults. See [configuration](CONFIGURATION.md).

## Homeostatic control model

Awareness is a supervised software control loop, not an autonomous agent. It
senses operational pressure in SQLite and hooks, compares that evidence with
bounded targets, recommends an actuator, and preserves human/agent choice at the
guard. Typical corrections are `attend --compact`, declaring file presence,
resolving a signal, verifying a run, previewing maintenance, or exporting a
query snapshot. Re-measurement closes the loop; unchanged state should inject no new
prompt text.

“Living repository” is a useful systems metaphor for continuous sensing,
adaptation, forgetting, and repair. It does not imply sentience, self-chosen goals,
network coordination, or permission to mutate code/instructions. The complete
pressure table and success measures live in [THESIS.md](THESIS.md).

## Durable work model

```text
Plan -> Task -> TaskRun -> RunFile
                        `-> Lock

Standalone WORK -> TaskRun(origin=WORK) -> RunFile / optional Lock
Hook fallback -> TaskRun(origin=HOOK) -> RunFile -> PENDING
```

| Entity | Meaning |
|---|---|
| Plan | Shared objective, lead, members, lifecycle, managed documents. |
| Task | Durable selectable work with reasoning, acceptance, paths, priority, dependencies. |
| TaskRun | One attempt and its verification contract. |
| RunFile | Advisory path presence for tracked work; many agents may share a path. |
| Lock | Optional exclusive protection for sensitive work. |
| EditLog | Completed edit event history. |

Tasks are the only shared backlog. Plan documents explain objective and decisions;
they never copy live task status into a second “today” list.

## Lifecycle

```text
ATTEND ONCE -> DO THE AUTHORIZED WORK -> COMMUNICATE WHEN NEEDED
  -> SAVE A REUSABLE LESSON OR HANDOFF ONLY WHEN WARRANTED
```

1. Reuse the host briefing or call `attend` once. Keep a stable workspace and identity.
2. Continue the authorized task. Discover a capability only when it can change the
   work: shared ownership, a meaningful overlap, an unsafe concurrent write, or recovery.
3. Read and reply to relevant peer messages. Preserve thread and sender IDs;
   acknowledge handled signals and resolve conversations only when finished.
4. After substantial work or a meaningful event, record a concise verified lesson
   only if it is reusable. Use a handoff only when someone must continue unfinished work.

### When work needs tracking

For a shared task or explicit standalone WORK, reuse existing task/run IDs. Declare
the affected paths and check plan, then edit and run the required checks while
presence remains active. Guard/full hooks can automate recognized file presence;
the default coordination profile does not create it.

`task submit` or `work end` moves the run to `PENDING`. Record the actual check
result through `verify mark`, then inspect `verify audit` after final writes.
A failed check stays `FAILED`; an unrun check remains pending. Settle or disclose
owned debt and release owned leases without changing peers' records. Expiry and
session exit never prove success. Ordinary overlap is advisory; use an exclusive
lock only where concurrent writes cannot be merged safely.

Host sessions are not work-unit boundaries. Only a task claim or explicit
`work start` may reuse an explicit standalone WORK run; fallback hook writes remain
isolated.

## Entity lifecycles

| Entity | Lifecycle | Invariant |
|---|---|---|
| Plan | `DRAFT -> ACTIVE <-> PAUSED -> COMPLETED or CANCELLED` | The lead owns transitions; completion waits for active work to resolve. |
| Task | `OPEN -> IN_PROGRESS -> VERIFY -> DONE or FAILED`; side paths `BLOCKED`/`CANCELLED` | “Ready” is derived from ACTIVE plan + satisfied dependencies + no live claim. |
| Run | `ACTIVE -> PENDING -> SUCCESS or FAILED` | Ending edits creates debt; only verification writes a terminal result. |
| RunFile | declared/active -> heartbeat/extend -> ended or expired | Tracked file presence is advisory; it is not a lock. |
| Lock | acquire -> renew -> release, expiry, or prune | Only `EXCLUSIVE`; reserved for sensitive work and attached to a run. |
| Signal | publish -> deliver/read/ack -> resolve -> optional prune | Messages are coordination evidence, not authority or a task queue. |
| Refinement | `open -> ongoing -> done` | Owned repo-fix follow-up; terminal closure requires a check receipt. Session handoffs are broadcast `kind=handoff` signals, not refinements. |
| Memory | record `ACTIVE` -> supersede/expire/archive -> optional restore or reviewed forget | Recall is a ranked lead; replacement history is immutable. |
| Awareness session capture | register/start -> prompts/turns -> compact capture -> shutdown/end | This is coordination context, not an Agent runtime session; PreCompact preserves it and end marks it inactive without success. |
| Query export | `query --format html/json/csv` writes a read-only `.octocode/` snapshot on request | SQLite is canonical; exports are never auto-generated and never read back as state; authored plan docs are preserved. |

Task, WORK, and HOOK are run origins, not interchangeable queues:

- `TASK` is a claimed durable plan task.
- `WORK` is explicit standalone work with rationale/files/test plan.
- `HOOK` is automatic fallback presence when a structured write has no task or WORK
  owner. A successful write remains active until a lifecycle boundary; a failed write
  discards only uncommitted HOOK presence.

## Hooks

The default coordination profile registers peers, delivers changed messages, and
ends session presence. It does not create per-edit work records, audit at every
stop, or recall memory. Guard/full opt into mutation bookkeeping; full adds
history and compaction boundaries. Verification reminders and session captures
also require their global feature switches. The following edges apply when enabled:

```text
SessionStart / prompt -> register + changed briefing
PreToolUse(write)     -> guard + resolve owner + declare presence + conflict check
PostToolUse(success)  -> heartbeat + edit log
PostToolUse(failure)  -> remove uncommitted HOOK presence; preserve TASK/WORK
SubagentStart         -> distinct child identity + context where supported
Stop/SubagentStop     -> finalize HOOK fallback + audit verification debt
PreCompact            -> finalize/capture but keep session reusable
SessionEnd/shutdown   -> finalizes/captures and marks the session ended, never success
```

Normal success is silent. Changed peer/message fingerprints emit one bounded message packet;
unchanged state emits nothing. An exclusive conflict blocks before presence. Prompt
briefing reads peer messages without retrieving memory or refinement state.
Enabled stop reminders are count-only.

### Manual CLI and hook parity

| Need | Hook/host automation | Manual control-plane equivalent |
|---|---|---|
| Enter/orient | session/prompt registration and changed briefing | `attend`, `agent register`, targeted reads |
| Declare write | pre-edit presence and exclusivity check | `work start|touch`; add `--exclusive` when required |
| Successful write | heartbeat and edit audit | keep work active; record/check through the owning run |
| Failed write | discard uncommitted HOOK presence | preserve or explicitly end/release TASK/WORK after judgment |
| Conclude editing | Stop/compact/end finalizes HOOK fallback | `task submit` or `work end` |
| Prove success | reminder/audit only | run check, `verify mark`, `verify audit` |
| Handoff/exit | compact/end capture | signal, refinement, or `session capture` |

Reuse a host briefing instead of repeating `attend`. Hooks never replace plan/task choice, deliberate exclusivity, verification
receipts, memory judgment, cleanup approval, or query-export requests.

Host wiring details live in [HOOKS.md](HOOKS.md).

## Peer event delivery

Publishing a signal and its `peer.message` outbox event is one database transaction.
An in-process host drains that outbox through `createAwarenessEventConsumer` with a
stable workspace-scoped consumer ID and the receiving agent ID. Each drain is
serialized and bounded. The consumer validates the event envelope, workspace,
actor, aggregate, target, provenance, expiry, and body before delivery.

The inbound policy accepts routine questions, requests, decisions, blockers and
handoffs as attributed peer data. Typed `approval` signals and untyped legacy
`APPROVAL`/`PROPOSAL` topics are held for human authorization. Typed messages are
not classified by words such as "permission" in their body. Directed requests,
questions, blockers and handoffs are actionable for a bounded host wake;
broadcasts do not wake every recipient. No peer signal grants authority.
Refused or malformed events are
acknowledged as refused. Accepted messages are marked read for the recipient only
after host delivery succeeds; the durable event acknowledgement then advances the
consumer cursor in sequence. A delivery or acknowledgement error stops that drain
at the failed event so a later drain can recover without skipping it. Transport
acknowledgements and signal read receipts remain separate records.

The host owns lifecycle drains and wake-ups. Pi drains at session start and agent
completion after creating a persistent session. It also watches the selected
database, WAL, and directory for change hints, then performs the same authoritative
drain. Watch/read failures retry quickly and settle on a sparse 30-second recovery
interval; lifecycle drains remain the fallback. Hosts without this watcher need a
lifecycle opportunity or explicit inbox read after an idle arrival. Delivery does
not imply handling: `signal ack` records handling, and `signal resolve` with
`thread_id` closes the finished conversation.

## Context model

Persist everything needed for coordination; prompt only actionable changes:

- ordinary edit: zero injected awareness text;
- remembered state: no automatic recall; inspect it when prior learning can change the approach;
- changed overlap: affected path summary only; inspect peers and ownership explicitly;
- exclusive conflict: holder, reason, expiry, recovery action;
- default attend: bounded registry presence;
- detailed state: explicit `attend --details`, `work show`, query, or recall.

This separates database completeness from token cost.

## Knowledge and memory

Memory is durable verified learning, not routine status. Signals are typed peer
messages. Refinements are owned follow-up/handoff state, not another task queue.

The memory lifecycle is deliberately conservative:

1. Recall only when prior learning can change the approach; task and scope filters narrow candidates.
2. Smart widening may relax low-value filters and reports what changed under
   `--explain`; semantic reranking is optional and safely falls back to lexical FTS.
3. Treat every hit as a lead and re-check current source/tests/output.
4. After substantial work or a meaningful event, record only a scoped, reusable,
   evidence-backed lesson, decision, gotcha, or source. Skip routine edits and repeated facts.
5. Correct facts with `--supersedes`; archive reversibly; hard-forget only after a
   narrow dry-run and review.

`query <view>` reads the live DB. Explicit query exports provide
CSV/HTML when requested (`query --format html/json/csv`). SQLite is canonical;
current source/tests/user instructions always win.

## Completion contract

For explicitly tracked work, completion requires:

- no required edited path lacks declared ownership;
- no unresolved exclusive conflict was bypassed;
- editing has ended through the owning task/WORK/HOOK lifecycle;
- the declared verification ran and its result was recorded;
- `verify audit` shows no unintended debt for the agent/scope;
- necessary peer threads or handoffs are resolved or explicitly owned;
- reusable learning was recorded only when warranted;
- cleanup was previewed and applied only when due.

## Boundaries

- Awareness owns coordination, memory, verification, hooks, and read-only query exports.
- `npx octocode` or Octocode MCP owns code/GitHub/package research and skill
  install/review operations.
- Harness proposals never self-apply. A human/user authorizes source or instruction
  changes, and normal verification still applies.

Schema detail: [DB.md](DB.md). File-work semantics: [LOCKS.md](LOCKS.md). User
recipes: [SKILLS.md](SKILLS.md). Research and prior-art boundaries:
[REFERENCES.md](REFERENCES.md).
