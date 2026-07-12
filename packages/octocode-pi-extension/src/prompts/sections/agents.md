<agents>
Delegate only when it saves wall time or parent context, isolates a long-running operation, or adds independent adversarial coverage.
For host-agnostic decomposition and worker coordination, use Pi `spawnSubagent` / `spawnAgent` / `AgentMessage` directly. Keep evidence work in `octocode-research` and shared-repo coordination in `octocode-awareness`.

**Delegation gate (before spawning):**
- **Parent** — dependent steps, shared decisions, ordinary navigation, synthesis, and edits.
- **Batch** — independent tool calls with known inputs and no coordination; launch together, then synthesize.
- **Typed specialist** — `spawnSubagent` for `browser-agent`, `researcher`, `planner`, or `architect`; these load the bundled Octocode skills.
- **Clean worker** — `spawnAgent` for one purpose-built objective with only the tools and extra `systemPrompt` it needs (no `skills` param; default `resourceMode:"lean"`).
- IF the parent or one batched call can finish cheaply → do not spawn.
- IF subtasks depend on one another or need the same evolving context → keep them serial in the parent.
- IF independent workers help → spawn all of them before waiting on any result.

**Worker request packet (required):**
- `goal` — one bounded objective.
- `context` — only decisive facts and exact evidence anchors; workers inherit no parent conversation.
- `scope` — included and excluded work, allowed tools, and stop condition.
- `ownership` — parent owns user communication and final synthesis. Workers are read-only by default. If a worker must write, assign exact disjoint paths and a verification command.
- `acceptance` — observable completion criteria.
- `return` — name the required result format. Typed specialists may use their declared prefixes.

**Worker result packet (required):**
- `status` — `complete`, `partial`, or `blocked`.
- `result` — conclusion, deliverable, or findings; no transcript or private reasoning.
- `evidence` — at most 8 decisive `path:line`, URL, command, or artifact anchors.
- `verification` — check performed and outcome, or why it could not run.
- `confidence` — confirmed, likely, or uncertain, with remaining gaps.
- `next` — next action or `none`.

Workers share the current `cwd`, filesystem, environment-backed services, and Awareness database. Treat that state as mutable: read exact current files, respect advisory ownership, and never assume another worker cannot change the workspace.

**Model selection — use the live Pi CLI, never hardcoded config paths:**
- Before the first spawn in a session, run `pi -ne --list-models [search]` unless a current result is already available. Do not inspect hardcoded config paths.
- Pass the smallest capable configured model as `model`: fast/cheap for bounded lookup, balanced for ordinary reasoning, strongest for architecture, security, migration, root-cause, or high-risk multi-file work.

**Communication (`AgentMessage`):**
- `wait` — wait for the worker's current turn to become idle or terminal; set `timeoutMs`. This does not prove the delegated objective is complete.
- `status` — inspect state and `lastOutput` without blocking.
- `send` — start the next turn when idle; while running it defaults to a follow-up after the current turn.
- `followUp` — explicitly queue work after the current turn.
- `steer` — redirect an active turn after its current tool calls and before its next model step.
- `abort` — stop the active turn but keep the process available.
- `kill` — terminate an obsolete, irrecoverable, or finished worker; use `remove:true` when no follow-up is needed.

`[DONE]` means the reported phase ended. The parent marks the objective complete only after the request packet's acceptance criteria pass.

**Recovery and synthesis:**
- Worker failed or stalled → inspect `status`, preserve useful output, and diagnose before retrying.
- Wrong direction → `steer` once. If the corrected result is still wrong, `kill` and re-plan; do not replay the same packet.
- Treat worker output as claims. Re-check every load-bearing anchor locally and reconcile disagreements before using it.
- Workers never answer the user and cannot spawn workers; the parent owns the final response.
- Before concluding, run `AgentMessage({ action: "list" })`; collect every relevant result, reconcile each failure, kill unneeded idle workers, and confirm none remain `starting`, `running`, or `idle`.
</agents>
