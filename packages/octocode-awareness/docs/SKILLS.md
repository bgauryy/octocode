# Octocode Awareness Skill

This package ships one Agent Skill backed by one runtime and one SQLite store:

- `octocode-awareness`: live workspace awareness, locks, recall, refinements, signals/messages, verification, reflection, learning, cleanup, and hooks.

Awareness keeps work safe while it is happening, moves live messages between agents, and decides what should persist after the work is done. Older prompt text may still name `octocode-agent-communication` or `octocode-reflection`; route those requests to `octocode-awareness`.

For technical onboarding, read [`skills/octocode-awareness/references/full-flow.md`](../skills/octocode-awareness/references/full-flow.md). It connects the CLI, skill instructions, hooks, locks, verification, generated `.octocode/` repo context, reflection, and handoff model in one flow.

## Skill Map

| Skill | Path | Primary job | Load it when |
|---|---|---|---|
| Awareness | `skills/octocode-awareness` | Attend, recall, claim files, signal agents, verify, reflect, clean, run hooks | Starting work, planning edits, checking locks, handling signals, learning from outcomes, or finishing work |

```mermaid
flowchart LR
  User["User"]
  Agent["Agent"]

  subgraph Skills["Agent Skills"]
    Awareness["octocode-awareness<br/>live coordination"]
  end

  subgraph Runtime["Package runtime"]
    CLI["octocode-awareness<br/>package CLI + skill script"]
    Hooks["lifecycle hooks<br/>pre/post/stop/briefing"]
    Pi["Pi bridge<br/>wirePiAwarenessHooks"]
    Core["@octocodeai/octocode-awareness<br/>src/*.ts modules"]
  end

  DB[("~/.octocode/memory/awareness.sqlite3")]

  User --> Agent
  Agent --> Awareness
  Awareness --> CLI
  Hooks --> CLI
  Pi --> Core
  CLI --> Core
  Core --> DB
```

## How The Skills Combine

Use `octocode-awareness` as the primary workflow. Old skill names route back to Awareness; they do not define separate checklists or CLI aliases.

1. Start with `octocode-awareness`.
2. If `workspace status`, `signal list`, or hook-injected briefing shows a message, handle it with the Awareness signal commands.
3. If work is finished or a reusable lesson exists, record it with Awareness memory/reflection commands.
4. Keep every command scoped to the same DB, workspace, artifact, repo, and ref.

The combined flow is CLI-first:

| Phase | Commands | Durable effect |
|---|---|---|
| Attend | `workspace status`, `memory recall`, `refinement get`, `signal list` | Reads current locks, memories, handoffs, and messages from SQLite |
| Claim | `lock acquire`, `lock wait` | Creates an edit task and per-file locks |
| Work | hooks or manual edits under the claim | Hooks can release locks as `PENDING` verification |
| Verify | `verify mark`, `verify audit` | Records the check that ran and clears verification debt |
| Reflect | `reflect record`, `reflect mine-weakness`, `reflect export-harness` | Stores lessons, clusters failure signatures, and previews human-reviewed harness guidance |
| Project | `query <view>`, `repo inject` | Reads live DB views or refreshes `.octocode/` auto wiki projections |
| Hand off | `signal publish|reply|ack|resolve`, `refinement set|get`, `session capture` | Preserves messages, decisions, and unfinished work for the next agent |

```mermaid
flowchart TD
  Start["Agent receives task"] --> Attend["Awareness: workspace status, memory recall, refinements, unread signals"]
  Attend --> MessageQ{"Unread or relevant<br/>agent message?"}
  MessageQ -->|yes| Comm["Awareness: inspect, act, reply, ack, resolve"]
  MessageQ -->|no| Plan["Plan work"]
  Comm --> Plan
  Plan --> EditQ{"Will write files?"}
  EditQ -->|yes| Claim["Awareness: lock acquire<br/>claim target files"]
  EditQ -->|no| Work["Do read-only work"]
  Claim --> Work["Work under lock"]
  Work --> Verify["Awareness: run checks, verify, release"]
  Verify --> LessonQ{"Lesson, stale state,<br/>or improvement?"}
  LessonQ -->|yes| Reflect["Awareness: record, reflect, clean, stage"]
  LessonQ -->|no| Finish["Finish"]
  Reflect --> Finish
```

## Agent Lifecycle

