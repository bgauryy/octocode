# Hooks — automatic file-claim enforcement

Read this to understand, tune, or disable bundled hooks, or to make file-claim enforcement session-wide. Hooks turn the "MUST claim a file before editing it" rule from agent memory into harness enforcement.

## What ships with this skill

`SKILL.md` frontmatter defines skill-scoped hooks only for hosts that execute Agent Skills frontmatter hooks, such as Claude-style shell-hook hosts.
Codex and Cursor do **not** execute standalone `SKILL.md` hook frontmatter as native skill metadata.
Install Codex hooks through `.codex/hooks.json`, inline `[hooks]` in `.codex/config.toml`, or an enabled plugin's `hooks/hooks.json`.
Install Cursor hooks through `.cursor/hooks.json`, `~/.cursor/hooks.json`, or a Cursor plugin's `hooks/hooks.json`; Cursor can also load Claude Code hook config from `.claude/settings*.json` when third-party configs are enabled.
Pi also does **not** execute this frontmatter; Pi uses the native adapter exported by `@octocodeai/octocode-awareness` and wired by `@octocodeai/pi-extension`.

| Behavior | Claude-style event | Codex config event | Cursor native event | Pi native event | Script / adapter | Side effect | Verify/audit command |
|-----------------|--------------------|--------------------|---------------------|-----------------|------------------|-------------|----------------------|
| pre-edit | `PreToolUse` on `Write\|Edit\|MultiEdit\|NotebookEdit\|apply_patch\|ApplyPatch` | same matcher in `.codex/hooks.json` | `preToolUse` in `.cursor/hooks.json` | `tool_call` / `tool_execution_start` | `scripts/hooks/pre-edit.sh` / `createPiAwarenessBridge().handleToolCall` | Claims the target file via `lock acquire`; blocks if another agent holds it. | `octocode-awareness workspace status --workspace "$PWD" --compact` should show the lock or conflict. |
| harness self-fix gate | `PreToolUse` on the same matcher | same matcher in `.codex/hooks.json` | `preToolUse` | `tool_call` / `tool_execution_start` | `scripts/hooks/harness-guard.sh` / `createPiAwarenessBridge().handleToolCall` | Blocks skill self-edits unless a human opened `OCTOCODE_ALLOW_HARNESS_APPLY=1` AND the skill root's git branch is a dedicated branch (checked live; `main`/`master` are always blocked; detached HEAD or a non-repo additionally needs `OCTOCODE_HARNESS_BRANCH_OK=1`). Pi uses `skillRoot` or `OCTOCODE_SKILL_ROOT`. | Use `octocode-awareness` staged approval guidance; verify with the requested checks after the approved edit. |
| post-edit | `PostToolUse` on the same matcher | same matcher in `.codex/hooks.json` | `postToolUse` | `tool_result` / `tool_execution_end` | `scripts/hooks/post-edit.sh` / `createPiAwarenessBridge().handleToolResult` | Releases this agent's lock on the written file as `PENDING` verification and inserts best-effort `edit_log` rows for extracted paths. | `octocode-awareness verify audit --agent-id <id> --workspace "$PWD" --compact` should list pending verification. |
| verify gate | `Stop` / `SubagentStop` | `Stop` / `SubagentStop` | `stop` / `subagentStop` | `agent_end` | `scripts/hooks/stop-verify.sh` / `wirePiAwarenessHooks(pi)` | Shell hooks hard-block conclusion once (exit 2); Pi cannot hard-block after `agent_end`, so it injects a follow-up reminder turn when PENDING tasks exist. | `octocode-awareness verify mark --agent-id <id> --workspace "$PWD" --all-pending --message "<check>" --compact`, then rerun `verify audit`. |
| session capture | `SessionEnd` | `PreCompact` best-effort | `sessionEnd` plus `preCompact` | `session_shutdown` / `session_before_compact` | `scripts/hooks/session-end.sh` / `wirePiAwarenessHooks(pi)` | Runs `session capture` to write a work-handoff refinement from this session's locks and dirty git tree. Codex has no current `SessionEnd`; Cursor cloud lacks `sessionEnd` but supports `preCompact`. | `octocode-awareness refinement get --workspace "$PWD" --limit 5 --compact`. |
| smart briefing | `UserPromptSubmit` | `UserPromptSubmit` | `sessionStart` | `before_agent_start` | `scripts/hooks/notify-deliver.sh` / `wirePiAwarenessHooks(pi)` | Runs `signal list --format hook`, touches the agent registry, and injects message/memory context where the host accepts it. Cursor native `beforeSubmitPrompt` cannot inject context, so native Cursor gets a session-start briefing. | `octocode-awareness signal list --agent-id <id> --workspace "$PWD" --all --limit 5 --compact`. |

