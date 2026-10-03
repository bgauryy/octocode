# Hooks

Load when you review or explain skill lifecycle hooks (before install), or wire a new hook into a skill. Why: a wrong host surface or a missing timeout gives a silent no-op or a hung harness.

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

The target skill's scripts/hooks/NAME.sh wrapper executes its internal brain under scripts/. Stdin JSON; exit 0 allow, 2 block (pre/stop only); fail open on bugs. Always set `timeout`. Fast pre-tool; best-effort post; stop = reminder not undo.

## Review before install

Read every hook script and every `command:` in skill/host configs. Flag destructive, silent, or unbounded hooks.

## Add a hook

Claude frontmatter:

```yaml
hooks:
  PreToolUse: [{ matcher: "Write|Edit", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/example-hook.sh", timeout: 20 }] }]
```

- `${CLAUDE_SKILL_DIR}` only (Claude Code v2.1.196+). No `$SKILL_DIR` / `${SKILL_DIR}` — those resolve to nothing.
- Installers writing `.claude/settings.json` / `.cursor/hooks.json` / `.codex/hooks.json` must use project-relative or absolute paths (no skill-dir var).
- Omit `matcher` for Stop, SessionEnd, UserPromptSubmit, SessionStart, PreCompact.

Cursor native (project hooks run from repository root; cloud agents support a subset of events only):

```json
{ "version": 1, "hooks": { "preToolUse": [{ "command": ".cursor/hooks/guard.sh", "matcher": "Write", "timeout": 20 }] } }
```

Steps:

1. Pick event + matcher from the tables above.
2. Copy `assets/hooks/example-hook.sh` (wrapper) into the target skill as scripts/hooks/NAME.sh (rename it).
3. Copy `assets/hooks/example-hook-brain.mjs` (brain + exit contract); replace TODO; keep `--help` + stdin parse.
4. Claude: add frontmatter with `${CLAUDE_SKILL_DIR}/…` + `timeout`. Cursor/Codex: native config or installer with `--dry-run` first.
5. Document in `SKILL.md` body (host, event, what it does, how to verify) — review requires `hooks-handling`.
6. Optional always-on installer: merge into host config only after dry-run + user approval.
7. Run `scripts/skill-review.mjs` — enforces `hook-script-routing` + `hook-timeout`.

Next: after wiring or editing frontmatter, load `references/skill-review.md`.
