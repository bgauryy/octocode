# Octocode Awareness Documentation

Each concept has one owner. Command names and schemas come from
`octocode-awareness schema commands --compact`; prose docs do not duplicate the
complete command inventory.

| Document | Owns |
|---|---|
| [THESIS.md](THESIS.md) | Homeostatic control model, metaphor boundary, pressures, and success measures |
| [HOW_IT_WORKS.md](HOW_IT_WORKS.md) | Canonical complete bootstrap, operating, state, hook, memory, projection, and exit lifecycle |
| [DB.md](DB.md) | SQLite schema, relationships, migration, scope |
| [LOCKS.md](LOCKS.md) | Advisory file work, exclusive locks, verification |
| [HOOKS.md](HOOKS.md) | Host installation and runtime behavior |
| [MEMORY_NAVIGATION.md](MEMORY_NAVIGATION.md) | Compact attend, workboard, delivery budgets |
| [SKILLS.md](SKILLS.md) | User/agent installation and operating recipes |
| [REFLECTION.md](REFLECTION.md) | Learning, failure signatures, human approval |
| [WIKI.md](WIKI.md) | Live queries and generated `.octocode/` projections |
| [HARNESS.md](HARNESS.md) | Maintainer invariants and verification matrix |
| [VERIFY.md](VERIFY.md) | Any-agent quick, installed, host, monorepo, and release verification runbook |
| [COMPREHENSIVE_AUDIT.md](COMPREHENSIVE_AUDIT.md) | Scored whole-system audit for coordination, storage, delivery, and read cost |
| [FEATURE_SWEEP.md](FEATURE_SWEEP.md) | Isolated end-to-end proof for planning, learning, wiki, registry, and maintenance surfaces |
| [REFERENCES.md](REFERENCES.md) | Evidence map, prior art, hypotheses, and design limits |

Agent-facing procedures live under repo-root `skills/octocode-awareness/references/` and
are listed by `octocode-awareness docs list --compact`.

Canonical data lives in the global Awareness SQLite database. Generated
`<workspace>/.octocode/` files are bounded projections; managed
`.octocode/plan/**` files are plan narrative, not a live task checklist.
