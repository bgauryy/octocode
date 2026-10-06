# Host-side lease admission

Leases stay advisory: a SQLite row cannot fence an OS write. A host can still reject a structured file-edit call that lacks a covering lease. This stops accidental unleased `write`/`edit` calls, not shell commands, subprocesses, custom tools or editor actions.

## Shared read-only check

```
agents-communication check_write '{"paths":[{"path":"src/module.ts"},{"path":"tests/module.test.ts"}]}' \
  --session SESSION_ID --workspace /absolute/repo --database /absolute/store.sqlite
```

`check_write` is a CLI-only host seam, not a model-facing tool. It takes 1–32 `{path, kind?}` targets, including absent files; `kind` defaults to `file`, and tree targets, workspace escapes and existing directories are rejected. It uses the acquisition path resolver and caseless namespace. Check both endpoints of a file rename; recursive directory mutations need a tree-aware host integration.

One read snapshot checks active identity and live owned file/tree coverage, plus the native binding when adapters pass `vendorSession`. It never acquires, renews, stages or writes. `ok:false` or an error means the host must not admit the write. `checks` gives coverage, lease ID and effective expiry (capped by owner presence), with `checkedAt` and `advisory:true`. The result can go stale on expiry, release, input rewrite or filesystem change.

## Check setup before editing

| Setup | Covered operations | Configuration evidence |
| --- | --- | --- |
| Pi `tools: 'editing'` | `write`, `edit` | Automatic; `controller.getGuardCapabilities()` reports `configured` and `bound` |
| Pi other profiles | None by default | Set `requireLeases:true`; same capability receipt |
| Claude guard | `Write`, `Edit`, `MultiEdit`, `NotebookEdit` | `--config` stderr receipt `configured:false` (settings preview only) |
| OpenCode guard | `write`, `edit` | Explicit plugin factory and native-session map; import alone installs nothing |
| Claude/Codex/Cursor/Grok message hooks | None | `host-config` stderr receipt `configured:false`, no edit operations; Claude guard is a separate opt-in |
| Codex | No bundled guard | `host-config --vendor codex` previews messaging hooks only |

Native settings stay unchanged on stdout; a preview receipt never claims a host loaded a hook. Before you rely on admission, check in the real host that an unleased structured write is blocked and a leased one succeeds.

## Pi editing setup

Load the inbox adapter from a trusted extension:

```js
import { registerPiInbox } from '/absolute/runtime/scripts/pi-inbox.mjs';

export default function (pi) {
  registerPiInbox(pi, {
    binary: '/absolute/runtime/scripts/agents-communication',
    workspace: '/absolute/repo',
    database: '/absolute/store.sqlite',
    tools: 'editing',
  });
}
```

The editing profile enables admission and rejects `requireLeases:false`; other profiles opt in with `requireLeases:true`. A `tool_call` handler resolves `input.path` of `write`/`edit` from the event cwd through the shared `hooks/lease-check.mjs`. Missing/stale bindings or paths, failed coverage, expired results and CLI failures block before the tool runs. Nothing auto-acquires: the agent coordinates and retries. Identity context states once per binding whether the guard is on; `getGuardCapabilities()` checks without model calls or DB writes.

Not covered: `bash`, `powershell`, custom tools, user terminals, other processes. Later Pi handlers can rewrite input: use a trusted extension order. Admission is not a sandbox; removing the extension removes it. See [Pi extension contracts](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md) and [event types](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts).

## Claude opt-in

The optional Node adapter checks only `PreToolUse` `Write`/`Edit`/`MultiEdit`/`NotebookEdit` (`NotebookEdit` reads `notebook_path`). Preview with the DB identity and its exact native session ID (the identity's `vendorSession` must match):

```sh
node /absolute/runtime/scripts/hooks/claude-lease-guard.mjs --config \
  --binary /absolute/runtime/scripts/agents-communication \
  --workspace /absolute/repo --database /absolute/store.sqlite \
  --session DB_SESSION_ID --host-session CLAUDE_SESSION_UUID
```

It prints a POSIX-shell-safe settings fragment and writes nothing. Merge it into the session's settings beside existing hooks, or pass it via `claude --settings` with the same UUID via `--session-id`. Never disable hooks. A resumed session with a new native ID needs a new binding. Use the bundled CLI version.

Per call it checks session and cwd, then one read-only `check_write`: failures or malformed/stale input deny; covered calls return `{}` so normal permissions still apply. It acquires and renews nothing and must not run async. Only this adapter needs Node.

Not covered: Bash/MCP/custom tools, OS writes, or a hook that fails to start or is killed before it answers. See [Claude's hook contract](https://code.claude.com/docs/en/hooks).

## OpenCode opt-in

Create a trusted local plugin from the factory (importing installs nothing) and bind every participating native session to its DB identity:

```js
import { createOpenCodeLeaseGuard } from '/absolute/runtime/scripts/hooks/opencode-lease-guard.mjs';

export const CommunicationLeaseGuard = createOpenCodeLeaseGuard({
  binary: '/absolute/runtime/scripts/agents-communication',
  workspace: '/absolute/repo',
  database: '/absolute/store.sqlite',
  sessions: { ses_NATIVE_ID: 'DB_SESSION_ID' },
});
```

OpenCode loads trusted plugins from `.opencode/plugins/`. The `tool.execute.before` handler guards built-in `write`/`edit` (absolute `args.filePath`); unbound sessions, failed checks and in-flight argument changes throw. The session map is copied at creation, so reconfigure for new sessions and use it only where every participating session is mapped. It creates and consumes nothing. Shell, `apply_patch`, custom tools, other plugins' rewrites and formatter side effects stay outside. See the [plugin guide](https://opencode.ai/docs/plugins/).

## Other hosts: supported seams, not installed adapters

| Host | Documented pre-execution seam | Material limits |
| --- | --- | --- |
| Codex | `PreToolUse` for `apply_patch`, Bash and local/MCP tools | Patch text needs an exact parser; some paths bypass hooks; `write_stdin` does not rerun the pre-hook |
| Grok | `PreToolUse` with explicit deny; Claude-compatible matcher aliases | `Write`/`Edit` alias to `search_replace`; crashes/timeouts/malformed output fail open |

These seams ([Codex](https://learn.chatgpt.com/docs/hooks), [Grok](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md)) are neither installed nor tested. Message/lifecycle hooks do not enforce leases; never relabel them as edit guards.

All three guards reject parent (`..`) traversal, leading `@`/`~`, file URLs and Unicode-space aliases instead of guessing host rewrites; on Windows also slash-root and drive-relative aliases. Use a plain repository-relative or absolute path (OpenCode: absolute). Reason: Pi normalizes lexically while the Python resolver follows physical traversal, so with symlinks a lease for one target could admit another. Plain symlink paths still get Python canonical checks.

Stronger isolation needs a host sandbox that removes other mutation paths, or a mutation service that checks ownership inside the write; shell-text regex is no substitute.

## Verification boundary

The source repository's guard tests (generic, Pi, Claude and OpenCode) run the real CLI against deterministic host fixtures: unleased structured writes are blocked before the side effect, leased writes proceed. They prove structured-write gates only, not shell/custom-tool enforcement or hostile-extension resistance.
