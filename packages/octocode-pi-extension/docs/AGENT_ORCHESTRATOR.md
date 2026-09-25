# Agent Orchestrator

`@octocodeai/pi-extension` implements worker orchestration inside Pi's extension/SDK surfaces. It does not fork Pi.

## Pi SDK mapping

| Need | Pi surface | Octocode implementation |
|---|---|---|
| Ordered lifecycle behavior | `pi.on(...)` extension hooks | `src/hook-composer.ts` middleware per event |
| Worker subprocesses | Pi CLI/RPC mode | `spawnRpcAgent` launches `pi --mode rpc` |
| Worker tools | SDK/CLI tool allowlists | `buildPiArgs`, `--no-tools` for an explicit empty allowlist, and recursive-tool exclusion |
| User controls | `pi.registerCommand` | `/octocode-inbox` |
| Live UX | register-once custom footer | bounded non-killed worker rows, attention first (`footer-view.ts`) |
| Full inspection | user command/tool result | `/octocode-inbox` and `agent` lifecycle queries |
| Session cleanup | `session_shutdown` hook | kill active workers and clear footer state |

## Runtime flow

## UX contract

Open `/octocode-inbox`, select a worker, then choose **View output**, **Steer**, or **Stop**. Output opens in a scrollable view with status, handback, and retained output. Steer and Stop are offered while the process is live. Escape closes the picker or inspector without taking an action.

The footer aggregates normal workers, names blocked or failed workers, and links to the inbox. The footer, inbox, event journal, and agent result cards share one display-state policy. A queued follow-up takes precedence over the preceding handback; an exited worker cannot appear steerable.

Worker handbacks are parsed from typed prefixes such as `[EVIDENCE]`, `[CONFIDENCE]`, `[BLOCKED]`, `[DONE]`, and `[FAILED]`. Unstructured output remains available, but normalized handbacks are the default UX because they are smaller and easier for the parent agent to verify.

## Policy contract

The default policy is warning-first. It warns when a worker packet omits recommended sections, when fan-out is high, when recursive tools are requested, or when a Claude/custom-provider-looking model omits `provider`. It blocks only when the active worker cap is reached, before any subprocess is created. Operators can tune caps with `OCTOCODE_AGENT_MAX_ACTIVE` and `OCTOCODE_AGENT_WARNING_ACTIVE`; invalid or non-positive values are ignored.

Parent and workers resolve the same physical database and keep distinct stable IDs.
Each worker uses its physical checkout for ownership and verification. Linked Git
worktrees share peer discovery, messages and memory. The runtime supplies CLI/database/workspace
bindings to guarded `bash`. Use the returned native run/task IDs and receipts;
do not duplicate lifecycle records or turn unverified worker output into memory.

## Rollback

The rollback path is extension-local: remove command/status/widget registration, bypass the hook composer by registering hooks directly, and keep the existing `spawnRpcAgent` worker path. No Pi fork or Pi core migration is required.
