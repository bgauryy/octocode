# Hooks And Host Integration

Hooks automate Awareness lifecycle edges; the CLI works without them. All hosts call
the same runtime and canonical SQLite database.

## Lifecycle

| Event | Behavior | Output/blocking |
|---|---|---|
| Prompt/session start | Register agent; deliver changed operational state plus at most one prompt-grounded memory lead. | Silent for unrelated memory or an unchanged fingerprint. |
| Before write | Run harness guard, resolve task/explicit work, declare advisory path; honor exclusivity. | Silent normally; compact peer delta; host-native denial on guard or exclusive conflict. |
| After write | Write edit audit and heartbeat; keep a scoped automatic HOOK active. | Best-effort, nonblocking. |
| Stop/subagent stop | Finalize the scoped HOOK once, then audit verification debt. | First 3 items + omitted count; block/remind where supported. |
| Session end/compact | Finalize scoped HOOK state, capture deduplicated handoff, and close the session. | Best-effort, nonblocking. |

Pre-edit is the single guard+presence hook. The old separate harness-guard install
entry is removed during install/repair to guarantee guard ordering.

## Host Support

| Host | Surface | Notes |
|---|---|---|
| Claude Code | Skill frontmatter while active, or `.claude/settings.json` | Project-wide install is separate from skill activation. |
| Codex | `.codex/hooks.json` | PreCompact substitutes for SessionEnd; project and hook-definition trust are separate runtime gates. |
| Cursor | `.cursor/hooks.json` | Native JSON outputs; model-context delivery varies by version/surface, so smoke local/cloud separately. |
| Pi | `wirePiAwarenessHooks(pi)` / Pi extension | In-process; never run shell hook install. |
| Custom | Library API or `hook run` payload | Must provide stable identity/path events. |

## Install And Verify

Preview writes, install after approval, then check exact host config:

```bash
octocode-awareness hooks install --host <claude|codex|cursor> \
  --project-dir . --dry-run --compact
octocode-awareness hooks install --host <claude|codex|cursor> \
  --project-dir . --compact
octocode-awareness hooks check --host <claude|codex|cursor> \
  --project-dir . --strict --compact
```

Remove (preview first) when uninstalling host wiring:

```bash
octocode-awareness hooks remove --host <claude|codex|cursor> \
  --project-dir . --dry-run --compact
octocode-awareness hooks remove --host <claude|codex|cursor> \
  --project-dir . --compact
```

Installers modify only Awareness-owned entries, quote command paths, add a Codex
Windows command, and repair obsolete paths/standalone guard entries.

`hooks check --strict` is deliberately config-scoped. Read:

- `health.config`: whether Awareness-owned entries are exact;
- `health.runtime`: always `unverified` until a real event fires;
- Codex runtime notes: project trust, hook-definition trust, and feature enablement
  are not discoverable from the config file alone;
- Cursor runtime notes: local/cloud and model-context delivery require separate
  smoke checks; flat hook config has no guaranteed Windows command override.

After installation, edit a harmless file and confirm:

1. `work list` shows the active task/explicit presence, or fallback enters Verify.
2. Two ordinary agents can share a file and receive one changed-peer summary.
3. An explicit exclusive run blocks the second agent before presence.
4. `verify audit` clears only after the declared check and `verify mark`.

Config-ready does not mean runtime-ready. Confirm where the host sends stdout/stderr,
that the exact hook runs, and that model-visible context or continuation arrives.

## Identity And Run Resolution

Identity order: `OCTOCODE_AGENT_ID`, payload agent, payload session, then a warned
host/workspace fallback. Set one stable ID so manual commands and hooks agree.

Pre-edit resolves the run in this order:

1. exactly one live task claim for the agent/workspace;
2. an explicit active WORK run already declaring the target path;
3. the active fallback HOOK for the same agent, stable session/transcript,
   workspace, and artifact; otherwise a new fallback.

Post-edit keeps that fallback active and attaches further files. Stop/agent-end
finalizes it once to PENDING, so N edits produce one item with N files.
TASK and explicit WORK are never merged. Without stable session/transcript identity,
post-edit uses the isolated per-event lifecycle rather than guessing across sessions.
Shell get-or-create is cross-process locked; Pi coalesces its synchronous in-process
tool callbacks. Session end/shutdown safely finalizes any remaining aggregate.

Fallback verification plans name up to three files plus an omitted count and require
the smallest relevant test/typecheck, diff inspection, and a recorded result. Recursive
Stop audits again only when continuation edits finalize new debt, avoiding silent loss
without looping forever on unchanged debt.

## Guard

The pre-edit wrapper exports `OCTOCODE_SKILL_ROOT`; the runner evaluates the guard
before touching `run_files`.

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
- A missing correlation never marks success; TTL and verification audit expose debt.

Environment controls:

| Variable | Effect |
|---|---|
| `OCTOCODE_AGENT_ID` | Stable cooperative identity. |
| `OCTOCODE_MEMORY_HOME` | Canonical DB directory. |
| `OCTOCODE_NO_VERIFY_GATE=1` | Disable stop gate only with replacement process. |
| `OCTOCODE_NO_NOTIFY=1` | Disable prompt briefing. |
| `OCTOCODE_NO_SESSION_CAPTURE=1` | Disable automatic handoff capture. |
| `OCTOCODE_NOTIFY_RUN_DIGEST=1` | Opt in to a scoped, deduped prompt-time maintenance preview; never applies cleanup. |
| `OCTOCODE_ALLOW_HARNESS_APPLY=1` | Open harness edit gate; branch rule still applies. |

Shell/Pi parity, wrapper extraction, installer repair, peer dedupe, guard order, and
verification caps are covered by focused tests.
