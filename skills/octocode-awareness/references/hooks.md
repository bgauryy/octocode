# Hooks — automatic file-claim enforcement

Read this to understand, tune, or disable the bundled hooks, or to make file-claim enforcement session-wide. Hooks turn the "MUST claim a file before editing it" rule from a thing the agent must remember into a thing the harness enforces.

## What ships with this skill

`SKILL.md` frontmatter defines several skill-scoped hooks (active only while this skill is loaded, auto-removed after):

| Event | Matcher | Script | Effect |
|-------|---------|--------|--------|
| `PreToolUse` | `Write\|Edit\|MultiEdit\|NotebookEdit` | `scripts/hooks/pre-edit.sh` | Claims the target file via `pre-flight-intent`. **Blocks the edit (exit 2)** if another agent holds it. |
| `PreToolUse` | same | `scripts/hooks/harness-guard.sh` | **Harness self-fix gate.** For edits to files inside the skill's own directory, **blocks (exit 2)** unless a human opened the gate (`OCTOCODE_ALLOW_HARNESS_APPLY=1`) and the skill repo is on a dedicated branch (not `main`/`master`; override `OCTOCODE_HARNESS_BRANCH_OK=1`). No-op for any file outside the skill. See `harness-apply` in `self-harness.md`. |
| `PostToolUse` | same | `scripts/hooks/post-edit.sh` | Releases this agent's lock on the file just written as `PENDING` verification. |
| `Stop` / `SubagentStop` | — | `scripts/hooks/stop-verify.sh` | Runs `audit-unverified`; **blocks the conclusion once (exit 2)** if an active or pending intent declared a test-plan but recorded no verification. Loop-guarded (`stop_hook_active`); opt out with `OCTOCODE_NO_VERIFY_GATE=1`. |
| `SessionEnd` | — | `scripts/hooks/session-end.sh` | Runs `session-capture` to auto-write a work-handoff refinement from this session's locks + dirty git tree. Non-blocking, fail-open; no-ops on a clean tree; opt out with `OCTOCODE_NO_SESSION_CAPTURE=1`. |
| `UserPromptSubmit` | — | `scripts/hooks/notify-deliver.sh` | Runs `notify-get --format hook` for this agent against the prompt's `cwd`, injecting unread repo messages (addressed to me or broadcast) into context via `additionalContext`, then advances the read cursor. Non-blocking, fail-open; emits nothing when the inbox is clear; opt out with `OCTOCODE_NO_NOTIFY=1`. |

Behavior details:
- **agent id** = `OCTOCODE_AGENT_ID` if set, else the hook's `session_id`, so concurrent Claude sessions are distinct agents and never block themselves (same-agent re-edits pass). Export `OCTOCODE_AGENT_ID` to give the hooks and your manual `pre-flight-intent`/`release-file-lock` calls one shared identity, so the two mechanisms never treat you as two agents.
- **TTL** = 15 min — the safety net if `PostToolUse` never fires (e.g. the tool errored). When `PostToolUse` does fire, it releases the lock but keeps the intent `PENDING` until `verify` records the test result.
- **Fail-open** — `pre-edit.sh` blocks (exit 2) *only* on a genuine lock conflict; any other error (DB issue, bad input) exits 0 with a warning so a hook bug never wedges real work.
- **Path extraction** — the lock hooks and `harness-guard.sh` accept both Claude-style `tool_input.file_path` and Codex-style `apply_patch` command payloads (`*** Update/Add/Delete File:` and `*** Move to:` lines). Non-file tool calls are a no-op.
- **Bounded waits** — hooks never sleep indefinitely. A wrapper that chooses to wait should call `wait-for-lock` or `pre-flight-intent --wait-seconds`; both return `2` with `conflicts[]` on timeout and sleep outside SQLite transactions.
- **Scoped verification** — `pre-flight-intent` records `workspace_path` + `files_json`; `Stop` passes the prompt `cwd` to `audit-unverified` when available, and `verify --workspace <root> --all-pending` avoids verifying unrelated pending work by the same agent in another repo.

All hooks use the **one shared store** (`~/.octocode/memory/awareness.sqlite3`, relocatable via `OCTOCODE_MEMORY_HOME`). The file-lock hooks (`pre-edit.sh`/`post-edit.sh`/`stop-verify.sh`) read/write locks + intents there, so claims are visible across every process on the machine and pending verification survives lock release. The workspace-scoped hooks (`session-end.sh` → refinement, `notify-deliver.sh` → notifications) write to the same file, scoped by `repo`/`ref` and `workspace_path` columns, so concurrent agents that resolve to the same working tree share one channel.

The installer (`scripts/install-hooks.mjs`, "make enforcement session-wide" below) manages **only** the two file-lock hooks. The `Stop`/`SessionEnd`/`UserPromptSubmit` hooks are skill-scoped only — they run while the skill is loaded and need no settings.json install.

## Hook events available (reference)

`PreToolUse` and `PermissionRequest` block on exit 2; `PostToolUse` runs after the tool and cannot block. Other useful events: `SessionStart`, `UserPromptSubmit`, `Stop`/`SubagentStop`, `PreCompact`. All events are valid in skill frontmatter; the structure mirrors `settings.json` hooks.

Claude Code wiring usually matches `Write|Edit|MultiEdit|NotebookEdit` and provides `tool_input.file_path`. Codex wiring should include `apply_patch` (the matcher aliases `Edit`/`Write` may also match file edits) and its hook payload exposes the patch text under `tool_input.command`, which the bundled scripts now parse. In both hosts, keep `PreToolUse` strict and fast, keep `PostToolUse` as best-effort release/context only, and use `Stop` for "continue, verification still owed" gates rather than trying to undo completed edits.

## Make enforcement session-wide

Skill-scoped hooks only fire while the skill is active. For always-on multi-agent locking, merge the same two hooks into the project's `.claude/settings.json` (shareable, committed) with the bundled installer.

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
- **Longer/shorter claim window**: change `--ttl-minutes` in `scripts/hooks/pre-edit.sh`.
- **Path placeholder**: the frontmatter commands use `${CLAUDE_SKILL_DIR}` — Claude Code's official placeholder for the skill's own install directory. It is rendered to an absolute path before the hook runs, so the frontmatter hooks work no matter where the skill is installed (personal `~/.claude/skills/`, project `.claude/skills/`, or plugin). The bundled `pre-edit.sh`/`post-edit.sh` also self-locate via `BASH_SOURCE`, so they still resolve correctly when invoked directly (e.g. by the installer). Only `.claude/settings.json` (no skill context) needs the installer's `${CLAUDE_PROJECT_DIR}`/absolute path instead.
