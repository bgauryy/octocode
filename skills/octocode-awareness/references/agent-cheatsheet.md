# Agent Cheat Sheet

Use `<cli>`: local monorepo `node packages/octocode-awareness/dist/index.js`;
installed package `npx @octocodeai/octocode-awareness` (or global `octocode-awareness`); bundled
`node scripts/awareness.mjs` only as fallback. Export `OCTOCODE_AGENT_ID`; use active
Claude frontmatter or checked Codex/Cursor config, never both Claude surfaces.

## BEFORE / READ

```bash
<cli> attend --workspace "$PWD" --query "<task>" --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Inspect Ready, Claimed, Verify, FilesUnderWork, Inbox. Follow `next` (Verify → Ready →
owned Claimed → FilesUnderWork → Inbox → evidence). Use `--help` / `schema json-schema`
/ `docs list` only when the next action needs them.

## DURING / DO — Shared Task

```bash
<cli> task claim --task-id <task> --agent-id "$OCTOCODE_AGENT_ID" --compact
# hooks declare paths; without hooks:
<cli> work start --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --file <path> --compact
# run the declared check while claim/presence remains active
<cli> task submit --task-id <task> --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "passed" --compact
```

## DURING / DO — Standalone WORK

```bash
<cli> work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file <path> --rationale "<why>" --test-plan "<check>" --compact
# run the declared check while presence remains active
<cli> work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
# then verify mark
```

Ordinary peers allowed; `work show --workspace "$PWD" --file <path>` when overlap matters. Sensitive work
adds `--exclusive`; exit `2` = wait/signal/switch. `lock wait` ≠ peer gone — re-check
presence before exclusive acquire.

## Token Discipline
Compact `attend` for the next action. Prefer `verify audit`, `signal list`, `work show`;
CSV/HTML for bulk. Recall and docs list are lean by default.

AFTER/VERIFY and LEARN/CLEAN: `references/agent-cheatsheet-finish.md`. Agents/skills:
`references/agent-cheatsheet-tooling.md`. Files: `references/files-awareness.md`.
