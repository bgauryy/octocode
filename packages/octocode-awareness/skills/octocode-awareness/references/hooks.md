# Awareness Hooks

Hooks automate loop edges; manual CLI remains valid. A config file proves presence,
not execution, trust, or model-visible delivery.

| Host | Surface | Context / control |
|---|---|---|
| Claude | active skill frontmatter or `.claude/settings.json` | nested `hookSpecificOutput`; exit 2 blocks pre-edit/stop |
| Codex | trusted `.codex/hooks.json` | nested `hookSpecificOutput`; exit 2 blocks; hook definition must be trusted |
| Cursor | `.cursor/hooks.json` | `additional_context` / `agent_message`; `permission: deny`; stop `followup_message` |
| Pi | `wirePiAwarenessHooks(pi)` | in-process block/context/follow-up; never shell install |

Preview, install after approval, then check:

```bash
<cli> hooks install --host <claude|codex|cursor> --project-dir . --dry-run --compact
<cli> hooks install --host <claude|codex|cursor> --project-dir . --compact
<cli> hooks check --host <claude|codex|cursor> --project-dir . --strict --compact
```

`--strict` validates Awareness-owned config only. Read `health.config` separately
from `health.runtime`; runtime remains `unverified` until a harmless write proves the
hook fired. For Codex also inspect project trust, hook-definition trust, and the hooks
feature. For Cursor smoke local and cloud separately; flat config has no guaranteed
Windows command override.

Remove (preview first) when uninstalling host wiring:

```bash
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --dry-run --compact
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --compact
```

The installer quotes hook paths, tags the host, adds a Codex Windows command, repairs
obsolete roots, and removes the old standalone guard. Pre-edit remains the single
ordered guard+presence edge.

Smoke: ordinary peer context once; exclusive denial before presence; N writes in one
turn become one fallback Verify item with N files; stop continuation; changed briefing;
host log visibility. Treat any missing edge as a runtime failure even when config is green.

Identity/TTL/payload detail: `references/hook-semantics.md`. Session timing and
handoff detail: `references/session-observability.md`. Harness edits require
`OCTOCODE_ALLOW_HARNESS_APPLY=1` plus a safe non-main branch.