```mermaid
sequenceDiagram
  participant Agent
  participant Awareness
  participant Hooks
  participant DB as awareness.sqlite3

  Agent->>Awareness: workspace status + memory recall + refinement get
  Awareness->>DB: read memories, locks, refinements, signals
  DB-->>Awareness: live workspace state
  Awareness-->>Agent: plan context

  alt messages exist
    Agent->>Awareness: signal list
    Awareness->>DB: read inbox
    Agent->>Awareness: reply or ack after acting
    Awareness->>DB: insert signals or signal_reads
  end

  Agent->>Awareness: lock acquire
  Awareness->>DB: create task and locks
  Hooks->>Awareness: post-edit release as PENDING
  Awareness->>DB: task remains verification owed
  Agent->>Awareness: verify mark and lock release
  Awareness->>DB: task SUCCESS or FAILED

  Agent->>Awareness: reflect record or memory record
  Awareness->>DB: memories, refinements, harness log
```

## User View

Users get three practical benefits:

- Safer multi-agent edits: file claims and verification gates make collisions visible.
- Better continuity: memories, refinements, and session captures keep useful context out of the model prompt until needed.
- Agent messaging: signals give agents a local mailbox with threads, broadcast, targeted delivery, ack, and resolution.

Users usually do not call every command manually. They install or preview hooks, then agents call the package CLI or bundled skill script as needed.

```bash
npx @octocodeai/octocode-awareness hooks install --host codex --dry-run
npx @octocodeai/octocode-awareness workspace status --workspace "$PWD"
npx @octocodeai/octocode-awareness signal list --agent-id "$USER" --workspace "$PWD"
```

## Agent View

Agents should treat the skills as progressive disclosure:

- Read `SKILL.md` first.
- Load a reference only when the routed condition matches.
- Use `octocode-awareness` for installed/package usage.
- Use `scripts/schema.mjs` before building wrappers or Pi adapters.
- Use `scripts/awareness.mjs` only as the bundled standalone fallback.
- Treat memory and signals as leads. Verify current code, files, and command output before relying on them.

## Tool Surface

The public package exposes `octocode-awareness`; the primary skill also bundles `scripts/awareness.mjs` for standalone installs. Old skill names route back to Awareness, but no separate stub skill folders are shipped.

### Memory And Recall

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `tell_memory` | `memory record` | Awareness | Store a durable reusable lesson, decision, gotcha, or observation |
| `get_memory` | `memory recall` | Awareness | Recall scoped memories by query, labels, tags, files, references, repo, and ref |
| `forget_memory` | `memory forget` | Awareness | Delete memories by id, tag, age, or importance ceiling |

### Repo Context Projections

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `query` | `query <view>` | Awareness | Read normalized DB views (`memories`, `gotchas`, `lessons`, `tasks`, `locks`, `agents`, `signals`, `refinements`, `files`, `activity`, `repo-profile`, `all`) as JSON/table/CSV/Markdown/HTML |
| `repo_inject` | `repo inject` | Awareness | Generate `.octocode/AGENTS.md`, memory/gotcha/learning docs, CSV projections, references, manifest, and optional HTML without editing `.gitignore` |

These projections are the awareness "auto wiki" surface. They are generated from the same SQLite store used by locks, signals, reflection, and verification; they are not a second source of truth and should be regenerated rather than hand-edited.

### Workspace And Locks

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `workspace_status` | `workspace status` | Awareness | Show DB health, memory counts, locks, refinements, and pending verification |
| `pre_flight_intent` | `lock acquire` | Awareness | Create an edit task and acquire file locks |
| `wait_for_lock` | `lock wait` | Awareness | Poll until target file locks clear without acquiring them |
| `prune_stale_locks` | `lock prune` | Awareness | Delete expired or age-stale locks; affected tasks become `PENDING` |
| `release_file_lock` | `lock release` | Awareness | Release locks as `SUCCESS`, `FAILED`, or `PENDING` |
| `verify` | `verify mark` | Awareness | Record that declared verification actually ran |
| `audit_unverified` | `verify audit` | Awareness | List pending or stale work that blocks clean conclusion |

### Refinements And Handoffs

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `refinement` | `refinement set` | Awareness | Save workspace work state for the next agent |
| `refine_query` | `refinement get` | Awareness | Read unfinished or filtered refinements |
| `refine_delete` | `refinement delete` | Awareness | Hard-delete stale refinements by id |

### Communication

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `agent_registry` | `agent register|list` | Awareness | Register or list agent identities in the shared DB |
| `agent_signal` | `signal publish|list|reply|ack|resolve` | Awareness | Publish, list, reply, ack, and resolve signals |
| `signal_prune` | `signal prune` | Awareness | Delete selected resolved, old, or explicit signals |

### Reflection And Harness

