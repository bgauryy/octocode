# Hooks And Host Integration

**Audience**: maintainers installing or debugging awareness automation in Claude Code, Codex, Cursor, Pi, or custom hosts.

Hooks automate the lifecycle edges that agents often forget: claim before writing, release after writing, verify before stopping, brief before prompts, and capture handoff state at the end.

The CLI works without hooks. Hooks make the workflow harder to skip.

## Host Support

Supported agents: **Codex**, **Claude Code**, **Cursor**, and **Pi**.

| Host | Integration |
|---|---|
| Claude Code | `SKILL.md` frontmatter hooks can run automatically when the skill is active. |
| Codex | Hook config via `.codex/hooks.json`; `SKILL.md` frontmatter is not enough. |
| Cursor | Hook config via `.cursor/hooks.json`; `SKILL.md` frontmatter is not enough. |
| Pi | In-process bridge through `wirePiAwarenessHooks(pi)`; pass `skillRoot` or set `OCTOCODE_SKILL_ROOT` to enable native harness self-edit guarding. |
| Custom | Call the library APIs or invoke the CLI/hook runner with host payloads. |

## Hook Lifecycle

| Event | Script | Purpose |
|---|---|---|
| `UserPromptSubmit` | `notify-deliver.sh` | Smart briefing: register/touch agent and surface unread signals/context. |
| `PreToolUse` write | `pre-edit.sh` | Claim target files before writes. |
| `PreToolUse` write | `harness-guard.sh` | Block skill/harness self-edits unless explicitly allowed. |
| `PostToolUse` write | `post-edit.sh` | Release locks as `PENDING` after writes. |
| `Stop` / `SubagentStop` | `stop-verify.sh` | Block exit while pending verification remains. |
| `SessionEnd` / `PreCompact` | `session-end.sh` | Capture handoff/refinement state. |

## Install, Check, Remove

Always preview first:

```bash
octocode-awareness hooks install \
  --host codex \
  --project-dir . \
  --dry-run \
  --compact
```

Install:

```bash
octocode-awareness hooks install --host codex --project-dir . --compact
octocode-awareness hooks install --host cursor --project-dir . --compact
octocode-awareness hooks install --host claude --compact
```

Check exact installed commands and fail on missing/drifted entries:

```bash
octocode-awareness hooks check --host codex --project-dir . --strict --compact
```

Remove awareness-owned hooks:

```bash
octocode-awareness hooks remove --host codex --project-dir . --dry-run --compact
```

Use `--global` for user-scope installation instead of project-scope installation.

## Smart Briefing

`notify-deliver.sh` runs before a prompt where the host supports it. It can surface:

- unread signals,
- active locks or pending verification,
- relevant memory/refinement context,
- agent registration/last-seen updates.

This is a briefing, not a substitute for validation. Agents should still verify remembered facts against current files before acting.

## Harness Guard

`harness-guard.sh` protects files that define the harness itself, such as skill docs and scripts.

Decision flow:

```text
write targets harness/skill files?
  no  -> allow
  yes -> OCTOCODE_ALLOW_HARNESS_APPLY=1?
           no  -> block
           yes -> on non-main branch?
                    yes -> allow
                    no  -> block unless explicitly permitted for detached/no-branch cases
```

Environment controls:

| Variable | Effect |
|---|---|
| `OCTOCODE_ALLOW_HARNESS_APPLY=1` | Allows harness edits if branch policy also passes. |
| `OCTOCODE_HARNESS_BRANCH_OK=1` | Allows detached/no-branch cases when explicitly needed. |
| `OCTOCODE_NO_VERIFY_GATE=1` | Disables stop-time verification blocking. |
| `OCTOCODE_NOTIFY_RUN_DIGEST=1` | Lets prompt-time smart briefing run the periodic maintenance digest before listing signals. |
| `OCTOCODE_AGENT_ID` | Stable identity for hosts that do not provide one. |
| `OCTOCODE_MEMORY_HOME` | Directory containing the canonical `awareness.sqlite3` DB, normally under global `~/.octocode/memory`. |

If a shell-hook host provides no `OCTOCODE_AGENT_ID` and no payload session or agent id, the hook runner derives a stable local fallback from host + workspace and warns. That fallback keeps pre/post hooks paired, but it is not strong multi-agent isolation; set `OCTOCODE_AGENT_ID` for shared workspaces.

## Failure Behavior

Hooks should protect work without making the editor unusable:

| Hook | Blocking behavior |
|---|---|
| `pre-edit` | Blocks on real lock conflicts. Other unexpected errors should fail open with a warning. |
| `harness-guard` | Blocks protected harness edits unless approval env/branch checks pass. |
| `stop-verify` | Blocks exit on pending verification unless `OCTOCODE_NO_VERIFY_GATE=1`. |
| `post-edit`, `session-end`, `notify-deliver` | Should not block normal work on non-critical failures. |

The shipped shell wrappers warn to stderr if the bundled `hook-runner.mjs` is missing, then exit 0. That keeps broken installs from blocking the editor while still making the failure visible in hook logs.

**Stderr visibility depends on host wiring, not just the hook script.** "Fail open with a warning" only helps if an agent (or a human) actually sees that stderr line. Some hosts surface tool/hook stderr directly in the transcript; others swallow it unless the session is run with verbose/debug logging, or only persist it to a log file the agent never reads. Before trusting a fail-open warning to be noticed in a given host, confirm where that host routes hook stderr (transcript, log file, or nowhere) — otherwise a broken hook install can silently degrade to "no awareness enforcement" with no visible signal.

## Pi Bridge

Pi does not need shell hooks. `wirePiAwarenessHooks(pi)` wires lifecycle behavior in-process and uses the same database and library functions. When the bridge receives `skillRoot` or `OCTOCODE_SKILL_ROOT`, Pi write tools also use the same harness self-edit approval rules as shell hosts.

Pi equivalents include:

| Pi tool | CLI equivalent |
|---|---|
| `workspace_status` | `workspace status` |
| `memory_recall` | `memory recall` |
| `file_lock` lock/release/status/renew | `lock acquire/release`, `workspace status` |
| `memory_verify` | `verify mark` |
| `memory_audit_unverified` | `verify audit` |
| `agent_signal` | `signal publish/list/reply/ack/resolve` |
| `memory_reflect` | `reflect record` |
| `memory_export_harness` | `reflect export-harness` |

## Known Gaps

- Bundled shell hooks and the Pi bridge insert best-effort `update` rows into `edit_log` for extracted paths; custom hosts still need `insertEditLog()` for richer diff metrics, renames, deletes, or host-specific edit metadata.
- Codex and Cursor require explicit hook config; skill frontmatter alone does not run there.
- Hook payload formats differ by host, so path extraction must be tested per host.
- Generated hook config should be previewed with `--dry-run` before writing and checked with `--strict`.
