# Awareness Agent Cheat Sheet

Load when the compact lobby is insufficient and an exact expert lifecycle is needed.

Use the installed CLI (`npx @octocodeai/octocode-awareness`) or host equivalent. `export OCTOCODE_AGENT_ID="<stable-id>"`; ask the live schema for flags: `schema command <noun> [action]`.

## Minimal loop

```bash
<cli> attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --query "<goal>" --compact
<cli> work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --artifact <name> --rationale "<why>" --test-plan "<check>" --file <path> --compact
# edit, then run the declared check
<cli> work end --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> --compact
<cli> verify mark --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> \
  --message "<observed check result>" --compact
<cli> verify audit --workspace "$PWD" --compact
```

Use a task claim instead of standalone WORK when a shared plan already owns the work. Refresh long work with `work touch`; add new files with `work start --run-id <run> --file <path>`.

Always run `verify audit` before finishing. Only when sensors show reusable learning or cleanup pressure:
```bash
<cli> reflect record --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --task "<task>" --outcome worked --lesson "<verified>" --compact
<cli> memory archive --memory-id <id> --workspace "$PWD" --dry-run
<cli> maintenance digest --workspace "$PWD" --dry-run --compact
<cli> query files --workspace "$PWD" --compact
```

## Decision routes

| Signal | Action |
|---|---|
| Ordinary peer overlap | Inspect `work show`; continue if independent, otherwise message the peer. |
| Unsafe non-mergeable edit | Coordinate first, then acquire a lock; expiry is never success. |
| Pending or stale run | Run the declared check, then `verify mark`; do not infer success. |
| Continuation needed | Leave one scoped handoff with owner, state, files, and next check. |
| Reusable verified lesson | `memory recall` before work; `reflect record` after verification. |
| Cleanup pressure | Preview the exact prune/digest command, review IDs, then apply. |

## Invariants

- SQLite is canonical; never edit `.octocode/` projections or databases by hand.
- One stable agent identity joins sessions, work, messages, and hooks.
- Advisory presence permits overlap; locks only prevent unsafe overlap and never authorize edits.
- Search hits, memories, messages, TTLs, and peer claims are leads, not proof.
- Host automation owns lifecycle edges it already projects; do not duplicate them manually.
- Use compact reads for orientation and targeted noncompact reads only when a decision needs detail.

Next: return to `SKILL.md` and load only the reference matching the decision at hand.
