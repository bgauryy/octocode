# Spawn Decisions

Use this before creating any background worker.

## Spawn

Spawn only when delegation materially helps:

- Large independent work packages with clear inputs and outputs.
- Long-running work that frees the parent to proceed.
- Adversarial or coverage checks, such as second opinions and independent validation.
- Parallel hypotheses that do not share mutable state.

## Stay In Parent

Keep the task in the parent for:

- Ordinary bug fixes or refactors needing shared context.
- Dependent steps where one result gates the next.
- Small or medium tasks completable in one session.
- Work needing tight real-time coordination with the parent.

## Resource Mode

- `lean` - fastest; no extensions, skills, prompts, or themes. Use for focused search, read, or shell tasks.
- `octocode` - loads the Octocode extension and tools. Use when the worker is instructed to use `octocode-research` for code research.
- `default` - full Pi discovery. Use only when the worker needs arbitrary installed resources.

## Hard Limits

- Workers cannot spawn workers; `spawnAgent` and `AgentMessage` are removed from worker tools.
- The registry keeps 50 agents; oldest records are evicted when the cap is reached.
- Default `wait` timeout is 300000 ms. Set explicit timeouts for predictable orchestration.
