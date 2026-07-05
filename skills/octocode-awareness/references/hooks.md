# Hooks — automatic file-claim enforcement

Read this to understand, tune, or disable the bundled hooks, or to make file-claim enforcement session-wide. Hooks turn the "MUST claim a file before editing it" rule from a thing the agent must remember into a thing the harness enforces.

## What ships with this skill

`SKILL.md` frontmatter defines several skill-scoped hooks for hosts that execute Agent Skills hooks (Claude/Codex). Pi does **not** execute this frontmatter; Pi uses the native adapter exported by `@octocodeai/octocode-memory` and wired by `@octocodeai/pi-extension`.

| Behavior | Claude/Codex lifecycle event | Pi native event | Script / adapter | Side effect | Verify/audit command |
|-----------------|------------------------------|-----------------|------------------|-------------|----------------------|
| pre-edit | `PreToolUse` on `Write\|Edit\|MultiEdit\|NotebookEdit` | `tool_call` | `scripts/hooks/pre-edit.sh` / `createPiAwarenessBridge().handleToolCall` | Claims the target file via `pre-flight-intent`; blocks if another agent holds it. | `node scripts/awareness.mjs status --workspace "$PWD"` should show the lock or conflict. |
| harness self-fix gate | `PreToolUse` on the same matcher | n/a | `scripts/hooks/harness-guard.sh` | Blocks skill self-edits unless a human opened `OCTOCODE_ALLOW_HARNESS_APPLY=1` and the repo is on a dedicated branch. | Try `OCTOCODE_ALLOW_HARNESS_APPLY=1 node scripts/awareness.mjs harness-apply --help`; see `self-harness.md` before real edits. |
| post-edit | `PostToolUse` on the same matcher | `tool_result` | `scripts/hooks/post-edit.sh` / `createPiAwarenessBridge().handleToolResult` | Releases this agent's lock on the written file as `PENDING` verification. | `node scripts/awareness.mjs audit-unverified --agent-id <id> --workspace "$PWD"` should list pending verification. |
| verify gate | `Stop` / `SubagentStop` | `agent_end` | `scripts/hooks/stop-verify.sh` / `wirePiAwarenessHooks(pi)` | Shell hooks block conclusion once (exit 2); Pi injects a follow-up reminder turn when PENDING intents exist. | `node scripts/awareness.mjs verify --agent-id <id> --workspace "$PWD" --all-pending --message "<check>"`, then rerun `audit-unverified`. |
| session capture | `SessionEnd` | `session_shutdown` | `scripts/hooks/session-end.sh` / `wirePiAwarenessHooks(pi)` | Runs `session-capture` to write a work-handoff refinement from this session's locks and dirty git tree. | `node scripts/awareness.mjs refine-get --workspace "$PWD" --limit 5`. |
| smart briefing | `UserPromptSubmit` | `before_agent_start` | `scripts/hooks/notify-deliver.sh` / `wirePiAwarenessHooks(pi)` | Runs `notify-get --format hook` and injects memory context. | `node scripts/awareness.mjs notify-get --agent-id <id> --workspace "$PWD" --all --limit 5`. |

Use this table as the hook audit story before installing, debugging, or copying the skill. It names the lifecycle event, thin wrapper script, durable side effect, and the command that proves the hook did what it said. The wrapper scripts in `skills/octocode-awareness/scripts/hooks/` contain no lifecycle logic; they only invoke the package-owned `hook-runner.mjs`, built from `packages/octocode-memory/bin/hook-runner.ts`.

Behavior details:
- **agent id** = `OCTOCODE_AGENT_ID` if set, else the hook payload's `session_id`/`sessionId`/`agent_id`; Pi falls back to `pi:<session-file-basename>` then `pi:<pid>`. Export `OCTOCODE_AGENT_ID` to give the hooks and your manual `pre-flight-intent`/`release-file-lock` calls one shared identity, so the two mechanisms never treat you as two agents.
- **TTL** = 15 min — the safety net if `PostToolUse` never fires (e.g. the tool errored). When `PostToolUse` does fire, it releases the lock but keeps the intent `PENDING` until `verify` records the test result.
- **Fail-open** — `pre-edit.sh` blocks (exit 2) *only* on a genuine lock conflict; any other error (DB issue, bad input) exits 0 with a warning so a hook bug never wedges real work.
- **Path extraction** — the lock hooks and `harness-guard.sh` accept Claude-style `tool_input.file_path`, Pi-style `input.path`/`args.path`, and Codex-style `apply_patch` command payloads (`*** Update/Add/Delete File:` and `*** Move to:` lines). Non-file tool calls are a no-op.
- **Bounded waits** — hooks never sleep indefinitely. A wrapper that chooses to wait should call `wait-for-lock` or `pre-flight-intent --wait-seconds`; both return `2` with `conflicts[]` on timeout and sleep outside SQLite transactions.
- **Scoped verification** — `pre-flight-intent` records `workspace_path` + `files_json`; `Stop` passes the prompt `cwd` to `audit-unverified` when available, and `verify --workspace <root> --all-pending` avoids verifying unrelated pending work by the same agent in another repo.

