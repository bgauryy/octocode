# Agent Cheat Sheet

Canonical CLI: `node scripts/awareness.mjs` in an installed skill, or `node packages/octocode-awareness/dist/bin/awareness.js` in this monorepo. Use `npx @octocodeai/octocode-awareness` only when neither local path exists. Set `OCTOCODE_AGENT_ID` once per run.

## Start

```bash
<cli> attend --workspace "$PWD" --query "<current task>" --compact
<cli> schema commands --compact
```

Follow `next` from `attend` when present — it is meant to be copy-runnable.

## Before edits

```bash
<cli> workspace status --workspace "$PWD" --compact
<cli> memory recall --query "<task>" --workspace "$PWD" --smart --compact
<cli> refinement get --workspace "$PWD" --state open --compact
<cli> signal list --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
<cli> lock acquire --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --target-file <path> --rationale "<why>" --test-plan "<exact verify>" --compact
```

Exit code `2` on lock conflict → wait, coordinate via signal, switch files, or stop.

## After edits

```bash
<cli> verify audit --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
# run the declared test plan, then:
<cli> verify mark --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --all-pending --compact
<cli> lock release --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
```

## Finish / hygiene

```bash
<cli> reflect record --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --task "<task>" --outcome success|partial|fail --lesson "<reusable>" --compact
<cli> session capture --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
<cli> maintenance digest --workspace "$PWD" --dry-run --compact
<cli> repo inject --workspace "$PWD" --compact   # only when projections should refresh
```

## Hard ideas

```bash
<cli> attend --workspace "$PWD" --query "<idea or risk>" --compact
# then read references/self-reflection-dialogue.md and run a bounded two-role pass
<cli> reflect record ... --duo --compact   # advisory supporter/skeptic prompts only
```


## Code search (not bundled here)

```bash
npx octocode search <dir> --tree --max-depth 2 --no-color
npx octocode search "<term>" <path> --no-color
npx octocode search <file> --content-view exact --no-color
```

Use `npx octocode` so the platform-native engine resolves correctly. Details: `references/octocode.md`.

## Rules of thumb

- SQLite is canonical; `.octocode/` markdown is a projection/lead.
- Memories and signals are leads — verify against files/tests before acting.
- Prefer `signal` vocabulary in docs/CLI; internal `notification*` names are aliases.
- Do not invent unshipped commands (`sleep`, dedicated trust gate) — see `homeostatic-loop.md`.
