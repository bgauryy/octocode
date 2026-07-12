# How Octocode Awareness Works

This is the canonical end-to-end lifecycle for Awareness. It owns how an agent enters
through `AGENTS.md`, activates skills, uses the CLI to create and mutate live state,
receives hook automation, verifies work, records learning, publishes projections,
and exits or hands off. Command recipes live in [SKILLS.md](SKILLS.md); host wiring
lives in [HOOKS.md](HOOKS.md); schema detail lives in [DB.md](DB.md).

Awareness is a coordination runtime over one local SQLite database. Every host and
surface uses the same state; no server or broker is required.

## Starting Point And Authority Chain

```text
Host starts
  -> AGENTS.md / CLAUDE.md        (entry + router; short and always loaded)
  -> Agent Skills                (policy + judgment; loaded when task matches)
  -> Awareness CLI / library     (control plane + executable contracts)
  -> awareness.sqlite3           (canonical live state)
       |-> attend / targeted query / workboard
       |-> plans, tasks, runs, files, locks, verification
       |-> signals, refinements, sessions, memory
       `-> optional .octocode/ projections

Hooks / Pi bridge --------------> same library and SQLite (edge automation)
```

Each layer has one job:

1. **`AGENTS.md` is the starting point.** It tells every repository agent to activate
   Awareness for non-trivial work, identifies package-specific guides, and links here.
   It routes; it does not duplicate the operating manual or command inventory.
2. **Agent Skills own judgment.** `octocode-awareness` decides when to attend, plan,
   claim, declare, coordinate, lock, verify, remember, clean, or project.
   `octocode-research` owns code/repository evidence. Skills do not own live
   coordination state.
3. **The Awareness CLI is the only agent-facing control plane for durable Awareness state.**
   It creates and changes plans, tasks, runs, file presence, locks, verification,
   signals, refinements, sessions, memory, maintenance, queries, and projections.
   CLI help and JSON schemas own exact flags and payloads.
4. **Hooks and the Pi bridge automate deterministic edges.** They call the same
   library used by the CLI. They can register sessions, declare writes, heartbeat,
   roll back failed writes, finalize fallback runs, deliver changed context, and
   surface verification debt. They never choose goals, create a plan, decide a lock
   is warranted, mark tests successful, or turn memory into authority.
5. **Human docs and `.octocode/` are read surfaces.** Authored plan documents explain
   intent. Generated AGENTS/KNOWLEDGE/manifest output is a bounded snapshot that
   routes readers back to live commands; it is never canonical state.

Authority descends from current user instructions and current source/tests, to live
SQLite state and fresh command evidence, to verified memory/signals, and finally to
generated projections. A lower layer cannot override a higher one.

Rows are isolated by normalized `workspace_path` and optional artifact/repo/ref scope.

The default agent surface is deliberately small: `attend`, `plan`, `task`, `work`,
`verify`, `memory`, `signal`, `wiki`, and `query`. `schema commands --compact` groups
these before advanced recovery/diagnostic nouns; `schema command <noun> [action]`
returns one action contract with router-injected fields removed. Locks are normally
requested through `work start --exclusive`, and generated knowledge is refreshed
through `wiki sync`. Raw lock, hook, maintenance, refinement, session, docs, and schema
commands remain available when the lifecycle requires them.

## Bootstrap Lifecycle

```text
INSTALL PACKAGE -> INSTALL SKILL -> INIT STORE -> SET IDENTITY
       -> CHOOSE HOST HOOK SURFACE -> PREVIEW/INSTALL -> STRICT CHECK -> RUNTIME SMOKE