All hooks use the **one shared store** (`~/.octocode/memory/awareness.sqlite3`, relocatable via `OCTOCODE_MEMORY_HOME`). The file-lock hooks (`pre-edit.sh`/`post-edit.sh`/`stop-verify.sh`) read/write locks + intents there, so claims are visible across every process on the machine and pending verification survives lock release. The workspace-scoped hooks (`session-end.sh` → refinement, `notify-deliver.sh` → notifications) write to the same file, scoped by `repo`/`ref` and `workspace_path` columns, so concurrent agents that resolve to the same working tree share one channel.

The installer (`scripts/install-hooks.mjs`, "make enforcement session-wide" below) manages **only** the two file-lock shell hooks. The `Stop`/`SessionEnd`/`UserPromptSubmit` shell hooks are skill-scoped only in hosts that execute skill hooks. Pi gets the equivalent behavior from `wirePiAwarenessHooks(pi)`, already wired by `@octocodeai/pi-extension`.

## Hook events available (reference)

For Claude/Codex shell hooks, `PreToolUse` and `PermissionRequest` block on exit 2; `PostToolUse` runs after the tool and cannot block. Other useful events: `SessionStart`, `UserPromptSubmit`, `Stop`/`SubagentStop`, `PreCompact`. For Pi, use extension events (`tool_call`, `tool_result`, `before_agent_start`, `agent_end`, `session_shutdown`, `session_before_compact`/`session_compact`) rather than skill frontmatter.

Claude Code wiring usually matches `Write|Edit|MultiEdit|NotebookEdit` and provides `tool_input.file_path`. Codex wiring should include `apply_patch` (the matcher aliases `Edit`/`Write` may also match file edits) and its hook payload exposes the patch text under `tool_input.command`, which the bundled scripts now parse. In both hosts, keep `PreToolUse` strict and fast, keep `PostToolUse` as best-effort release/context only, and use `Stop` for "continue, verification still owed" gates rather than trying to undo completed edits.

## Make enforcement session-wide

Skill-scoped shell hooks only fire while the skill is active in hosts that execute Agent Skills hooks. For always-on multi-agent locking in Claude/Codex, merge the same two hooks into the project's `.claude/settings.json` (shareable, committed) with the bundled installer. For Pi, use `@octocodeai/pi-extension` or call `wirePiAwarenessHooks(pi)` from your Pi extension.

**GATE: writing project settings requires explicit user approval.** Preview first, then install only on confirmation:

```bash
node <skill_root>/scripts/install-hooks.mjs --dry-run   # show the resulting settings.json
node <skill_root>/scripts/install-hooks.mjs             # merge our two hooks
node <skill_root>/scripts/install-hooks.mjs --check     # report install status
node <skill_root>/scripts/install-hooks.mjs --remove    # uninstall our hooks
```

The installer is idempotent and non-destructive: it only adds/removes its own `pre-edit.sh`/`post-edit.sh` entries and never touches other hooks. Use `--project-dir <path>` to target a specific project (default: current directory).

The installer writes `.claude/settings.json`, which has no skill context, so it can't use `${CLAUDE_SKILL_DIR}`. It instead resolves the hook path from its own location: a **shareable `${CLAUDE_PROJECT_DIR}`-relative** path when the skill lives inside the project (commit it), or an **absolute** path when it lives elsewhere (e.g. a user-scope `~/.claude/skills/` install). This differs from the `SKILL.md` frontmatter, which uses `${CLAUDE_SKILL_DIR}` and is portable on its own (see below).

If this skill is repackaged as a plugin, ship the same config as `hooks/hooks.json`. `${CLAUDE_SKILL_DIR}` still resolves (to the skill's subdirectory inside the plugin), so the frontmatter commands need no change; only the settings.json install path would.

## Tune or disable

- **Disable**: remove the `hooks:` block from `SKILL.md` frontmatter (and any copy in `.claude/settings.json`).
- **Narrow scope**: tighten the matcher, or add an `if` condition to the hook entry.
- **Longer/shorter claim window**: change the TTL in `packages/octocode-memory/bin/hook-runner.ts`, then rebuild `@octocodeai/octocode-memory` so `skills/octocode-awareness/scripts/` is regenerated.
- **Path placeholder**: the frontmatter commands use `${CLAUDE_SKILL_DIR}` — Claude Code's official placeholder for the skill's own install directory. It is rendered to an absolute path before the hook runs, so the frontmatter hooks work no matter where the skill is installed (personal `~/.claude/skills/`, project `.claude/skills/`, or plugin). The bundled `pre-edit.sh`/`post-edit.sh` also self-locate via `BASH_SOURCE`, so they still resolve correctly when invoked directly (e.g. by the installer). Only `.claude/settings.json` (no skill context) needs the installer's `${CLAUDE_PROJECT_DIR}`/absolute path instead.
