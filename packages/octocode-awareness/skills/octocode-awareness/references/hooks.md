# Awareness Hooks

Hooks automate the lobby lifecycle; manual CLI remains valid. Installed config only
matters when the host executes it.

| Behavior | Effect |
|---|---|
| Brief | Deliver changed signals/memory/refinement only. |
| Pre-edit | Guard harness first; declare advisory work; block exclusivity conflict. |
| Post-edit | Audit/heartbeat; end HOOK fallback PENDING. |
| Stop | Bound verification debt and block/remind. |
| Capture | Deduplicate handoff/session state. |

## Hosts

- Claude: skill frontmatter while active, or project `.claude/settings.json`.
- Codex: explicit `.codex/hooks.json`; PreCompact captures handoff.
- Cursor: explicit `.cursor/hooks.json`; cloud event support differs.
- Pi: `wirePiAwarenessHooks(pi)`; never shell install.

Preview, install after approval, then check:

```bash
<cli> hooks install --host <claude|codex|cursor> --project-dir . --dry-run --compact
<cli> hooks install --host <claude|codex|cursor> --project-dir . --compact
<cli> hooks check --host <claude|codex|cursor> --project-dir . --strict --compact
```

Remove (preview first) when uninstalling host wiring:

```bash
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --dry-run --compact
<cli> hooks remove --host <claude|codex|cursor> --project-dir . --compact
```

Installer repairs obsolete paths and removes the old separate harness-guard entry;
pre-edit is the single ordered guard+presence hook. Smoke ordinary peer overlap,
exclusive block, fallback verification, and stderr visibility.

Identity/TTL/payload detail: `references/hook-semantics.md`. Session timing and
handoff detail: `references/session-observability.md`. Harness edits require
`OCTOCODE_ALLOW_HARNESS_APPLY=1` plus a safe non-main branch.