| Schema name | CLI command | Owner skill | Purpose |
|---|---|---|---|
| `reflect` | `reflect record` | Awareness | Record outcome, lesson, failure signature, and staged improvement hints |
| `export_harness` | `reflect export-harness` | Awareness | Preview AGENTS.md or harness guidance candidates from top lessons |
| `digest` | `maintenance digest` | Awareness | Preview or run memory/signal/refinement cleanup |
| `mine_weakness` | `reflect mine-weakness` | Awareness | Cluster repeated failure signatures |
| `doc_staleness` | `docs staleness` | Awareness | Find docs likely stale from edit log activity |
| `session_capture` | `session capture` | Awareness hooks | Write a session handoff refinement from lock and git state |
| n/a | `maintenance init` / `maintenance self-test` | Runtime | Initialize or smoke-test the shared DB |

### Pi Tool Mapping

| Pi-facing tool | CLI/runtime equivalent |
|---|---|
| `workspace_status` | `workspace status` |
| `memory_recall` | `memory recall` |
| `memory_record` | `memory record` |
| `memory_refine_get` | `refinement get` |
| `agent_signal` | `signal publish|list|reply|ack|resolve` |
| `file_lock type:lock` | `lock acquire` |
| `file_lock type:release` | `lock release` |
| `memory_verify` | `verify mark` |
| `memory_audit_unverified` | `verify audit` |
| `memory_reflect` | `reflect record` |
| `memory_export_harness` | `reflect export-harness` |

## Communication Model

Communication is local-first. The SQLite DB is the broker. Hooks, Pi bridge, and CLI calls are delivery surfaces.

```mermaid
flowchart LR
  Sender["Sender agent"] --> Publish["signal publish<br/>or reply"]
  Publish --> Signals[("signals")]
  Signals --> Inbox{"Receiver surface runs?"}
  Inbox -->|hook or Pi bridge| Briefing["additionalContext briefing"]
  Inbox -->|manual poll| List["signal list"]
  Inbox -->|no running surface| Durable["Message waits in DB"]
  Briefing --> Act["Receiver acts"]
  List --> Act
  Act --> Ack["signal ack<br/>writes signal_reads"]
  Act --> Reply["signal reply<br/>same thread_id"]
  Act --> Resolve["signal resolve<br/>closes signal/thread"]
```

Important semantics:

- `to_agent = NULL` is broadcast.
- `thread_id` is the topic/thread. Do not add a second topic store unless a real need appears.
- Hook delivery does not mark read. Agents ack only after acting.
- A message cannot be pushed into an agent that has no hook, no Pi bridge, and no polling run. It remains durable until a surface reads it.
- A2A-style mapping is local: `agents` is identity, `signals` is message/task state, `signal_reads` is acknowledgement, and `thread_id` groups a task or discussion. This package does not expose a public A2A server.

## Hook Model

Hooks turn guidance into lifecycle enforcement where the host supports them.

```mermaid
flowchart TD
  Prompt["UserPromptSubmit"] --> Brief["notify-deliver<br/>register agent + inject unread messages"]
  Pre["PreToolUse write"] --> Guard["harness-guard<br/>protect skill self-edits"]
  Pre --> Claim["pre-edit<br/>claim file locks"]
  Post["PostToolUse write"] --> Pending["post-edit<br/>release locks as PENDING"]
  Stop["Stop/SubagentStop"] --> Audit["stop-verify<br/>block if unverified work remains"]
  Compact["SessionEnd or PreCompact"] --> Capture["session-end<br/>session capture refinement"]
```

Codex does not run standalone `SKILL.md` hook frontmatter. Install Codex hooks with `.codex/hooks.json`, inline `[hooks]` config, or plugin hooks. Claude-style hosts may execute skill frontmatter. Pi uses `wirePiAwarenessHooks(pi)`.

## Data Model

```mermaid
erDiagram
  agents ||--o{ sessions : "starts"
  agents ||--o{ tasks : "owns"
  agents ||--o{ memories : "records"
  agents ||--o{ signals : "sends"
  agents ||--o{ signal_reads : "acks"
  sessions ||--o{ tasks : "groups"
  tasks ||--o{ locks : "claims"
  tasks ||--o{ task_log : "verified_by"
  memories ||--o{ memory_refs : "cites"
  signals ||--o{ signal_reads : "read_by"
  sessions ||--o{ edit_log : "records"
  sessions ||--o{ harness_log : "captures"
```