```

1. Install the package and the `octocode-awareness` skill for the host.
2. Run `maintenance init` once. It creates/checks the canonical SQLite store; it does
   not create repository work.
3. Set one stable `OCTOCODE_AGENT_ID` for the main agent. Host-provided child IDs keep
   subagents distinct while the parent CLI and hooks share one identity.
4. Choose one hook surface: Claude skill frontmatter or Claude settings, Codex/Cursor
   project settings, or the Pi bridge. Never install both Claude surfaces.
5. Preview configuration writes, install after approval, then run strict config
   health. Strict success proves exact entries and existing script targets, not that
   the host executed them or delivered context.
6. Smoke a harmless write, failure, stop, compaction, and session boundary on the real
   host. Only runtime evidence upgrades configuration health to operational trust.

Hooks are optional. If they are absent or unhealthy, the manual CLI lifecycle below
remains complete.

## Homeostatic Control Model

Awareness is a supervised software control loop, not an autonomous agent. It
senses operational pressure in SQLite and hooks, compares that evidence with
bounded targets, recommends an actuator, and preserves human/agent choice at the
guard. Typical corrections are `attend --compact`, declaring file presence,
resolving a signal, verifying a run, previewing maintenance, or refreshing a
projection. Re-measurement closes the loop; unchanged state should inject no new
prompt text.

“Living repository” is a useful systems metaphor for continuous sensing,
adaptation, forgetting, and repair. It does not imply sentience, self-chosen goals,
network coordination, or permission to mutate code/instructions. The complete
pressure table and success measures live in [THESIS.md](THESIS.md).

## Durable Work Model

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
| RunFile | Mandatory advisory path presence; many agents may share a path. |
| Lock | Optional exclusive protection for sensitive work. |
| EditLog | Completed edit event history. |

Tasks are the only shared backlog. Plan documents explain objective and decisions;
they never copy live task status into a second “today” list.

## Lifecycle

```text
ENTER -> ACTIVATE -> ATTEND -> CHOOSE -> CLAIM/WORK -> DECLARE -> ACT
  -> SUBMIT/END -> VERIFY -> LEARN/HANDOFF -> CLEAN/PROJECT -> EXIT
