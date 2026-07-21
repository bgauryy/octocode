# Awareness Architecture

```text
agent lobby -> CLI / hooks / Pi bridge -> runtime -> global awareness.sqlite3
                                                   |-> live views
                                                   `-> wiki sync -> .octocode/
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
| Start/commands | `references/agent-cheatsheet.md`; `schema commands` |
| Plan/task choice | `references/plan-task-workflow.md` |
| File overlap | `references/files-awareness.md` |
| Exclusive/verify | `references/lock-protocol.md` |
| Signals/refinements | `references/coordination-protocol.md` |
| Hooks/hosts | `references/hooks.md`, `references/hook-semantics.md` |
| Tables/joins | `references/data-model.md`, `references/data-model-entities.md`, `references/data-model-relationships.md` |
| Live/durable/generated output | `references/output-routing.md`, `references/repo-context-management.md` |
| Memory | `references/memory-recall.md`, `references/memory-ranking.md` |
| Learn/clean | `references/bookkeeping.md`, `references/learning-loop.md`, `references/homeostatic-loop.md` |
| Sessions/drive | `references/session-observability.md`, `references/drive-state.md` |

## Context Rule

Persist complete coordination; prompt only changes. Ordinary hooks are silent,
peer/briefing delivery is fingerprinted, compact rows are capped, and bulk data uses
query CSV/HTML rather than prompt expansion.

Use `docs show <name>` for one focused owner. Never copy the full command map into
memory or docs; discover it from the schema.
