# Awareness Hooks

Load when installing, checking, or changing host lifecycle automation.

Hooks automate loop edges; they do not choose tasks or replace `attend`/verify. Export one stable `OCTOCODE_AGENT_ID`. A config entry proves presence, not execution, trust, or model-visible delivery.

| Host | Surface | Key constraint |
|---|---|---|
| Claude | skill frontmatter or settings | Choose one surface; do not also install duplicate hooks. |
| Codex | trusted `.codex/hooks.json` | No distinct failed-write event. |
| GitHub Copilot | `.github/hooks/octocode-awareness.json` | Project-only official v1 hook file. |
| Cursor | `.cursor/hooks.json` | Local/cloud lifecycle coverage differs. |
| Gemini CLI | `.gemini/settings.json` | Uses Gemini event names and timeout units. |
| OpenCode | `.opencode/plugins/octocode-awareness.js` | Project plugin translates events into the shared runner. |
| Pi | native extension events | Guard at `tool_call`; audit/release at `tool_execution_end`; never install shell hooks. |

Before installation, show the noncompact dry-run and obtain explicit approval. Then apply and strict-check the same host and scope:

```bash
<cli> hooks install --host <claude|codex|copilot|cursor|gemini|opencode> --project-dir . --dry-run
<cli> hooks install --host <claude|codex|copilot|cursor|gemini|opencode> --project-dir . --compact
<cli> hooks check --host <claude|codex|copilot|cursor|gemini|opencode> --project-dir . --strict
```

For drift, preview removal, remove, reinstall, and strict-check. Configuration answers are not installation approval.

## Write path

1. Extract deduplicated paths; no path is a no-op.
2. Evaluate the harness guard before DB presence.
3. Resolve one TASK claim, matching WORK, or scoped HOOK fallback.
4. Declare advisory work; real exclusivity blocks, ordinary overlap succeeds.
5. Emit only changed overlap pointers.
6. Log/heartbeat successful edits; failed writes create no success receipt.
7. Stop, compact, or session end finalizes scoped HOOK work and audits debt.

N edits in one scoped turn produce one pending HOOK with N files. TASK/WORK never merge into it. Correlation loss never marks success. Expiry removes stale coordination only.

Smoke: registration, one-time peer context, exclusive denial before presence, failed-write behavior, multi-file fallback verification, compaction reuse, session end, and host log visibility. Missing runtime observation is failure even when strict config checks pass.

Prompt delivery stays bounded: changed state emits a pointer; details require targeted reads. Harness apply also requires `OCTOCODE_ALLOW_HARNESS_APPLY=1` on a safe non-main branch.

Next: use `references/files-awareness.md` for edit decisions or return to `SKILL.md`.
