# Awareness reference index

Start with the [canonical Awareness guide](../README.md) for all feature families,
CLI/API entry points, architecture, local Git, expiration, and known limits.
This index routes detailed references; it does not own another overview or assessment.
Routine operation names come from
`npx @octocodeai/octocode-awareness schema commands --compact`; add `--all` only
for the bounded operator/recovery catalog. Prose docs do not duplicate either inventory.

| Document | Owns |
|---|---|
| [../ARCHITECTURE.md](../ARCHITECTURE.md) | Package ownership, storage boundaries, dependency rules, and generated-source policy. |
| [API.md](API.md) | Full structured command API, native host bindings, result codes, continuations, and agent prompt exports |
| [THESIS.md](THESIS.md) | Homeostatic control model, metaphor boundary, pressures, and success measures |
| [AGENT_PHYSIOLOGY.md](AGENT_PHYSIOLOGY.md) | Observed operational state, advisory regulation, host-only sensors, and control acceptance criteria |
| [HOW_IT_WORKS.md](HOW_IT_WORKS.md) | Lean default flow, optional tracked work, authority boundaries, peer delivery, and completion |
| [ENTITY_LINKS.md](ENTITY_LINKS.md) | All entity keys, declared foreign keys, logical links, and workspace checks |
| [DB.md](DB.md) | Exact SQLite schema admission, relationships, consolidation, and scope |
| [STORAGE_SCOPES.md](STORAGE_SCOPES.md) | Agent/Awareness path boundary, ownership matrix, overrides, canonical copies, and artifacts |
| [LOCAL_HISTORY.md](LOCAL_HISTORY.md) | Private Git objects, correlated captures, restore previews, verification, and recovery limits |
| [GIT_COORDINATION.md](GIT_COORDINATION.md) | Git suitability, linked-worktree communication, shared knowledge and physical lock boundaries |
| [CONFIGURATION.md](CONFIGURATION.md) | Lean feature defaults, opt-in profiles, validation, and storage selection |
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
