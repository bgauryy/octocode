<agents>
Understand the task fully before starting. For broad/non-trivial work: research → plan → write findings to a doc → compact → execute.

**Decomposition — pick the smallest shape that is correct:**
- First break the task into independent vs dependent subtasks; keep shared-context work in the parent.
- **Parent** — dependent steps, shared context, ordinary navigation and edits.
- **Batch** — independent tool calls with known inputs; no coordination needed; launch them together and synthesize after all return.
- **Typed specialist** — use `spawnSubagent` for bundled Octocode specialists (`browser-agent`, `researcher`, `planner`, `architect`). These load all Octocode skills by default.
- **Clean worker** — use `spawnAgent` for any purpose-built worker that should start lean. It defaults to no extensions/no skills and only receives the tools, skills, system prompt, and model you pass.
- **Spawn** — large independent work, long-running tasks, adversarial checks, parallel hypotheses; use only when the parallelism saves context or wall time.

**Before spawning** — load `octocode-subagents/SKILL.md`. Full protocol: parameters, lifecycle, communication patterns, anti-patterns, synthesis, and limits.

**Model selection — use the live Pi CLI, never hardcoded config paths:**
- Before choosing a worker model, run `pi -ne --list-models` (or `pi -ne --list-models <search>`) and treat that table as the source of truth for the user's configured models.
- Do not inspect fixed model config files; Pi may change config locations, merge defaults, or filter availability.
- Pick the smallest capable configured model for each worker and pass it as `model`.
- Small/simple workers: prefer the fastest/cheapest configured model with enough tool support and context.
- Medium workers: prefer a balanced configured coding/reasoning model.
- Large/high-risk workers: prefer the strongest configured model with the largest useful context/output budget and reasoning support.
- If the table makes the choice ambiguous, name the model choice in the worker prompt rationale and bias toward the stronger model for architecture, root-cause, security, migration, and multi-file implementation work.

**Worker design rules:**
- One worker = one objective. No shared state between workers.
- Prompt is the only channel — include every fact the worker needs; worker has zero parent context.
- Restrict tools to minimum needed (`tools` allowlist); read-only by default unless writes are required.
- Use clean `spawnAgent` when a worker should not inherit the Octocode specialist skill stack.
- Request structured output (JSON / numbered list) so results are parseable without inference.
- Workers are researchers, not responders — NEVER have workers communicate results to the user directly.

**Communication decision tree (`AgentMessage`):**
- `wait` — block until done; always set explicit `timeoutMs`.
- `status` — poll without blocking; use between `wait` calls for long tasks.
- `followUp` — queue a message; worker finishes current turn first.
- `steer` — interrupt mid-turn immediately; use when direction is clearly wrong.
- `abort` — graceful stop; process stays alive for follow-up messages.
- `kill` — hard terminate; use after 2 failed steers or when output is irrecoverable.

**Error recovery:**
- Worker `failed` or stuck → `status` to read output → diagnose root cause first.
- Wrong direction → `steer` once; still wrong → `kill` + spawn fresh with corrected prompt.
- Same failure twice → stop and re-plan; never retry blindly.

**Core invariants (always enforce):**
- `spawnAgent` returns `agentId`; use `AgentMessage` to monitor, steer, and collect.
- Spawn all independent workers **before** waiting on any of them.
- Workers cannot spawn workers — `spawnAgent`/`AgentMessage` are removed from worker tool lists.
- Worker prompts must be fully self-contained — the worker has zero parent context.
- Treat all worker output as **claims** — verify with local tools before relaying.
- Before concluding: `AgentMessage({ action: "list" })` — confirm every worker is `exited` or `killed`.
</agents>
