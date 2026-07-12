# Awareness Hooks
Hooks automate loop edges; manual CLI remains valid. A config file proves presence, not
execution, trust, or model-visible delivery. Export one stable `OCTOCODE_AGENT_ID` shared
by CLI and hooks — without it, presence and peer packets do not join correctly.
| Host | Surface | Context / control |
|---|---|---|
| Claude | active skill frontmatter or `.claude/settings.json` | success/failure writes, subagent start/stop, PreCompact, SessionEnd; nested output; exit 2 blocks |
| Codex | trusted `.codex/hooks.json` | SessionStart, successful writes, subagent start/stop, PreCompact, prompt/stop; no SessionEnd/failure event |
| Cursor | `.cursor/hooks.json` | success/failure writes and lifecycle; subagent context delivery varies by surface/version; native deny/follow-up output |
| Pi | `wirePiAwarenessHooks(pi)` | in-process block/context/follow-up; never shell install |
Choose one surface. With Claude frontmatter, preview/remove legacy project or global
Awareness hooks; do not also install them. Codex/Cursor require project config.
Preview, install after approval, then check:

```bash
<cli> hooks install --host <codex|cursor> --project-dir . --dry-run
<cli> hooks install --host <codex|cursor> --project-dir . --compact
<cli> hooks check --host <codex|cursor> --project-dir . --strict
```

Noncompact dry-run exposes the settings diff; compact output is only a receipt.
`--strict` validates exact entries and their runner; Claude frontmatter is a separate definition surface.
Read definition/config separately from runtime. Bounded SQLite upserts report
`unverified|observed|stale|failed`, `coverage`, and `last_seen` without payloads.
Codex: inspect project trust, definition trust, and feature enablement. Cursor: smoke local/cloud; flat config lacks a guaranteed
Windows command override. Use `--host claude` only when frontmatter is unavailable.
Remove (preview first) when uninstalling host wiring:

```bash
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --dry-run
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --compact
```

The installer quotes paths, adds a Codex Windows command, and removes Awareness hooks
at obsolete roots/events. For drift: preview remove, remove, install, strict-check. Pre-edit remains the single
ordered guard+presence edge.

Smoke: session/subagent registration; ordinary peer context once; exclusive denial before presence; a failed write creates no audit/debt; N successful writes in one
turn become one fallback Verify item with N files; PreCompact continuation reuses the
session; SessionEnd ends it; changed briefing and host log visibility. Treat any missing
edge as a runtime failure even when config is green.

Prompt-time delivery is transient: shell hooks pass an event prompt when available; Pi buffers
only the latest `input` through `before_agent_start`, clearing empty/consumed input. The hook emits
at most one grounded memory lead (or silence), keeps signals/overrides independent, and caps
the final five-item packet at 1 KiB UTF-8. Selection/trust: `references/memory-recall.md`.

Identity/TTL/payload: `references/hook-semantics.md`; session/handoff: `references/session-observability.md`.
Harness edits require `OCTOCODE_ALLOW_HARNESS_APPLY=1` plus a safe non-main branch.
