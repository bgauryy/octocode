# How Octocode Awareness Works

Awareness is a coordination runtime over one local SQLite database. The CLI,
library, hooks, Pi bridge, and query/projection surfaces all use the same state.

```text
Agent Skill -> CLI / library / hooks / Pi -> awareness.sqlite3
                                           |-> live query/attend/workboard
                                           `-> optional .octocode/ projection
```

No server or broker is required. Rows are isolated by normalized `workspace_path`
and optional artifact/repo/ref scope.

## Work Model

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
ATTEND -> CHOOSE -> DECLARE -> ACT -> SUBMIT -> VERIFY
                    |                         |
                    `-> SIGNAL/HANDOFF        `-> REFLECT/MAINTAIN/PROJECT
```

1. `attend --compact` returns current actions, file overlaps, verification debt,
   relevant evidence, and one next command.
2. An agent claims one ready task, or explicitly opens standalone work with `work start`.
3. Each edited file gets `run_files` presence. Ordinary overlaps are visible but
   allowed.
4. Sensitive work requests exclusivity. Acquisition fails if another agent already
   has live presence; an exclusive lock blocks later advisory declarations.
5. `task submit` or `work end` creates verification debt. `verify mark` records the
   actual check and closes the run/task.
6. Signals coordinate peers; refinements/session capture preserve unfinished state;
   reflection stores only reusable outcomes.

Host sessions are not work-unit boundaries. Only a task claim or explicit
`work start` may reuse an explicit standalone WORK run; fallback hook writes remain
isolated.

## Hooks

Pre-edit performs the harness self-edit guard first, then declares advisory work.
Normal success is silent. If the peer set changes, the hook emits one bounded
summary; if an exclusive conflict exists, it blocks before creating presence.

Post-edit writes `edit_log` and heartbeats. Task and explicit work runs remain
active; hook fallback runs end as `PENDING`. Prompt briefing and session capture use
fingerprints so unchanged state is not re-injected or duplicated. Stop output is
capped.

Host wiring details live in [HOOKS.md](HOOKS.md).

## Context Model

Persist everything needed for coordination; prompt only actionable changes:

- ordinary edit: zero injected awareness text;
- changed overlap: file, bounded peers, task/reason, omitted count;
- exclusive conflict: holder, reason, expiry, recovery action;
- compact attend: bounded action packet, not full organ/drive/profile aliases;
- full rows: explicit `work show`, query, recall, or noncompact attend.

This separates database completeness from token cost.

## Knowledge And Projection

Memory is durable verified learning, not routine status. Signals are typed peer
messages. Refinements are owned follow-up/handoff state, not another task queue.

`query <view>` reads the live DB. `repo inject` publishes bounded Markdown, CSV,
HTML, and a manifest under `.octocode/`. Generated files are leads and may contain
machine-local paths; current source/tests/user instructions always win.

## Boundaries

- Awareness owns coordination, memory, verification, hooks, and projection.
- `npx octocode` or Octocode MCP owns code/GitHub/package research and skill
  install/review operations.
- Harness proposals never self-apply. A human/user authorizes source or instruction
  changes, and normal verification still applies.

Schema detail: [DB.md](DB.md). File-work semantics: [LOCKS.md](LOCKS.md). User
recipes: [SKILLS.md](SKILLS.md). Research and prior-art boundaries:
[REFERENCES.md](REFERENCES.md).