```

1. **ENTER:** the host loads repository instructions. `AGENTS.md` routes the agent to
   this package guide and the Awareness skill.
2. **ACTIVATE:** load `octocode-awareness`; select other skills only for their owned
   decisions. Export the stable identity and choose the local/installed CLI.
3. **ATTEND:** `attend --compact --query <task>` returns verification debt, owned or
   ready work, file overlaps, inbox pressure, relevant evidence, and one `next` action.
4. **CHOOSE:** follow `next`. Join/inspect a plan, claim one derived-ready task, or open
   explicit standalone WORK. Do not create a second Markdown task queue.
5. **CLAIM/WORK:** a task claim or `work start` creates one run with rationale and an
   exact verification plan. Heartbeat long work.
6. **DECLARE:** every edited path receives advisory `RunFile` presence, either through
   hooks or explicit CLI. Ordinary overlap is allowed and visible. Use an exclusive
   lock only for sensitive, non-mergeable work; never bypass a conflict.
7. **ACT:** edit/review/test while presence remains active. Inspect peer rationale when
   overlap matters; use signals only when another agent must know or act.
8. **SUBMIT/END:** `task submit` or `work end` ends editing and moves the run to
   `PENDING`. This creates verification debt; it does not claim success.
9. **VERIFY:** run the declared check, record its receipt with `verify mark`, and finish
   with `verify audit`. Only `SUCCESS` clears the debt and completes task-backed work.
10. **LEARN/HANDOFF:** record only reusable, evidence-backed learning. Supersede stale
    memory. Use signals, refinements, or session capture for unfinished work.
11. **CLEAN/PROJECT:** only when pressure or a file-reader need exists, preview cleanup
    or regenerate `.octocode/`. Destructive maintenance remains explicit and scoped.
12. **EXIT:** stop/end hooks finalize only automatic HOOK fallback runs and capture
    handoff state. Session exit never marks a task or WORK run successful.

Host sessions are not work-unit boundaries. Only a task claim or explicit
`work start` may reuse an explicit standalone WORK run; fallback hook writes remain
isolated.

## Entity Lifecycles

| Entity | Lifecycle | Invariant |
|---|---|---|
| Plan | `DRAFT -> ACTIVE <-> PAUSED -> COMPLETED or CANCELLED` | The lead owns transitions; completion waits for active work to resolve. |
| Task | `OPEN -> IN_PROGRESS -> VERIFY -> DONE or FAILED`; side paths `BLOCKED`/`CANCELLED` | “Ready” is derived from ACTIVE plan + satisfied dependencies + no live claim. |
| Run | `ACTIVE -> PENDING -> SUCCESS or FAILED` | Ending edits creates debt; only verification writes a terminal result. |
| RunFile | declared/active -> heartbeat/extend -> ended or expired | Presence is mandatory and advisory; it is not a lock. |
| Lock | acquire -> renew -> release, expiry, or prune | Only `EXCLUSIVE`; reserved for sensitive work and attached to a run. |
| Signal | publish -> deliver/read/ack -> resolve -> optional prune | Messages are coordination evidence, not authority or a task queue. |
| Refinement | `open -> ongoing -> done` | Owned follow-up/handoff; terminal closure requires a check receipt. |
| Memory | record `ACTIVE` -> supersede/expire/archive -> optional restore or reviewed forget | Recall is a ranked lead; replacement history is immutable. |
| Session | register/start -> prompts/turns -> compact capture -> shutdown/end | PreCompact preserves the active session; end marks it inactive without success. |
| Projection | generate -> snapshot ages -> refresh or prune owned orphans | SQLite remains canonical; authored plan docs are preserved. |

Task, WORK, and HOOK are run origins, not interchangeable queues:

- `TASK` is a claimed durable plan task.
- `WORK` is explicit standalone work with rationale/files/test plan.
- `HOOK` is automatic fallback presence when a structured write has no task or WORK
  owner. A successful write remains active until a lifecycle boundary; a failed write
  discards only uncommitted HOOK presence.

## Hooks

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

Normal success is silent. Changed peer/briefing fingerprints emit one bounded delta;
unchanged state emits nothing. An exclusive conflict blocks before presence. Prompt
briefing uses transient prompt text to select at most one grounded memory lead or stay
silent; signals and overrides remain independent. Stop output is capped.

### Manual CLI And Hook Parity

| Need | Hook/Pi automation | Manual control-plane equivalent |
|---|---|---|
| Enter/orient | session/prompt registration and changed briefing | `attend`, `agent register`, targeted reads |
| Declare write | pre-edit presence and exclusivity check | `work start|touch`; add `--exclusive` when required |
| Successful write | heartbeat and edit audit | keep work active; record/check through the owning run |
| Failed write | discard uncommitted HOOK presence | preserve or explicitly end/release TASK/WORK after judgment |
| Conclude editing | Stop/compact/end finalizes HOOK fallback | `task submit` or `work end` |
| Prove success | reminder/audit only | run check, `verify mark`, `verify audit` |
| Handoff/exit | compact/end capture | signal, refinement, or `session capture` |

Hooks never replace `attend`, plan/task choice, deliberate exclusivity, verification
receipts, memory judgment, cleanup approval, or projection requests.

Host wiring details live in [HOOKS.md](HOOKS.md).

## Context Model

Persist everything needed for coordination; prompt only actionable changes:

- ordinary edit: zero injected awareness text;
- unrelated remembered state: zero injected text; a matching prompt gets at most one
  `Memory lead — verify` item;
- changed overlap: file, bounded peers, task/reason, omitted count;
- exclusive conflict: holder, reason, expiry, recovery action;
- compact attend: bounded action packet, not full organ/drive/profile aliases;
- full rows: explicit `work show`, query, recall, or noncompact attend.

This separates database completeness from token cost.

## Knowledge And Projection

Memory is durable verified learning, not routine status. Signals are typed peer
messages. Refinements are owned follow-up/handoff state, not another task queue.

The memory lifecycle is deliberately conservative:

1. Recall with task query and scope; filters/search/sort narrow candidates.
2. Smart widening may relax low-value filters and reports what changed under
   `--explain`; semantic reranking is optional and safely falls back to lexical FTS.
3. Treat every hit as a lead and re-check current source/tests/output.
4. Record only a scoped, reusable, evidence-backed lesson, decision, gotcha, or source.
5. Correct facts with `--supersedes`; archive reversibly; hard-forget only after a
   narrow dry-run and review.

`query <view>` reads the live DB. `wiki sync` publishes lean AGENTS, optional nonempty
bounded KNOWLEDGE, and a manifest under `.octocode/`. Explicit query exports provide
CSV/HTML when requested. Generated files are leads and may contain machine-local paths;
current source/tests/user instructions always win.

## Completion Contract

Awareness work is complete only when:

- no required edited path lacks declared ownership;
- no unresolved exclusive conflict was bypassed;
- editing has ended through the owning task/WORK/HOOK lifecycle;
- the declared verification ran and its result was recorded;
- `verify audit` shows no unintended debt for the agent/scope;
- necessary peer threads or handoffs are resolved or explicitly owned;
- reusable learning was recorded only when warranted;
- cleanup/projection was previewed and applied only when due.

## Boundaries

- Awareness owns coordination, memory, verification, hooks, and projection.
- `npx octocode` or Octocode MCP owns code/GitHub/package research and skill
  install/review operations.
- Harness proposals never self-apply. A human/user authorizes source or instruction
  changes, and normal verification still applies.

Schema detail: [DB.md](DB.md). File-work semantics: [LOCKS.md](LOCKS.md). User
recipes: [SKILLS.md](SKILLS.md). Research and prior-art boundaries:
[REFERENCES.md](REFERENCES.md).
