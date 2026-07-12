# Agent Cheat Sheet

Use `<cli>`: local `node packages/octocode-awareness/out/octocode-awareness.js`; installed
`npx @octocodeai/octocode-awareness`; bundled `node scripts/awareness.mjs` only as fallback.
Export `OCTOCODE_AGENT_ID`; use Claude frontmatter or checked host config, never both.

## BEFORE / READ

```bash
<cli> attend --workspace "$PWD" --query "<task>" --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Inspect Ready, Claimed, Verify, FilesUnderWork, Inbox. Follow `next` (Verify → Ready →
owned Claimed → FilesUnderWork → Inbox → evidence). Use `--help`,
`schema command <noun> [action]`, or docs only when the next action needs them.
If prior learning could alter the plan, run `memory recall --query "<task>" --workspace "$PWD" --smart --compact`; re-check ranked leads. `.octocode/` is a menu, not live state; refresh it only with `wiki sync`.

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
adds `--exclusive`; exit `2` = wait/signal/switch. `lock wait/prune` are advanced
recovery commands; re-check presence before retrying.

## Token Discipline
Compact `attend` for the next action. Prefer grouped `schema commands`, one exact
`schema command`, targeted `verify audit`/`signal list`/`work show`, and CSV/HTML for
bulk. Add `--full` only when the compact receipt cannot drive the next decision.

AFTER/VERIFY and LEARN/CLEAN: `references/agent-cheatsheet-finish.md`. Agents/skills:
`references/agent-cheatsheet-tooling.md`. Files: `references/files-awareness.md`.