Use this table as the hook audit story before installing, debugging, or copying the skill. It names the lifecycle event, wrapper script, side effect, and verification command. The wrapper scripts in `skills/octocode-awareness/scripts/hooks/` only invoke package-owned `hook-runner.mjs`.

Behavior details:
- **agent id** = `OCTOCODE_AGENT_ID` if set, else the hook payload's `session_id`/`sessionId`/`agent_id`; Pi falls back to `pi:<session-file-basename>` then `pi:<pid>`.
  Export `OCTOCODE_AGENT_ID` so hooks and manual lock calls share one identity.
  If a shell-hook host provides no identity, hooks derive a stable local fallback from host + workspace and warn; this keeps pre/post hooks paired but is not a replacement for an explicit per-agent id.
  Hook events register/touch that id in the shared `agents` table when the DB is available.
- **message routing** — hook-injected messages are a trigger to use Awareness signal commands. Hooks surface the inbox; `signal publish|list|reply|ack|resolve` owns protocol steps such as targeted send, reply, acknowledgement, resolution, and A2A-style mapping.
- **TTL** = 10 min — the safety net if `PostToolUse` never fires (e.g. the tool errored). When `PostToolUse` does fire, it releases the lock but keeps the task `PENDING` until `verify` records the test result.
- **Fail-open with warning** — shell wrappers warn and exit 0 if `hook-runner.mjs` is missing.
  `pre-edit.sh` blocks (exit 2) *only* on a genuine lock conflict; other infrastructure or input errors exit 0 with a warning so a hook bug never wedges real work.
- **Prompt digest opt-in** — `notify-deliver.sh` normally does a lightweight briefing. Set `OCTOCODE_NOTIFY_RUN_DIGEST=1` if you also want it to run the periodic maintenance digest before a prompt.
- **Path extraction** — the lock hooks and `harness-guard.sh` accept Claude-style `tool_input.file_path`, Cursor-style `file_path`, Pi-style `input.path`/`args.path`, and Codex-style `apply_patch` payloads. Non-file tool calls are a no-op.
- **Bounded waits** — hooks never sleep indefinitely. A wrapper that chooses to wait should call `lock wait` or `lock acquire --wait-seconds`; both return `2` with `conflicts[]` on timeout and sleep outside SQLite transactions.
- **Scoped verification** — `lock acquire` records `workspace_path` + `files_json`. `Stop` passes the prompt `cwd` to `verify audit` when available. `verify mark --workspace <root> --all-pending` avoids verifying unrelated pending work by the same agent in another repo.

All hooks use the **one shared canonical store** (`~/.octocode/memory/awareness.sqlite3` under the global Octocode home, relocatable via `OCTOCODE_MEMORY_HOME`).
File-lock hooks read/write `tasks` and `locks` there, so claims are visible across local processes.
Pending verification survives lock release.
Workspace-scoped hooks write to the same DB file, scoped by `repo`/`ref` and `workspace_path`. They do not write the workspace `.octocode/` projection; `repo inject` does that.

The CLI installer (`octocode-awareness hooks install|check|remove`, or bundled `node scripts/awareness.mjs`) manages all bundled Claude, Codex, or Cursor lifecycle hooks: pre/post edit, harness guard, verify gate, capture, and briefing.
For Codex and Cursor, skill frontmatter alone is not an installation. The init flow is: install the skill, run `maintenance init`, preview host hooks with `hooks install --host <host> --dry-run`, install after explicit approval, then run `hooks check --host <host> --strict`.
`scripts/install-hooks.mjs` remains as a compatibility wrapper for older docs and installs.
The same shell hooks are skill-scoped only in hosts that execute skill-frontmatter hooks.
Pi gets equivalent behavior from `wirePiAwarenessHooks(pi)`, already wired by `@octocodeai/pi-extension`.

## Hook events available (reference)

