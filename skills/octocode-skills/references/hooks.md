# Hooks

Use when a skill needs a lifecycle hook - validating a tool call, gating a stop, capturing session state - or before installing a skill that already bundles one.

## Host Surfaces

A hook is a host lifecycle script that observes, blocks, or modifies an agent action. It may be skill-scoped frontmatter, project/user config, or plugin config depending on the host.

| Host | Preferred hook surface | Notes |
|---|---|---|
| Claude-style shell-hook hosts | `hooks:` in `SKILL.md` or `.claude/settings.json` | Use `${CLAUDE_SKILL_DIR}` only inside skill/agent frontmatter. |
| Cursor | `.cursor/hooks.json`, `~/.cursor/hooks.json`, or plugin `hooks/hooks.json` | Cursor can load Claude Code hook config when third-party configs are enabled, but native Cursor skills do not run `SKILL.md` hook frontmatter. |
| Codex | `.codex/hooks.json`, inline `[hooks]`, or plugin `hooks/hooks.json` | Standalone `SKILL.md` hook frontmatter is not a Codex hook source. |
| Pi | Native extension adapter | Pi does not execute skill-frontmatter shell hooks. |

Confirm the target host actually executes the hook surface before relying on one.

## Claude Frontmatter

```yaml
hooks:
  <EventName>: [{ matcher: "ToolA|ToolB", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/<name>.sh", timeout: 20 }] }]
```

- `${CLAUDE_SKILL_DIR}` is Claude Code's real substitution for the skill's own directory (requires Claude Code v2.1.196+), valid only inside `SKILL.md`/agent frontmatter.
- There is no bare `$SKILL_DIR` or `${SKILL_DIR}` variable. Claude Code does not recognize either, so a command using them silently resolves to a nonexistent path.
- A separate installer that writes `.claude/settings.json`, `.cursor/hooks.json`, or `.codex/hooks.json` has no skill context and must use a project-relative or absolute path instead.
- Omit `matcher` for events with no tool target (`Stop`, `SessionEnd`, `UserPromptSubmit`, `SessionStart`, `PreCompact`).
- Multiple independent hook entries can share one event.

## Cursor Native Config

Cursor hook files use lower-camel event names and flat command entries:

```json
{
  "version": 1,
  "hooks": {
    "preToolUse": [{ "command": ".cursor/hooks/guard.sh", "matcher": "Write", "timeout": 20 }],
    "stop": [{ "command": ".cursor/hooks/verify.sh", "loop_limit": 5 }]
  }
}
```

Project hooks run from the project root. Use `.cursor/...` for scripts committed with the repo, or an absolute path for scripts living inside an installed skill.

Cursor cloud agents run command-based project/team/enterprise hooks for shell/read/file-edit/tool/subagent/preCompact events. User hooks, prompt hooks, `sessionStart`, `sessionEnd`, `stop`, Tab hooks, workspace hooks, MCP hooks, and agent-response/thought hooks are not available in cloud agents.

## Events

| Event | Fires | Can block? | Good for |
|---|---|---|---|
| `PreToolUse` / `preToolUse` | before a matched tool runs | yes - exit `2` | validation gates, locks, guards |
| `PostToolUse` / `postToolUse` | after a matched tool ran | no | logging, releasing state, follow-up |
| `Stop` / `stop`, `SubagentStop` / `subagentStop` | agent is about to conclude | yes, host-dependent follow-up | "you still owe X" verification |
| `SessionStart` / `sessionStart`, `SessionEnd` / `sessionEnd` | session boundaries | no | capture/restore session state |
| `UserPromptSubmit` / `beforeSubmitPrompt` | before the agent sees a new prompt | no | validate prompts; context injection is host-specific |
| `PreCompact` / `preCompact` | before context compaction | no | snapshot state that compaction would lose |

Cursor also has event-specific hooks such as `beforeShellExecution`, `afterFileEdit`, `afterAgentResponse`, `beforeTabFileRead`, and `workspaceOpen`.

## Script Contract

- Ship a thin `scripts/hooks/<name>.sh` wrapper that self-locates (`BASH_SOURCE`) and `exec`s a real Node/Python "brain" script under `scripts/`. Never inline logic in frontmatter/config or duplicate it across wrappers.
- Read the payload from stdin as JSON; never prompt interactively.
- Exit `0` to allow, `2` to block (`PreToolUse`/`preToolUse` and stop-style gates only - other events cannot block); any other nonzero code is an error, not a decision.
- Fail open: a bug in the hook (bad input, missing dependency) should exit `0` with a warning, not block real work. Reserve `2` for the condition you are actually enforcing.
- Set a `timeout` next to every `command:`; lifecycle hooks must never hang the harness.
- Keep pre-tool hooks fast and strict, post-tool hooks best-effort, and use stop hooks for reminders rather than trying to undo a completed edit.

## Add A Hook

1. Pick the event and matcher from the table above.
2. Copy `assets/hooks/example-hook.sh` into the target skill's `scripts/hooks/`, rename it - it already self-locates and forwards to a companion brain script.
3. Copy `assets/hooks/example-hook-brain.mjs` next to the skill's other scripts (or point the wrapper at an existing one), then replace the `TODO` with the real check; keep `--help` and explicit stdin/argv parsing.
4. For Claude-style skill-scoped hooks, add the `hooks:` block to `SKILL.md` frontmatter, pointing at `${CLAUDE_SKILL_DIR}/scripts/hooks/<name>.sh` with a `timeout`.
5. For Cursor or Codex, add native hook config (`.cursor/hooks.json`, `.codex/hooks.json`, or plugin `hooks/hooks.json`) or an installer script that writes it after a dry-run preview.
6. Document the hook in `SKILL.md`'s body: which host/event, what it does, how to inspect or verify it - the lint's `hooks-handling` rule requires this.
7. If the skill should also act when it is not the active skill, add a small installer script that merges the same command into the target host's project/user hook config.
   Gate every write behind `--dry-run` first and explicit user approval.
8. Run `scripts/skill-lint.mjs`; it enforces `hook-script-routing` (frontmatter commands route to `scripts/`/`hooks/`, not inline shell) and `hook-timeout` (every hook command has a nearby `timeout`).

## Templates Shipped Here

- `assets/hooks/example-hook.sh` - thin wrapper template: self-locates, reads stdin JSON, execs the brain script, forwards its exit code.
- `assets/hooks/example-hook-brain.mjs` - brain-script template: parses a subcommand and stdin JSON, shows the allow/block exit-code contract, ships `--help`.

## Reviewing Hooks

Before installing a skill that bundles hooks, read every `scripts/hooks/*` file and every `command:` target from `SKILL.md`, `.claude/settings*.json`, `.cursor/hooks.json`, `.codex/hooks.json`, or plugin `hooks/hooks.json`; confirm what each command runs and whether it touches files or the network.

Flag anything destructive, silent, or unbounded before writing it into a user or project scope.

For a production example - wired lifecycle events, a shared Node dispatcher, and a multi-host installer for Claude, Codex, and Cursor - inspect the `octocode-awareness` skill's own `references/hooks.md`.
