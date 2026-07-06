# Agentic Flows

Use this when combining Awareness' manual loop, lifecycle hooks, subagent handoffs, reflection, and cleanup. One operating model is the goal: skill teaches intent, hooks catch lifecycle moments, and reflection improves future behavior.

## Three flow layers

| Layer | Handles | Use it for | Avoid using it for |
|-------|---------|------------|--------------------|
| Skill loop | `memory_recall`, `memory_refine_get`, `workspace_status`, `file_lock type:lock`, `memory_verify`, `memory_reflect fix_repo:`, `memory_reflect` | Intentional work: attend, focus, claim, verify, encode, sleep | Automatic enforcement by itself |
| Hooks | `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`, `SessionEnd` | Lifecycle guardrails: deliver messages, claim files, keep verification visible, capture handoffs | Deciding that the artifact is correct |
| Agentic loop | `agent_signal action:publish`, `reflect --duo`, `--eval-failure-json`, `mine_weakness` (via CLI), `export_harness` (via CLI) | Peer coordination, critique, recurring failure mining, harness improvements | Storing raw private reasoning or unattended self-modification |

Hooks should make the right behavior harder to forget. They do not replace the agent's judgment, test plan, or explicit verification.

## Recommended compositions

### Read-only research

Run `memory_recall smart:true`, `memory_refine_get`, `workspace_status`, and `agent_signal action:list`. Treat memories and notifications as leads, then prove claims from current files, commands, or Octocode research. No file lock is needed unless the work will write files.

### Single-agent edit

1. Attend: recall memory, handoff, status, and messages.
2. Focus: choose the smallest file set and test plan.
3. Claim: call `file_lock type:lock`; hooks may also claim during edit tools.
4. Work: edit only the claimed files.
5. Verify: run the declared checks and record them with `memory_verify`.
6. Encode: write a refinement or memory only if it changes a future decision.
7. Sleep: audit idle state, reflect, release or confirm released locks, and prune only with dry-run evidence.

If hooks are active, `PreToolUse` claims and `PostToolUse` releases the live lock as `PENDING`, but the agent still owes verification before claiming success.

### Multi-agent or subagent work

Set a stable `OCTOCODE_AGENT_ID` when possible so hook-managed and manual calls share identity.
Use parent/child names such as `codex/research-web` when delegating.
Default subagents to read-only research or review.
Integrate final writes in one agent. Split writes only when files are clearly disjoint.

Require a compact subagent evidence receipt before using delegated conclusions:

```text
role:
scope/files/surfaces:
claims/results:
evidence anchors:
verification run or not run:
decision impact:
open questions:
trace/ref ids:
```

Store the receipt with `notify --kind handoff` for live coordination.
Use `memory_reflect fix_repo:` when the next run must inherit it.
Do not store raw transcripts. `SubagentStop` can flag missing verification.
The parent still checks anchors, records verification, and decides what survives.

### Harness improvement

Use `reflect --duo` for ambiguous or substantial outcomes.
Use `--eval-failure-json` when another skill emits structured failures.
Then run `mine_weakness` (via CLI) to find repeated signatures.
Preserve the path `trace -> finding -> eval target -> bounded task`.
Group repeated failures before changing the harness.
Make each proposed fix small enough to verify.
Use `export_harness` (via CLI) to preview proposed changes.
Apply changes only after human approval on a dedicated branch.

### Sleep cleanup

Sleep runs at end-of-work, session end, subagent handoff, or explicit cleanup.
Sleep is not triggered by quiet time alone.
Audit first with `workspace_status`, `memory_audit_unverified`, `agent_signal action:list`, `memory_refine_get`, `memory_forget dry_run:true`, and `memory_digest dry_run:true`.
Then record verification, reflect, and mark true handoffs done.
Supersede stale memories, prune resolved messages, update stable corpus docs, and release locks.

## Hook leverage

Use hooks for checkpoints that line up with the host lifecycle:

- `UserPromptSubmit`: inject unread repo messages before the agent reasons.
- `PreToolUse`: claim files before writes and block real collisions.
- `PostToolUse`: release the live lock while preserving a pending verification obligation.
- `Stop` / `SubagentStop`: block one unverified conclusion and force the agent to verify or hand off.
- `SessionEnd`: capture a best-effort refinement from dirty state and active work.

The installer (`scripts/install-hooks.mjs`) manages only file-lock hooks for session-wide enforcement. `Stop`, `SessionEnd`, and `UserPromptSubmit` are skill-scoped and run while this skill is loaded.

## Agentic guardrails

- Current code, tests, and user instructions beat memory.
- Store evidence, decisions, and judgment notes; do not store secrets or raw private reasoning.
- Use dry-run previews before destructive cleanup.
- Keep hook scripts fast and fail-open except for genuine lock conflicts or explicit verification gates.
- Do not make sleep destructive or time-based; use an audit result.
- When a repeated manual pattern becomes mechanical, propose a script or command. A future `sleep-audit` command should be deterministic and preview-first, not an automatic cleanup daemon.
