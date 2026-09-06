# Octocode Awareness documentation

Each concept has one owner. This index routes the complete bootstrap, operating, state, hook, memory, projection, and exit lifecycle. Command names and schemas come from
`npx @octocodeai/octocode-awareness schema commands --compact`; prose docs do not duplicate the
complete command inventory.

| Document | Owns |
|---|---|
| [../ARCHITECTURE.md](../ARCHITECTURE.md) | Package ownership, storage boundaries, dependency rules, and generated-source policy. |
| [THESIS.md](THESIS.md) | Homeostatic control model, metaphor boundary, pressures, and success measures |
| [AGENT_PHYSIOLOGY.md](AGENT_PHYSIOLOGY.md) | Observed operational state, advisory regulation, host-only sensors, and control acceptance criteria |
| [HOW_IT_WORKS.md](HOW_IT_WORKS.md) | Canonical agent loop, structured `attend.next`, authority boundaries, hooks, memory, projection, and exit lifecycle |
| [ENTITY_LINKS.md](ENTITY_LINKS.md) | All entity keys, declared foreign keys, logical links, and workspace checks |
| [DB.md](DB.md) | SQLite schema, relationships, migration, scope |
| [STORAGE_SCOPES.md](STORAGE_SCOPES.md) | Agent/Awareness path boundary, ownership matrix, overrides, migration, and artifacts |
| [CONFIGURATION.md](CONFIGURATION.md) | Global feature defaults, onboarding questions, validation, and fixed safety boundaries |
| [LOCKS.md](LOCKS.md) | Advisory file work, exclusive locks, verification |
| [HOOKS.md](HOOKS.md) | Host installation and runtime behavior |
| [MEMORY_NAVIGATION.md](MEMORY_NAVIGATION.md) | Compact attend, workboard, delivery budgets |
| [SKILLS.md](SKILLS.md) | User/agent installation and operating recipes |
| [REFLECTION.md](REFLECTION.md) | Learning, failure signatures, human approval |
| [HARNESS.md](HARNESS.md) | Maintainer invariants and verification matrix |
| [VERIFY.md](VERIFY.md) | Any-agent quick, installed, host, monorepo, and release verification runbook |
| [COMPREHENSIVE_AUDIT.md](COMPREHENSIVE_AUDIT.md) | Scored whole-system audit for coordination, storage, delivery, and read cost |
| [FEATURE_SWEEP.md](FEATURE_SWEEP.md) | Isolated evaluation recipes for every feature family, including hooks, semantic recall, saturation, and long-run recovery |
| [REFERENCES.md](REFERENCES.md) | Evidence map, prior art, hypotheses, and design limits |

Agent-facing procedures live under package-local `skills/octocode-awareness/references/` and
are listed by `npx @octocodeai/octocode-awareness docs list --compact`. Start with `flow-matrix`
when choosing among lifecycle paths, then open exactly one deeper reference.

Canonical Awareness data lives in `$OCTOCODE_HOME/awareness/awareness.sqlite3` by
default. An explicit workspace policy or `--db-scope repo` selects
`<workspace>/.octocode/awareness.sqlite3`; `--db-scope global` selects the global
store for one call, and an explicit `--db` path has highest precedence. Agent
databases under `$OCTOCODE_HOME/agent/` have a separate owner and identity.
On request, `query` writes read-only `<workspace>/.octocode/` export snapshots;
managed `.octocode/plan/**` files are plan narrative, not a live task checklist.
