# Agent Cheat Sheet — Finish And Handoffs

Core loop: `references/agent-cheatsheet.md`. Run only the branch that has work.

## AFTER / VERIFY — Always

Run the declared check while presence/locks remain active. Then `task submit` or
`work end`, immediately record the result, and confirm this agent has no debt:

```bash
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "<check result>" --compact
<cli> verify audit --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --compact
```

## LEARN / CLEAN / PROJECT — Only when due

| Condition | Action |
|---|---|
| Verified outcome is reusable | `reflect record --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --task "<task>" --outcome worked\|partial\|failed --lesson "<lesson>"`; route remaining work with `--fix-repo`, `--fix-harness`, or `--fix-instructions`. |
| Work remains for another run | Publish a handoff signal, update the owning refinement, or run `session capture`. |
| Workboard reports cleanup pressure | Prefer `memory archive --memory-id <id> --workspace "$PWD" --dry-run --compact`; run `maintenance digest --workspace "$PWD" --dry-run --compact` and inspect before irreversible prune/forget. |
| File references may be stale | Run `query files --workspace "$PWD" --format table --limit 50`; repair/supersede the owning rows. |
| File readers need refreshed context | Run `wiki sync --workspace "$PWD" --mode local --compact`; review `orphan_candidates`, then add `--prune-orphans` to remove retired manifest-owned files. Never hand-edit generated wiki files. |
| A human needs bulk inspection | Run `query all --workspace "$PWD" --format html --out .octocode/awareness/index.html`. |
| Instructions caused a wrong turn | Run `reflect developer-review --workspace "$PWD"`; close the same feedback row after the instruction fix is verified. |

Wiki sync publishes a lean `AGENTS.md`, optional nonempty bounded `KNOWLEDGE.md`, and
`awareness/manifest.json`. SQLite is canonical; `references/repo-context-management.md`
owns root pointer permissions and publication details.

## Hard ideas

For a risky judgment, run `attend --query <risk>`, then load
`references/self-reflection-dialogue.md`; use `references/subagent-rubber-duck.md`
only when independent inspection adds value. Agreement is not verification.

## Handoffs

`refinement get --state open` returns coding rows. Add `--include-handoffs` only
when resuming session handoffs. Close the same row after applying and verifying it.
