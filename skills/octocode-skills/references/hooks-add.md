# Add A Hook

Load when wiring a new lifecycle hook into a skill. Why: wrong host surface or missing timeout = silent no-op or hung harness.

## Claude frontmatter

```yaml
hooks:
  PreToolUse: [{ matcher: "Write|Edit", hooks: [{ type: command, command: "${CLAUDE_SKILL_DIR}/scripts/hooks/example-hook.sh", timeout: 20 }] }]
```

- `${CLAUDE_SKILL_DIR}` only (Claude Code v2.1.196+). No `$SKILL_DIR` / `${SKILL_DIR}` — those resolve to nothing.
- Installers writing `.claude/settings.json` / `.cursor/hooks.json` / `.codex/hooks.json` must use project-relative or absolute paths (no skill-dir var).
- Omit `matcher` for Stop, SessionEnd, UserPromptSubmit, SessionStart, PreCompact.

## Cursor native

```json
{ "version": 1, "hooks": { "preToolUse": [{ "command": ".cursor/hooks/guard.sh", "matcher": "Write", "timeout": 20 }] } }
```

Project hooks run from repo root. Cloud agents support a subset of events only.

## Steps

1. Pick event + matcher from `hooks.md`.
2. Copy `assets/hooks/example-hook.sh` → target `scripts/hooks/example-hook.sh` (rename if needed).
3. Copy `assets/hooks/example-hook-brain.mjs`; replace TODO; keep `--help` + stdin parse.
4. Claude: add frontmatter with `${CLAUDE_SKILL_DIR}/…` + `timeout`. Cursor/Codex: native config or installer with `--dry-run` first.
5. Document in `SKILL.md` body (host, event, what it does, how to verify) — review requires `hooks-handling`.
6. Optional always-on installer: merge into host config only after dry-run + user approval.
7. Run `scripts/skill-review.mjs` — enforces `hook-script-routing` + `hook-timeout`.

Templates: `assets/hooks/example-hook.sh` (wrapper), `example-hook-brain.mjs` (brain + exit contract).

Next: when checking host/event tables load `references/hooks.md`; after wiring load `references/skill-review.md`.
