# Awareness Architecture

```text
agent lobby -> CLI / hooks / Pi bridge -> runtime -> global awareness.sqlite3
                                                   |-> live views
                                                   `-> repo inject -> .octocode/
```

SQLite is canonical and scoped. Generated `.octocode/` files are leads; managed
`.octocode/plan/**` narrative is authored, while live tasks remain in SQLite.

## Collaboration Core

```text
plan -> task -> task run -> advisory run files
                         `-> optional exclusive locks
standalone work -> explicit WORK run -> same file/lock model
```

Tasks are the only durable queue. Runs are attempts. File work is mandatory and
non-blocking by default. Locks are exclusive safety for sensitive work. Edit log is
completed-event history.

## Owners

| Need | Reference/surface |
|---|---|
| Start/commands | `agent-cheatsheet.md`; `schema commands` |
| Plan/task choice | `plan-task-workflow.md` |
| File overlap | `files-awareness.md` |
| Exclusive/verify | `lock-protocol.md` |
| Signals/refinements | `coordination-protocol.md` |
| Hooks/hosts | `hooks.md`, `hook-semantics.md` |
| Tables/joins | `data-model.md`, `data-model-entities.md`, `data-model-relationships.md` |
| Live/durable/generated output | `output-routing.md`, `repo-context-management.md` |
| Memory | `memory-recall.md`, `memory-ranking.md` |
| Learn/clean | `bookkeeping.md`, `learning-loop.md`, `homeostatic-loop.md` |
| Sessions/timing | `session-observability.md` |

## Context Rule

Persist complete coordination; prompt only changes. Ordinary hooks are silent,
peer/briefing delivery is fingerprinted, compact rows are capped, and bulk data uses
query CSV/HTML rather than prompt expansion.

Use `docs show <name>` for one focused owner. Never copy the full command map into
memory or docs; discover it from the schema.
