# Hooks

Use when a skill needs a lifecycle hook — validating a tool call, gating a stop, capturing session state — or before installing a skill that already bundles one.

## What a skill hook is

A `hooks:` block in `SKILL.md` frontmatter, executed by hosts that run Agent Skill hooks (Claude Code, Codex).

Support is not universal. Confirm the target host actually executes skill-frontmatter hooks before relying on one — some hosts (e.g. Pi) need a native extension adapter instead.

## Frontmatter shape

```yaml
hooks:
  <EventName>: [{ matcher: "ToolA|ToolB", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/<name>.sh", timeout: 20 }] }]
```

- `${CLAUDE_SKILL_DIR}` is Claude Code's real substitution for the skill's own directory (requires Claude Code v2.1.196+), valid only inside `SKILL.md`/agent frontmatter.
- There is no bare `$SKILL_DIR` or `${SKILL_DIR}` variable. Claude Code does not recognize either, so a command using them silently resolves to a nonexistent path.
- A separate installer that writes project/user `.claude/settings.json` has no skill context and must use a project-relative or absolute path instead.
- Omit `matcher` for events with no tool target (`Stop`, `SessionEnd`, `UserPromptSubmit`, `SessionStart`, `PreCompact`).
- Multiple independent hook entries can share one event.

## Events

| Event | Fires | Can block? | Good for |
|---|---|---|---|
| `PreToolUse` | before a matched tool runs | yes — exit `2` | validation gates, locks, guards |
| `PostToolUse` | after a matched tool ran | no | logging, releasing state, follow-up |
| `Stop` / `SubagentStop` | agent is about to conclude | yes (forces a continue turn) | "you still owe X" verification |
| `SessionStart` / `SessionEnd` | session boundaries | no | capture/restore session state |
| `UserPromptSubmit` | before the agent sees a new prompt | no | inject context or a briefing |
| `PreCompact` | before context compaction | no | snapshot state that compaction would lose |

## Script contract

- Ship a thin `scripts/hooks/<name>.sh` wrapper that self-locates (`BASH_SOURCE`) and `exec`s a real Node/Python "brain" script under `scripts/`. Never inline logic in frontmatter or duplicate it across wrappers.
- Read the payload from stdin as JSON; never prompt interactively.
- Exit `0` to allow, `2` to block (`PreToolUse`/`Stop` only — other events cannot block); any other nonzero code is an error, not a decision.
- Fail open: a bug in the hook (bad input, missing dependency) should exit `0` with a warning, not block real work. Reserve `2` for the condition you are actually enforcing.
- Set a `timeout` (seconds) next to every `command:` — lifecycle hooks must never hang the harness.
- Keep `PreToolUse` fast and strict, `PostToolUse` best-effort, and use `Stop` for a reminder rather than trying to undo a completed edit.

## Add a hook to a skill

1. Pick the event and matcher from the table above.
2. Copy `assets/hooks/example-hook.sh` into the target skill's `scripts/hooks/`, rename it — it already self-locates and forwards to a companion brain script.
3. Copy `assets/hooks/example-hook-brain.mjs` next to the skill's other scripts (or point the wrapper at an existing one), then replace the `TODO` with the real check; keep `--help` and explicit stdin/argv parsing.
4. Add the `hooks:` block to the skill's `SKILL.md` frontmatter, pointing at `${CLAUDE_SKILL_DIR}/scripts/hooks/<name>.sh` with a `timeout`.
5. Document the hook in `SKILL.md`'s body: which event, what it does, how to inspect or verify it — the lint's `hooks-handling` rule requires this.
6. If the skill should also act when it is not the active skill, add a small installer script that merges the same command into `.claude/settings.json`; gate every write behind `--dry-run` first and explicit user approval.
7. Run `scripts/skill-lint.mjs`; it enforces `hook-script-routing` (frontmatter commands route to `scripts/`/`hooks/`, not inline shell) and `hook-timeout` (every hook command has a nearby `timeout`).

## Templates shipped here

- `assets/hooks/example-hook.sh` — thin wrapper template: self-locates, reads stdin JSON, execs the brain script, forwards its exit code.
- `assets/hooks/example-hook-brain.mjs` — brain-script template: parses a subcommand and stdin JSON, shows the allow/block exit-code contract, ships `--help`.

## Reviewing someone else's hooks

Before installing a skill that bundles hooks, read every `scripts/hooks/*` file (or each frontmatter `command:` target) and confirm what it runs and whether it touches files or the network.

Flag anything destructive, silent, or unbounded before writing it into a user or project scope.

For a production example — six wired lifecycle events, a shared Node dispatcher, and a `.claude/settings.json` installer — inspect the `octocode-awareness` skill's own `references/hooks.md`.
