# Host-side lease admission

Path leases remain advisory across vendors. A SQLite row cannot fence an arbitrary
OS write. The host can make a useful narrower guarantee: reject a supported,
structured file-edit call when the agent does not currently hold its covering lease.
This closes accidental unleased `write`/`edit` calls without pretending to parse every
shell command, subprocess, custom tool, or editor action.

## Shared read-only check

```
agents-communication check_write '{"paths":[{"path":"src/module.ts"},{"path":"tests/module.test.ts"}]}' \
  --session SESSION_ID --workspace /absolute/repo --database /absolute/store.sqlite
```

`check_write` is a CLI-only host integration seam, not another model-facing tool.
It accepts 1–32 `{path, kind?}` targets, including absent files; `kind` defaults to `file` and tree targets are rejected. It uses the same path
resolver and conservative caseless namespace as acquisition. Workspace escapes and
existing directory targets are rejected. Check both concrete endpoints of a file
rename; recursive directory mutations need a separate tree-aware host integration.

The command checks active identity and owned live file/tree coverage in one read
snapshot. Host adapters also pass `vendorSession` to validate the native session
binding in that same snapshot; generic raw callers may omit it. It never acquires, renews, stages messages, or changes storage. `ok:false`
means at least one path lacks coverage; errors also mean the host must not admit a
guarded write. `checks` identifies coverage and the lease ID/effective expiry, capped
by owner presence. `checkedAt` is the snapshot time and `advisory:true` remains explicit.
The result is not an enduring capability: expiry, release, a later extension rewrite,
or filesystem topology changes can invalidate it after the check.

## Check setup before editing

| Setup | Covered operations | Configuration evidence |
| --- | --- | --- |
| Pi `tools: 'editing'` | `write`, `edit` | Enabled automatically; `controller.getGuardCapabilities()` reports `configured` and current `bound` state |
| Pi other profiles | None by default | Set `requireLeases:true` explicitly; the same capability receipt reports the result |
| Claude guard | `Write`, `Edit` | `--config` emits a stderr capability receipt with `configured:false`: it is only a settings preview |
| OpenCode guard | `write`, `edit` | Explicit plugin factory and native-session map; importing alone installs nothing |
| Cursor/Grok message hooks | None | `host-config` emits a stderr capability receipt with `configured:false` and no supported edit operations |
| Codex | No bundled guard | `host-config --vendor codex` rejects unsupported setup |

Native settings remain on stdout unchanged. Preview receipts never claim that a
host loaded or enabled a hook. Verify a blocked unleased structured write and a
successful leased write in the actual configured host before relying on admission.
None of these receipts promise shell/custom-tool or operating-system fencing.

## Pi editing setup

Configure a trusted extension to load the existing inbox adapter:

```js
import { registerPiInbox } from '/absolute/skill/scripts/pi-inbox.mjs';

export default function (pi) {
  registerPiInbox(pi, {
    binary: '/absolute/skill/scripts/agents-communication',
    workspace: '/absolute/repo',
    database: '/absolute/store.sqlite',
    tools: 'editing',
  });
}
```

The editing tool profile enables admission by default and rejects
`requireLeases:false`. Other profiles retain their messaging behavior; explicitly
set `requireLeases:true` when they also use structured writes. The adapter registers a
`tool_call` handler for Pi's structured `write` and `edit` tools. It resolves their
`input.path` from the event's working directory through the same
`hooks/lease-check.mjs` admission call as the Claude and OpenCode guards. Missing/stale bindings, missing paths, failed coverage, expired
results, and CLI failures return an explicit block before that tool runs. The
adapter never auto-acquires a lease; the agent must coordinate and retry.
Its initial identity context states whether the guard is configured once per
binding; this is not repeated on every event. The returned controller exposes
`getGuardCapabilities()` for host checks without model calls or database writes.

