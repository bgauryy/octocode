# Hooks And Host Integration

Hooks automate Awareness lifecycle edges; the CLI works without them. All hosts call
the same runtime and canonical SQLite database.

## Lifecycle

| Event | Behavior | Output/blocking |
|---|---|---|
| Prompt/session start | Register agent; deliver changed signals/memory/refinement briefing. | Silent when fingerprint unchanged. |
| Before write | Run harness guard, resolve task/explicit work, declare advisory path; honor exclusivity. | Silent normally; compact peer delta; exit/block 2 on guard or exclusive conflict. |
| After write | Write edit audit and heartbeat; end automatic HOOK fallback. | Best-effort, nonblocking. |
| Stop/subagent stop | Audit verification debt. | First 3 items + omitted count; block/remind where supported. |
| Session end/compact | Capture deduplicated handoff and close session state. | Best-effort, nonblocking. |

Pre-edit is the single guard+presence hook. The old separate harness-guard install
entry is removed during install/repair to guarantee guard ordering.

## Host Support

| Host | Surface | Notes |
|---|---|---|
| Claude Code | Skill frontmatter while active, or `.claude/settings.json` | Project-wide install is separate from skill activation. |
| Codex | `.codex/hooks.json` | Enable host hooks; PreCompact substitutes for missing SessionEnd. |
| Cursor | `.cursor/hooks.json` | Cloud supports fewer events; smoke local/cloud separately. |
| Pi | `wirePiAwarenessHooks(pi)` / Pi extension | In-process; never run shell hook install. |
| Custom | Library API or `hook run` payload | Must provide stable identity/path events. |

## Install And Verify

Preview writes, install after approval, then check exact host wiring:

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

Installers modify only Awareness-owned entries and repair obsolete command
paths/standalone guard entries.

After installation, edit a harmless file and confirm:

1. `work list` shows the active task/explicit presence, or fallback enters Verify.
2. Two ordinary agents can share a file and receive one changed-peer summary.
3. An explicit exclusive run blocks the second agent before presence.
4. `verify audit` clears only after the declared check and `verify mark`.

Installed does not mean enabled. Confirm where the host sends stdout/stderr and that
the hook actually executes.

## Identity And Run Resolution

Identity order: `OCTOCODE_AGENT_ID`, payload agent, payload session, then a warned
host/workspace fallback. Set one stable ID so manual commands and hooks agree.

Pre-edit resolves the run in this order:

1. exactly one live task claim for the agent/workspace;
2. an explicit active WORK run already declaring the target path;
3. a new isolated HOOK run.

It never groups work by host session alone. Ambiguous matches do not guess.

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

Hook payloads emit one host-appropriate `additionalContext` field, not duplicated
camel/snake aliases. Peer summaries cap detail and expose omitted counts.

## Failure Behavior

- Real exclusive conflict and harness denial block before write.
- Stop gate blocks/reminds on real verification debt where the host permits.
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
