Load when a migration or refactoring term is used: seam, strangler fig, branch by abstraction, parallel change, feature toggle, shim or vertical slice.

# Concepts: migration and refactoring vocabulary

## Migration and refactoring vocabulary

Terms used by [change discipline](change-discipline.md#refactor), which owns the moves and guardrails.

| Term | Meaning | Evidence it worked |
|---|---|---|
| Seam | A place where behavior can be changed without editing the code in place | A [characterization test](concepts-bugs-testing.md#testing-concepts) can substitute one side |
| Strangler fig | Route traffic to new code path by path, then retire the old | Both paths measured; old path removed after consumer checks |
| Branch by abstraction | Introduce an abstraction, move implementations behind it, swap | No duplicated callers at the end |
| Parallel change (expand, migrate, contract) | Add the new shape, move consumers and data, remove the old; the zero-downtime schema migration | Persisted data and deployed versions read both shapes during migration |
| Feature toggle (feature flag) | Runtime switch selecting a path, for gradual rollouts | Both paths tested; toggle removed after rollout |
| Sprout, wrap | Add new behavior in a new unit, or around the old one | Old unit unchanged; new behavior tested at the seam |
| Shim, compatibility layer | Adapter that keeps an old contract alive | Consumer list and retirement check; a candidate for removal once consumers moved |
| Vertical slice | One scenario moved end to end | Scenario passes through the new path with the old removed for it |
