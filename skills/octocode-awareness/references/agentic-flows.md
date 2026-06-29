# Agentic Flows

Use this when deciding how to combine Awareness' manual loop, lifecycle hooks, subagent handoffs, reflection, and cleanup. The goal is one operating model: the skill teaches the agent what to do, hooks catch lifecycle moments, and reflection turns outcomes into better future behavior.

## Three flow layers

| Layer | Handles | Use it for | Avoid using it for |
|-------|---------|------------|--------------------|
| Skill loop | `get-memory`, `refine-get`, `status`, `pre-flight-intent`, `verify`, `refine-set`, `reflect` | Intentional work: attend, focus, claim, verify, encode, sleep | Automatic enforcement by itself |
| Hooks | `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`, `SessionEnd` | Lifecycle guardrails: deliver messages, claim files, keep verification visible, capture handoffs | Deciding that the artifact is correct |
| Agentic loop | `notify`, `reflect --duo`, `--eval-failure-json`, `mine-weakness`, `export-harness`, `harness-apply` | Peer coordination, critique, recurring failure mining, harness improvements | Storing raw private reasoning or unattended self-modification |

Hooks should make the right behavior harder to forget. They do not replace the agent's judgment, test plan, or explicit verification.

## Recommended compositions

### Read-only research

Run `get-memory --smart`, `refine-get`, `status`, and `notify-get`. Treat memories and notifications as leads, then prove claims from current files, commands, or Octocode research. No file lock is needed unless the work will write files.

### Single-agent edit

1. Attend: recall memory, handoff, status, and messages.
2. Focus: choose the smallest file set and test plan.
3. Claim: call `pre-flight-intent`; hooks may also claim during edit tools.
4. Work: edit only the claimed files.
5. Verify: run the declared checks and record them with `verify`.
6. Encode: write a refinement or memory only if it changes a future decision.
7. Sleep: audit idle state, reflect, release or confirm released locks, and prune only with dry-run evidence.

If hooks are active, `PreToolUse` claims and `PostToolUse` releases the live lock as `PENDING`, but the agent still owes verification before claiming success.

### Multi-agent or subagent work

Set a stable `OCTOCODE_AGENT_ID` when possible so hook-managed and manual calls share identity. Use parent/child names such as `codex/research-web` when delegating. Default subagents to read-only research or review unless writes are clearly disjoint; keep final write integration single-threaded.

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

Store the receipt with `notify --kind handoff` for live coordination or `refine-set` when the next run must inherit it. Do not store raw transcripts. `SubagentStop` can flag missing verification, but the parent agent still reads the evidence anchors, runs or records the declared verification, and decides what survives.

### Harness improvement

Use `reflect --duo` for ambiguous or substantial outcomes. Use `--eval-failure-json` when another skill emits structured failures, then `mine-weakness` to find repeated signatures. Preserve the path `trace -> finding -> eval target -> bounded task`: group repeated failures before changing the harness, and make each proposed fix small enough to verify. Use `export-harness` to preview proposed changes. Apply changes to this skill only through the gated `harness-apply` path with human approval and a dedicated branch.

### Sleep cleanup

Sleep runs at end-of-work, session end, subagent handoff, or explicit cleanup. It is not triggered by quiet time alone. Audit first with `status`, `audit-unverified`, `notify-get`, `refine-get`, `forget --dry-run`, and `notify-prune --dry-run`. Then record verification, reflect, mark handoffs done when true, supersede stale memories, prune resolved messages, update corpus docs only for stable reusable knowledge, and release any remaining locks.

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
