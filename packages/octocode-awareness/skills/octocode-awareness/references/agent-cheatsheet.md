# Agent Cheat Sheet

Use `<cli>` in this order: installed `node scripts/awareness.mjs`; monorepo
`node packages/octocode-awareness/dist/bin/awareness.js`; package fallback
`npx @octocodeai/octocode-awareness`. Export one `OCTOCODE_AGENT_ID`.

## Start

```bash
<cli> attend --workspace "$PWD" --query "<task>" --compact
```

Inspect Ready, Claimed, Verify, FilesUnderWork, and Inbox. Follow `next`; use
`<command> --help` only when flags are unknown, `schema json-schema <name>` only
when constructing a machine payload, and `docs list` only when the reference owner
is unknown.

## Shared Task

```bash
<cli> task claim --task-id <task> --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks declare paths; without hooks:
<cli> work start --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --file <path> --compact
<cli> task submit --task-id <task> --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
# run checks
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "passed" --compact
```

## Standalone WORK

```bash
<cli> work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file <path> --rationale "<why>" --test-plan "<check>" --compact
# add/heartbeat: work start --run-id <run> --file <path>; work touch --run-id <run>
<cli> work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
# run check, then verify mark
```

Ordinary peers are allowed. Use `work show --file <path>` when overlap matters.
Sensitive work adds `--exclusive`; exit `2` means wait/signal/switch, never bypass.

## Token Discipline
Use compact `attend` for the next action. `--compact` minifies all JSON but does not
make every generic query lean; workboard limits apply per lane. Prefer targeted
`verify audit`, `signal list`, or `work show`; use CSV/HTML for bulk data. Recall and
docs list are lean by default. Request bodies/full rows only when acting.

Finish/handoff: `agent-cheatsheet-finish.md`. Agents/skills/search:
`agent-cheatsheet-tooling.md`. File decisions: `files-awareness.md`.
