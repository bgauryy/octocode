# Hooks And Host Integration

Hooks automate Awareness lifecycle edges; the CLI works without them. All hosts call
the same runtime and canonical SQLite database.

## Lifecycle

| Event | Behavior | Output/blocking |
|---|---|---|
| Prompt/session start | Register agent; deliver changed operational state plus at most one prompt-grounded memory lead. | Silent for unrelated memory or an unchanged fingerprint. |
| Before write | Run harness guard, resolve task/explicit work, declare advisory path; honor exclusivity. | Silent normally; compact peer delta; host-native denial on guard or exclusive conflict. |
| Successful write | Write edit audit and heartbeat; keep a scoped automatic HOOK active. | Best-effort, nonblocking. |
| Failed write | Discard hook-created path presence that has no successful edit audit. | No edit audit or verification debt for a change that never happened. |
| Subagent start | Register the host child identity and deliver changed state where the host supports child-context injection. | Cursor registration is useful, but child-context injection remains version/surface-dependent. |
| Stop/subagent stop | Finalize the scoped HOOK once, then audit verification debt. | First 3 items + omitted count; block/remind where supported. |
| PreCompact | Finalize scoped HOOK state and capture a deduplicated handoff; keep the session reusable. | Best-effort, nonblocking. |
| SessionEnd | Finalize/capture, then mark the host session ended without claiming work success. | Best-effort, nonblocking. |

Pre-edit is the single guard+presence hook. The old separate harness-guard install
entry is removed during install/repair to guarantee guard ordering.

## Host Support

| Host | Surface | Notes |
|---|---|---|
| Claude Code | Skill frontmatter while active, or `.claude/settings.json` | Success/failure writes, subagent start/stop, PreCompact, SessionEnd, prompt briefing. Choose one surface. |
| Codex | `.codex/hooks.json` | SessionStart, success writes, subagent start/stop, PreCompact, prompt/stop. No SessionEnd or distinct failure event; PostToolUse failure metadata is handled when present. |
| Cursor | `.cursor/hooks.json` | Success/failure writes plus session/subagent/compact/end edges. Native output varies by local/cloud/version; child-context injection is not assumed. |
| Pi | `wirePiAwarenessHooks(pi)` / Pi extension | Native `session_start`, tool start/end with `isError`, before-agent, compact, agent-end, shutdown; never shell install. |
| Custom | Library API or `hook run` payload | Must provide stable identity/path events. |

## Install And Verify

Codex/Cursor require project config. When Claude skill frontmatter is active, use it
and do not also install project settings; `hooks check` reports that definition as a
separate surface without pretending activation was observed.
Preview writes, install after approval, then check exact host config:

```bash
octocode-awareness hooks install --host <codex|cursor> \
  --project-dir . --dry-run
octocode-awareness hooks install --host <codex|cursor> \
  --project-dir . --compact
octocode-awareness hooks check --host <codex|cursor> \
  --project-dir . --strict --compact
```

Use `--host claude` only when skill frontmatter is unsupported or disabled.

Remove (preview first) when uninstalling host wiring:

```bash
octocode-awareness hooks remove --host <claude|codex|cursor> \
  --project-dir . --dry-run
octocode-awareness hooks remove --host <claude|codex|cursor> \
  --project-dir . --compact
```

Installers modify only recognizable Awareness-owned entries, including obsolete roots
or event placements, while preserving unrelated hooks. Project hooks call the canonical
`hook-runner.mjs` directly; generated `out/` is only a fallback when no project skill
exists. Commands are quoted and Codex gets a Windows command. Strict health verifies
the runner target; a missing target is drift.

`hooks check --strict` remains definition/config-scoped. Runtime receipts are one bounded
SQLite upsert per workspace, host, and event—no payloads or append-only hook logs. Read:

- `surface`: settings or Claude skill frontmatter;
- `health.config` / `health.definition`: whether the selected surface is exact;
- `health.activation`: still unverified when a host cannot expose it;
- `health.runtime`: `unverified`, `observed`, `stale` after seven days, or `failed`;
- `health.coverage` and `last_seen`: which expected events have actually executed;
- Codex runtime notes: project trust, hook-definition trust, and feature enablement
  are not discoverable from the config file alone;
- Cursor runtime notes: local/cloud and model-context delivery require separate
  smoke checks; flat hook config has no guaranteed Windows command override.

After installation, edit a harmless file and confirm:

1. `work list` shows the active task/explicit presence, or fallback enters Verify.
2. Two ordinary agents can share a file and receive one changed-peer summary.
3. An explicit exclusive run blocks the second agent before presence.
4. `verify audit` clears only after the declared check and `verify mark`.
5. A write after PreCompact reuses the session; SessionEnd leaves later conflicts
   with `holder_session_active:false` rather than claiming success.

Config-ready does not mean runtime-ready. Confirm where the host sends stdout/stderr,
that the exact hook runs, and that model-visible context or continuation arrives.

## Identity And Run Resolution

Identity order: host payload agent, `OCTOCODE_AGENT_ID`, payload session, then a
warned host/workspace fallback. A host child ID prevents subagents collapsing into
their parent; set one stable environment ID so main-agent CLI and hook work agree.

Pre-edit resolves the run in this order:

1. exactly one live task claim for the agent/workspace;
2. an explicit active WORK run already declaring the target path;
3. the active fallback HOOK for the same agent, stable session/transcript,
   workspace, and artifact; otherwise a new fallback.

