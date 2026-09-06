# Awareness Architecture

Load when changing store ownership, host composition, or policy routing. This reference step ends here; return to the main skill flow.

## Shared coordination plane

```text
agent CLI     -> npx @octocodeai/octocode-awareness ┐
host tools/hooks -> package API                     ├-> shared dispatcher -> awareness.sqlite3
other integrations -> package API                   ┘                        (global or explicit repo scope)
```

Root-noun CLI routes and host methods share one physical ledger: `awareness_plans`, `awareness_tasks`, `task_runs`, `run_files`, `awareness_locks`, `awareness_memories`, `awareness_agents`, and `signals`. Unique continuity operations use the shared dispatcher. Inspect `schema entities --all` for actual relations and constraints.

Global scope resolves by default to `$OCTOCODE_HOME/awareness/awareness.sqlite3`. An explicit workspace policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`; `--db-scope global` selects the global store for one call, and `--db <path>` has highest precedence. Existing databases are preserved; no automatic merge occurs. Agent control (`agent.sqlite3`) and Rust runtime (`core.sqlite3`) remain separate Agent-owned databases under `$OCTOCODE_HOME/agent/`.

## Runtime boundary

The same package CLI owns attend/workboard, plans/tasks/work, locks/verification, agents/signals/sessions, memory/refinements/reflection, query/docs/schema, hooks, and maintenance. Stores initialize lazily; use `maintenance init` only when an explicit initialization check is needed.

The global store can hold multiple normalized `workspace_path` identities. Repository scope remains available for deliberate isolation through policy or `--db-scope repo`.

The CLI boundary uses canonical root nouns. Use `status` for store health, `query <view>` for targeted inspection, and inspect the matching live schema before acting.

## Invariants

- Workspace path scopes shared rows; two repositories may use the same agent id or relative file path safely.
- Reads do not clean up state. Expired rows are filtered/projected; reclaim and pruning are explicit mutations.
- Task completion creates verification debt. CLI verification targets the owned run; host check APIs also fence the checked completion timestamp. Old receipts cannot verify a new run. Terminal tasks cannot be released or revived in closed plans.
- Presence is advisory; locks are exceptional exclusivity.
Return to `SKILL.md`. Hooks automate deterministic edges, never task choice or success claims.