This gate does not cover `bash`, `powershell`, arbitrary custom tools, user terminal
commands, or unrelated processes. Pi allows later handlers to rewrite input; use a
trusted extension order and do not treat admission as a sandbox. Disabling or
removing the extension also removes its gate. [Pi's extension contracts](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)
and [event types](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts)
document the blocking event and mutable input.

## Claude opt-in

The optional Node adapter checks only Claude's structured `PreToolUse` `Write` and
`Edit` calls. Generate a preview with the existing DB identity and its exact native
Claude session ID (the DB identity must have matching `vendorSession`):

```sh
node /absolute/skill/scripts/hooks/claude-lease-guard.mjs --config \
  --binary /absolute/skill/scripts/agents-communication \
  --workspace /absolute/repo --database /absolute/store.sqlite \
  --session DB_SESSION_ID --host-session CLAUDE_SESSION_UUID
```

This prints a POSIX-shell-safe settings fragment; it writes no settings and creates
no identity. Merge the fragment into the intended session's settings, preserving
existing lifecycle/message hooks. For an owned CLI session, pass that fragment via
`claude --settings` and the same UUID via `--session-id`. Do not use options that
disable hooks. A resumed session with a different native ID needs a new binding.
Use this adapter and its matching bundled CLI together.

For supported calls, it checks the event's session and working directory, then makes
one read-only Python `check_write` call. Failed coverage, malformed input, missing or
stale binding, and checker failures return an explicit deny. Covered calls return
`{}` so normal host permission checks still apply. The guard does not renew presence
or acquire a lease, and must not be configured as an async hook. Node is optional
for this adapter; raw communication remains a standalone Python CLI.

This covers neither Bash/MCP/custom tools nor OS writes. If Node cannot start, the
hook is disabled, or the host terminates it before a response, the adapter cannot
supply its denial. Host failure semantics and later hook input rewrites remain
outside this gate. [Claude's hook contract](https://code.claude.com/docs/en/hooks)
describes the structured inputs and permission decision.

## OpenCode opt-in

Create a trusted local plugin using the factory; importing the helper alone installs
nothing. Bind every participating native session explicitly to its DB identity:

```js
import { createOpenCodeLeaseGuard } from '/absolute/skill/scripts/hooks/opencode-lease-guard.mjs';

export const CommunicationLeaseGuard = createOpenCodeLeaseGuard({
  binary: '/absolute/skill/scripts/agents-communication',
  workspace: '/absolute/repo',
  database: '/absolute/store.sqlite',
  sessions: { ses_NATIVE_ID: 'DB_SESSION_ID' },
});
```

OpenCode loads trusted plugins from `.opencode/plugins/`. This factory returns a
`tool.execute.before` handler for the built-in `write` and `edit` tools and their
`args.filePath` (absolute paths only, matching the documented tool contract).
Unbound native sessions, failed checks and in-flight argument changes
throw before execution. The mapping is copied at creation; restart/reconfigure it
for new native sessions. Restrict this opt-in plugin to hosts where all participating
sessions have mappings. It does not discover identities, create sessions, call models,
or consume messages. Shell, `apply_patch`, custom tools, post-check rewrites by other
plugins and formatter side effects remain outside coverage. Disable alternative
mutation paths separately when a stronger host policy is required.

The [plugin hook types](https://github.com/anomalyco/opencode/blob/dev/packages/plugin/src/index.ts),
[write schema](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/tool/write.ts),
[edit schema](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/tool/edit.ts)
and [plugin guide](https://opencode.ai/docs/plugins/) establish this integration seam.

## Other hosts: supported seams, not installed adapters

| Host | Documented pre-execution seam | Material limits |
| --- | --- | --- |
| Codex | `PreToolUse` for `apply_patch`, Bash and local/MCP tools | Patch text needs an exact parser; some specialized paths bypass hooks and `write_stdin` does not rerun the pre-hook |
| Grok | `PreToolUse` with explicit deny; Claude-compatible matcher aliases | `Write`/`Edit` alias to `search_replace`; crashes/timeouts/malformed output fail open |

Sources: [Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Grok hooks](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md).
These sources establish available integration seams, not installed or live-tested
lease guards for Codex or Grok. Existing message/lifecycle hooks do not enforce
leases and must not be relabeled as edit guards.

All three structured guards reject parent (`..`) traversal, leading `@`/`~`,
file URLs and Unicode-space aliases rather than guessing how each host rewrites them.
Use a plain repository-relative or absolute path; OpenCode requires an absolute one.
On Windows, shell-style slash-root and drive-relative aliases are also rejected.
This matters for symlink/parent combinations: Pi lexically normalizes a path before
writing, while the Python lease resolver follows physical traversal. Rejecting these
ambiguous inputs prevents a lease for one target from admitting a different target.
[Pi's path resolver](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/utils/paths.ts)
and [write implementation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/tools/write.ts)
establish that distinction. Plain symlink paths still undergo Python canonical checks.

Stronger write isolation requires host permissions/sandbox policy that removes
alternative mutation paths, or a trusted mutation service that checks ownership
inside the write operation. Regex inspection of shell text is not a substitute.

## Verification boundary

`tests/lease-guard.test.mjs` exercises real CLI admission, own versus foreign leases,
file/tree coverage, missing files, expiry, workspace escapes, invalid inputs, and
read-only operation while another connection owns the SQLite writer.
`tests/pi-lease-guard.test.mjs` invokes the real adapter with a deterministic Pi event
fixture and the real Python CLI, proving an unleased structured write is blocked before
the simulated side effect and a leased write proceeds. It also checks event-relative
paths, stale identity, unavailable presence, explicit opt-in, and uncovered tools.
These deterministic fixtures are not a claim of universal filesystem enforcement.

`tests/claude-lease-guard.test.mjs` and `tests/opencode-lease-guard.test.mjs` run
real-CLI fixtures for denial, coverage, exact host binding, path/cwd handling,
expiry, checker errors, read-only operation under a held writer, ignored tools,
shell-safe settings previews, rejection before a simulated side effect, native
identity mismatch and input mutation during the check.

Single live trials with installed Pi, Claude Code and OpenCode hosts each rejected
one unleased structured write without creating the file, then accepted the same
write after host lease acquisition. Their receipts and reproducible probes stay in
the local `.octocode/benchmarks/communication-{pi,claude,opencode}-lease-guard/`
artifacts. The OpenCode trial used a deterministic provider fixture, not live model
inference. The trials predate the single-call `check_write` binding and the
ambiguous-path rejection, which the deterministic fixtures cover. They demonstrate
the structured-write gates only, not shell/custom-tool enforcement or
hostile-extension resistance.
