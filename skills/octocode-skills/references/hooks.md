# Hooks

Load when reviewing or explaining skill lifecycle hooks — before install or before adding one.

A hook observes, blocks, or modifies an agent action. Surface depends on host.

| Host | Preferred surface | Note |
|------|-------------------|------|
| Claude-style | `hooks:` in `SKILL.md` or `.claude/settings.json` | `${CLAUDE_SKILL_DIR}` only in skill frontmatter |
| Cursor | `.cursor/hooks.json` / plugin `hooks/hooks.json` | Native skills do **not** run `SKILL.md` hook frontmatter |
| Codex | `.codex/hooks.json` / plugin hooks | Standalone `SKILL.md` hooks are not a Codex source |
| Pi | Extension adapter | No skill-frontmatter shell hooks |

Confirm the target host executes the surface before relying on it.

## Events (common)

| Event | Can block? | Good for |
|-------|------------|----------|
| PreToolUse / preToolUse | yes (exit 2) | validation, locks, guards |
| PostToolUse / postToolUse | no | logging, release state |
| Stop / SubagentStop | host-dependent | "you still owe X" verify |
| SessionStart / SessionEnd | no | capture/restore |
| UserPromptSubmit | no | prompt validate / inject |
| PreCompact | no | snapshot before compaction |

## Script contract

Thin wrapper at `scripts/hooks/example-hook.sh` → exec brain under `scripts/`. Stdin JSON; exit 0 allow, 2 block (pre/stop only); fail open on bugs. Always set `timeout`. Fast pre-tool; best-effort post; stop = reminder not undo. Rename the example when adding a real hook.

## Review before install

Read every `scripts/hooks/*` and every `command:` in skill/host configs. Flag destructive, silent, or unbounded hooks. Production example: `octocode-awareness` skill hooks.

Next: when wiring a new hook load `references/hooks-add.md`; after editing frontmatter load `references/skill-review.md`.