Post-edit keeps that fallback active and attaches further files. Stop/agent-end or
PreCompact finalizes it once to PENDING, so N edits produce one item with N files.
TASK and explicit WORK are never merged. Without stable session/transcript identity,
post-edit uses the isolated per-event lifecycle rather than guessing across sessions.
Shell get-or-create is cross-process locked; Pi coalesces its synchronous in-process
tool callbacks. PreCompact keeps the session reusable; SessionEnd/shutdown safely
finalizes any remaining aggregate and marks the session ended.

Fallback verification plans name up to three files plus an omitted count and require
the smallest relevant test/typecheck, diff inspection, and a recorded result. Recursive
Stop audits again only when continuation edits finalize new debt, avoiding silent loss
without looping forever on unchanged debt.

## Guard

The configured runner receives `--skill-root`; Claude frontmatter wrappers export the
equivalent value. The guard runs before touching `run_files`.

Protected harness/skill edits require:

- explicit user authorization;
- `OCTOCODE_ALLOW_HARNESS_APPLY=1`;
- a non-main branch;
- `OCTOCODE_HARNESS_BRANCH_OK=1` only for explicitly approved detached/non-repo cases.

A denied guard leaves no false file presence.

## Briefing And Peer Dedupe

`delivery_state` stores content fingerprints per consumer/channel/scope. Unchanged
briefings and peer sets emit nothing. This does not acknowledge signals; `signal ack`
remains explicit.

For Claude/Codex prompt hooks and Pi `input`, the current prompt is held only as a
bounded transient query; it is not written to SQLite. The selector searches the
existing scoped memory bank, requires at least two meaningful prompt/memory token
matches across the bounded normal recall pool, emits at most one
`Memory lead — verify` item, and otherwise stays silent.
Signals, overrides, recurring-failure pressure, and open-refinement counts remain
separate operational interventions. This is a deterministic local policy, not a
second reasoning agent, and it never makes recalled text authoritative.

The final hook briefing keeps at most five items, truncates each item by UTF-8 bytes,
and stays within 1 KiB even for multi-byte text. Signal summaries retain the file
count and one bounded file lead; use `signal list` for full bodies and paths.

Claude/Codex emit event-named `hookSpecificOutput.additionalContext`. Cursor emits
native `additional_context` at session start and `agent_message` around tool use;
delivery is best-effort and must be smoked. Peer summaries cap detail and expose
omitted counts.

## Failure Behavior

- Real exclusive conflict and harness denial use exit 2 on Claude/Codex,
  `permission: deny` on Cursor, and `{ block: true }` on Pi.
- Stop debt uses exit 2 on Claude/Codex, Cursor `followup_message`, and Pi follow-up.
- Infrastructure, extraction, post-edit, briefing, and session failures warn and fail
  open so the editor remains usable.
- A failed write rolls back hook-created file presence without an edit audit; TASK or
  explicit WORK ownership is preserved because the user may retry or investigate.
- A missing correlation never marks success; TTL and verification audit expose debt.

Environment controls, read identically by every host (shell hooks and Pi share
`bin/hook-payload.ts` / `src/pi-hooks-inputs.ts`):

| Variable | Effect |
|---|---|
| `OCTOCODE_AGENT_ID` | Stable cooperative identity. |
| `OCTOCODE_AGENT_NAME` | Optional display name attached to registered agent identity. |
| `OCTOCODE_ARTIFACT` (aliases `OCTOCODE_PACKAGE`, `OCTOCODE_SERVICE`) | Scope presence/coordination to one artifact inside a monorepo workspace. |
| `OCTOCODE_MEMORY_HOME` | Canonical DB directory. |
| `OCTOCODE_NO_VERIFY_GATE=1` | Disable stop gate only with replacement process. |
| `OCTOCODE_NO_NOTIFY=1` | Disable prompt briefing. |
| `OCTOCODE_NO_SESSION_CAPTURE=1` | Disable automatic handoff capture. |
| `OCTOCODE_NOTIFY_RUN_DIGEST=1` | Opt in to a scoped, deduped prompt-time maintenance preview; never applies cleanup. |
| `OCTOCODE_NO_DIGEST=1` | Force-disable the digest preview even when `OCTOCODE_NOTIFY_RUN_DIGEST=1` is set. |
| `OCTOCODE_DIGEST_INTERVAL_HOURS` | Minimum hours between digest previews (default 4). |
| `OCTOCODE_ALLOW_HARNESS_APPLY=1` | Open harness edit gate; branch rule still applies. |
| `OCTOCODE_SKILL_ROOT` | Skill root the pre-edit guard checks edits against; exported by the shell wrapper, or passed to `wirePiAwarenessHooks`/`createPiAwarenessBridge` for Pi. Guard is a no-op when unset. |
| `OCTOCODE_HARNESS_BRANCH_OK=1` | Acknowledge a detached/non-repo skill root when the branch cannot be confirmed. |

Invocation plumbing, not ordinary agent configuration: project hooks pass `--host`
and invoke the resolved Node executable directly. Claude skill wrappers instead accept
`OCTOCODE_NODE_BIN` when the host environment must override `node`; host payloads or
the runner's Claude default identify that surface.

Shell/Pi parity, wrapper extraction, installer repair, peer dedupe, guard order, and
verification caps are covered by focused tests.