For shell-hook hosts, `PreToolUse` and `PermissionRequest` block on exit 2; `PostToolUse` runs after the tool and cannot block.
Codex currently supports `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `SessionStart`, `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, and `Stop`. Codex does not expose `SessionEnd`; use `PreCompact` for best-effort session capture.
Cursor native hooks use lower-camel event names: `preToolUse`, `postToolUse`, `subagentStart`, `subagentStop`, `beforeShellExecution`, `afterShellExecution`, `beforeMCPExecution`, `afterMCPExecution`, `beforeReadFile`, `afterFileEdit`, `beforeSubmitPrompt`, `afterAgentResponse`, `afterAgentThought`, `stop`, `sessionStart`, `sessionEnd`, `preCompact`, and `workspaceOpen`.
Cursor cloud agents run command-based project/team/enterprise hooks only. Current cloud support includes shell/read/file-edit/tool/subagent/preCompact events, but not user-level hooks, prompt hooks, `sessionStart`, `sessionEnd`, `stop`, Tab hooks, workspace hooks, MCP hooks, or agent-response/thought hooks.
For Pi, use extension events (`tool_call`, `tool_result`, `before_agent_start`, `agent_end`, `session_shutdown`, `session_before_compact`) rather than skill frontmatter.

Shell-hook wiring usually matches `Write|Edit|MultiEdit|NotebookEdit` and provides a file path in the tool payload.
Codex-style wiring should include `apply_patch`; bundled scripts parse patch text under `tool_input.command`.
Cursor-style wiring uses flat command entries in `.cursor/hooks.json`; use lower-camel event keys and absolute commands unless the hook script lives under the project root.
Keep `PreToolUse` strict and fast.
Keep `PostToolUse` best-effort.
Use `Stop` for "continue, verification still owed" gates instead of trying to undo completed edits.

## Make enforcement session-wide

Skill-scoped shell hooks only fire while the skill is active in hosts that execute Agent Skills frontmatter hooks.
For always-on multi-agent locking and verification in shell-hook hosts, merge the same hooks into project settings with the bundled installer.
For Pi, use `@octocodeai/pi-extension` or call `wirePiAwarenessHooks(pi)` from your Pi extension.

**GATE: writing project settings requires explicit user approval.** Preview first, then install only on confirmation.

| Need | Hosts | Command |
|---|---|---|
| Preview | `claude`, `codex`, `cursor` | `octocode-awareness hooks install --host <host> --dry-run --compact` |
| Install | `claude`, `codex`, `cursor` | `octocode-awareness hooks install --host <host> --compact` |
| Check | `claude`, `codex`, `cursor` | `octocode-awareness hooks check --host <host> --strict --compact` |
| Remove | `claude`, `codex`, `cursor` | `octocode-awareness hooks remove --host <host> --compact` |

- Add `--project-dir <path>` to target a specific project.
- The installer is idempotent, only adds/removes its own hook commands, and never touches other hooks.
- Claude resolves hook paths from the installer location; Codex and Cursor use absolute commands because they do not provide Claude's project/skill placeholders.

If this skill is repackaged as a Codex or Cursor plugin, ship native hook config as plugin-level `hooks/hooks.json`; do not rely on `SKILL.md` frontmatter.

## Tune or disable

- **Disable**: remove the `hooks:` block from `SKILL.md` frontmatter and any installed copy in `.claude/settings.json`, `.codex/hooks.json`, `.cursor/hooks.json`, or plugin `hooks/hooks.json`.
- **Narrow scope**: tighten the matcher, or add an `if` condition to the hook entry.
- **Longer/shorter claim window**: change the TTL in `packages/octocode-awareness/bin/hook-runner.ts`, then rebuild `@octocodeai/octocode-awareness` so `skills/octocode-awareness/scripts/` is regenerated.
- **Path placeholder**: frontmatter commands use `${CLAUDE_SKILL_DIR}`, Claude Code's substitution for "directory containing this skill's `SKILL.md`" (requires Claude Code v2.1.196+; only Claude honors it — Codex and Cursor have no equivalent frontmatter placeholder).
  Codex and Cursor hook config do not use this placeholder; install those hooks through `octocode-awareness hooks install --host codex`, `--host cursor`, or plugin hooks.
  Bundled shell wrappers also self-locate via `BASH_SOURCE`; direct installer calls still resolve correctly.
