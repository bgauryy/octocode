# Hooks — automatic file-claim enforcement

Read this to understand, tune, or disable bundled hooks, or to make file-claim enforcement session-wide. Hooks turn the "MUST claim a file before editing it" rule from agent memory into harness enforcement.

## What ships with this skill

`SKILL.md` frontmatter defines skill-scoped hooks only for hosts that execute Agent Skills frontmatter hooks, such as Claude-style shell-hook hosts.
Codex does **not** execute standalone `SKILL.md` hook frontmatter.
Install Codex hooks through `.codex/hooks.json`, inline `[hooks]` in `.codex/config.toml`, or an enabled plugin's `hooks/hooks.json`.
Pi also does **not** execute this frontmatter; Pi uses the native adapter exported by `@octocodeai/octocode-awareness` and wired by `@octocodeai/pi-extension`.

| Behavior | Claude-style event | Codex config event | Pi native event | Script / adapter | Side effect | Verify/audit command |
|-----------------|--------------------|--------------------|-----------------|------------------|-------------|----------------------|
| pre-edit | `PreToolUse` on `Write\|Edit\|MultiEdit\|NotebookEdit\|apply_patch\|ApplyPatch` | same matcher in `.codex/hooks.json` | `tool_call` / `tool_execution_start` | `scripts/hooks/pre-edit.sh` / `createPiAwarenessBridge().handleToolCall` | Claims the target file via `pre-flight-intent`; blocks if another agent holds it. | `node scripts/awareness.mjs status --workspace "$PWD"` should show the lock or conflict. |
| harness self-fix gate | `PreToolUse` on the same matcher | same matcher in `.codex/hooks.json` | n/a | `scripts/hooks/harness-guard.sh` | Blocks skill self-edits unless a human opened `OCTOCODE_ALLOW_HARNESS_APPLY=1` AND the skill root's git branch is a dedicated branch (checked live; `main`/`master` are always blocked; detached HEAD or a non-repo additionally needs `OCTOCODE_HARNESS_BRANCH_OK=1`). | Use `octocode-reflection` staged approval guidance; verify with the requested checks after the approved edit. |
| post-edit | `PostToolUse` on the same matcher | same matcher in `.codex/hooks.json` | `tool_result` / `tool_execution_end` | `scripts/hooks/post-edit.sh` / `createPiAwarenessBridge().handleToolResult` | Releases this agent's lock on the written file as `PENDING` verification. | `node scripts/awareness.mjs audit-unverified --agent-id <id> --workspace "$PWD"` should list pending verification. |
| verify gate | `Stop` / `SubagentStop` | `Stop` / `SubagentStop` | `agent_end` | `scripts/hooks/stop-verify.sh` / `wirePiAwarenessHooks(pi)` | Shell hooks hard-block conclusion once (exit 2); Pi cannot hard-block after `agent_end`, so it injects a follow-up reminder turn when PENDING tasks exist. | `node scripts/awareness.mjs verify --agent-id <id> --workspace "$PWD" --all-pending --message "<check>"`, then rerun `audit-unverified`. |
| session capture | `SessionEnd` | `PreCompact` best-effort | `session_shutdown` / `session_before_compact` | `scripts/hooks/session-end.sh` / `wirePiAwarenessHooks(pi)` | Runs `session-capture` to write a work-handoff refinement from this session's locks and dirty git tree. Codex has no current `SessionEnd` event, so the installer uses `PreCompact`. | `node scripts/awareness.mjs refine-get --workspace "$PWD" --limit 5`. |
| smart briefing | `UserPromptSubmit` | `UserPromptSubmit` | `before_agent_start` | `scripts/hooks/notify-deliver.sh` / `wirePiAwarenessHooks(pi)` | Runs `notify-get --format hook`, touches the agent registry, and injects message/memory context. | `node scripts/awareness.mjs notify-get --agent-id <id> --workspace "$PWD" --all --limit 5`. |

Use this table as the hook audit story before installing, debugging, or copying the skill. It names the lifecycle event, wrapper script, side effect, and verification command. The wrapper scripts in `skills/octocode-awareness/scripts/hooks/` only invoke package-owned `hook-runner.mjs`.

Behavior details:
- **agent id** = `OCTOCODE_AGENT_ID` if set, else the hook payload's `session_id`/`sessionId`/`agent_id`; Pi falls back to `pi:<session-file-basename>` then `pi:<pid>`.
  Export `OCTOCODE_AGENT_ID` so hooks and manual lock calls share one identity.
  Hook events register/touch that id in the shared `agents` table when the DB is available.