| Table | Used by | Meaning |
|---|---|---|
| `agents` | Communication, hooks, Pi | Stable agent id, display name, context, last-seen scope |
| `sessions` | Awareness, hooks | Contiguous agent work period |
| `memories` | Awareness, Reflection | Durable lessons and observations |
| `memory_refs` | Reflection | Structured provenance for memories |
| `tasks` | Awareness | Declared edit intent and verification plan |
| `locks` | Awareness hooks | Per-file lock rows tied to tasks |
| `task_log` | Awareness | Verification events |
| `refinements` | Awareness, Reflection | Workspace work state and handoffs |
| `signals` | Communication | Typed live messages, replies, threads, and status |
| `signal_reads` | Communication | Idempotent per-agent acknowledgements |
| `edit_log` | Reflection harness | Optional edit audit trail |
| `harness_log` | Reflection harness | Self-improvement and capture events |

All rows are scoped primarily by `workspace_path`, with optional `artifact`, `repo`, and `ref`. That lets one DB serve a whole machine while still isolating packages, branches, and projects.

## Implementation

The package runtime is TypeScript plus Node's built-in SQLite.

| Module | Role |
|---|---|
| `src/db.ts` | DB path resolution, schema DDL, migration, FTS setup, WAL mode |
| `src/memory.ts` | Memory insert, recall, scoring, FTS, embeddings, weakness mining |
| `src/intents.ts` | File lock acquisition and release |
| `src/verify.ts` | Verification audit and task log updates |
| `src/refinements.ts` | Handoff/work-state CRUD |
| `src/notifications.ts` | Signals, inbox reads, ack, resolve, prune, `agentSignal` facade |
| `src/agents.ts` | Agent registry, display names, last-seen updates |
| `src/maintenance.ts` | Status, smart briefing, digest, session capture, harness export |
| `src/reflect.ts` | Reflection memories and staged improvement signals |
| `src/sessions.ts` | Session lifecycle |
| `src/audit.ts` | Edit and harness logs |
| `src/docs.ts` | Doc staleness mining |
| `src/pi-hooks.ts` | Pi bridge, write-path extraction, hook wiring |
| `bin/awareness.ts` | CLI flag parsing and command dispatch |
| `bin/hook-runner.ts` | Shell hook dispatcher for lifecycle events |
| `scripts/schema.mjs` | Zod schemas, examples, validation, JSON Schema export |

Build output is copied into the primary awareness skill's `scripts/` directory. Retired legacy skill names are pruned from generated mirrors.

```mermaid
flowchart LR
  TS["src/*.ts and bin/*.ts"] --> Build["build.mjs"]
  Schema["scripts/schema.mjs"] --> Build
  Build --> Dist["dist/"]
  Build --> AwarenessScripts["skills/octocode-awareness/scripts"]
```

## Choosing The Right Skill

| Situation | Skill | First action |
|---|---|---|
| Starting or planning work | Awareness | `workspace status`, `memory recall`, `refinement get`, inbox check |
| Editing files | Awareness | `lock acquire` before edits, `verify mark` before finishing |
| A message appears in status or hook context | Awareness | `signal list`, then act, reply, ack, resolve |
| Need to ask another agent something | Awareness | `agent list`, then `signal publish` |
| Finished work produced a reusable lesson | Awareness | `reflect record` or `memory record` |
| Old memory, signal, refinement, or pending task looks stale | Awareness | Use dry-run cleanup first |
| Skill or harness should improve itself | Awareness | Stage a proposal with evidence and rollback |

## Practical End-To-End Recipe

```bash
# 1. Attend
octocode-awareness workspace status --workspace "$PWD"
octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart

# 2. Communicate when needed
octocode-awareness agent list --workspace "$PWD"
octocode-awareness signal list --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD"

# 3. Claim and work
octocode-awareness lock acquire \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --rationale "Make the requested change" \
  --test-plan "Run the focused test" \
  --target-file "$PWD/path/to/file"

# 4. Verify and release
octocode-awareness verify mark \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --all-pending \
  --message "Focused tests passed"

# 5. Reflect after the outcome is known
octocode-awareness reflect record \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --task "Describe the work" \
  --outcome worked \
  --lesson "Reusable lesson for next time"
```

## Verification Expectations

Before changing any skill or runtime code:

1. Use awareness to claim target files.
2. Run `yarn workspace @octocodeai/octocode-awareness build` so generated skill scripts refresh.
3. Run focused tests for the changed behavior.
4. Run `yarn workspace @octocodeai/octocode-awareness test:quiet` for shared runtime changes.
5. Run skill lint when `SKILL.md` or references change.
6. Use `git diff --check`.
7. Record verification with `verify` and release locks.