- **message routing** — hook-injected messages are a trigger to load `octocode-agent-communication`. Hooks surface the inbox; that skill owns protocol steps such as targeted send, reply, ack, resolve, and A2A mapping.
- **TTL** = 10 min — the safety net if `PostToolUse` never fires (e.g. the tool errored). When `PostToolUse` does fire, it releases the lock but keeps the task `PENDING` until `verify` records the test result.
- **Fail-open** — `pre-edit.sh` blocks (exit 2) *only* on a genuine lock conflict; any other error (DB issue, bad input) exits 0 with a warning so a hook bug never wedges real work.
- **Path extraction** — the lock hooks and `harness-guard.sh` accept Claude-style `tool_input.file_path`, Pi-style `input.path`/`args.path`, and Codex-style `apply_patch` payloads. Non-file tool calls are a no-op.
- **Bounded waits** — hooks never sleep indefinitely. A wrapper that chooses to wait should call `wait-for-lock` or `pre-flight-intent --wait-seconds`; both return `2` with `conflicts[]` on timeout and sleep outside SQLite transactions.
- **Scoped verification** — `pre-flight-intent` records `workspace_path` + `files_json`. `Stop` passes the prompt `cwd` to `audit-unverified` when available. `verify --workspace <root> --all-pending` avoids verifying unrelated pending work by the same agent in another repo.

All hooks use the **one shared store** (`~/.octocode/memory/awareness.sqlite3`, relocatable via `OCTOCODE_MEMORY_HOME`).
File-lock hooks read/write `tasks` and `locks` there, so claims are visible across local processes.
Pending verification survives lock release.
Workspace-scoped hooks write to the same file, scoped by `repo`/`ref` and `workspace_path`.

The installer (`scripts/install-hooks.mjs`, "make enforcement session-wide" below) manages all bundled Claude or Codex lifecycle hooks: pre/post edit, harness guard, verify gate, capture, and briefing.
The same shell hooks are skill-scoped only in hosts that execute skill-frontmatter hooks.
Pi gets equivalent behavior from `wirePiAwarenessHooks(pi)`, already wired by `@octocodeai/pi-extension`.

## Hook events available (reference)

For shell-hook hosts, `PreToolUse` and `PermissionRequest` block on exit 2; `PostToolUse` runs after the tool and cannot block.
Codex currently supports `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `SessionStart`, `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, and `Stop`. Codex does not expose `SessionEnd`; use `PreCompact` for best-effort session capture.
For Pi, use extension events (`tool_call`, `tool_result`, `before_agent_start`, `agent_end`, `session_shutdown`, `session_before_compact`/`session_compact`) rather than skill frontmatter.

Shell-hook wiring usually matches `Write|Edit|MultiEdit|NotebookEdit` and provides a file path in the tool payload.
Codex-style wiring should include `apply_patch`; bundled scripts parse patch text under `tool_input.command`.
Keep `PreToolUse` strict and fast.
Keep `PostToolUse` best-effort.
Use `Stop` for "continue, verification still owed" gates instead of trying to undo completed edits.

## Make enforcement session-wide

Skill-scoped shell hooks only fire while the skill is active in hosts that execute Agent Skills frontmatter hooks.
For always-on multi-agent locking and verification in shell-hook hosts, merge the same hooks into project settings with the bundled installer.
For Pi, use `@octocodeai/pi-extension` or call `wirePiAwarenessHooks(pi)` from your Pi extension.

**GATE: writing project settings requires explicit user approval.** Preview first, then install only on confirmation:

```bash
node <skill_root>/scripts/install-hooks.mjs --host claude --dry-run   # preview .claude/settings.json
node <skill_root>/scripts/install-hooks.mjs --host codex --dry-run    # preview .codex/hooks.json
node <skill_root>/scripts/install-hooks.mjs --host codex              # merge Codex awareness hooks
node <skill_root>/scripts/install-hooks.mjs --host codex --check      # report install status
node <skill_root>/scripts/install-hooks.mjs --host codex --remove     # uninstall our Codex hooks
```

The installer is idempotent and non-destructive.
The installer only adds/removes its own hook commands and never touches other hooks.
Use `--project-dir <path>` to target a specific project.

The installer writes project settings without skill context, so it cannot use `${CLAUDE_SKILL_DIR}`.
For Claude, the installer resolves hook paths from its own location: `${CLAUDE_PROJECT_DIR}`-relative inside a project, or absolute for user-scope installs.
For Codex, the installer writes absolute commands because Codex hook config does not provide Claude's project/skill placeholders.
Project settings differ from portable `SKILL.md` frontmatter, which uses `${CLAUDE_SKILL_DIR}`.

If this skill is repackaged as a Codex plugin, ship Codex hook config as plugin-level `hooks/hooks.json`; do not rely on `SKILL.md` frontmatter.

## Tune or disable

- **Disable**: remove the `hooks:` block from `SKILL.md` frontmatter and any installed copy in `.claude/settings.json`, `.codex/hooks.json`, or plugin `hooks/hooks.json`.
- **Narrow scope**: tighten the matcher, or add an `if` condition to the hook entry.
- **Longer/shorter claim window**: change the TTL in `packages/octocode-awareness/bin/hook-runner.ts`, then rebuild `@octocodeai/octocode-awareness` so `skills/octocode-awareness/scripts/` is regenerated.
- **Path placeholder**: frontmatter commands use `${CLAUDE_SKILL_DIR}`, Claude Code's substitution for "directory containing this skill's `SKILL.md`" (requires Claude Code v2.1.196+; only Claude honors it — Codex has no equivalent frontmatter placeholder).
  Codex hook config does not use this placeholder; install Codex hooks through `scripts/install-hooks.mjs --host codex` or plugin hooks.
  Bundled shell wrappers also self-locate via `BASH_SOURCE`; direct installer calls still resolve correctly.
