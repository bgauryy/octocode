# Agent Cheat Sheet — Finish And Handoffs

Core loop: `references/agent-cheatsheet.md`. Run only the branch that has work.

## AFTER / VERIFY — Always

After the declared check and `task submit` or `work end`, record the result and
confirm this agent has no remaining debt:

```bash
<cli> verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --message "<check result>" --compact
<cli> verify audit --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --compact
```

## LEARN / CLEAN / PROJECT — Only when due

| Condition | Action |
|---|---|
| Verified outcome is reusable | `reflect record --outcome worked\|partial\|failed --lesson <lesson>`; route remaining work with `--fix-repo`, `--fix-harness`, or `--fix-instructions`. |
| Work remains for another run | Publish a handoff signal, update the owning refinement, or run `session capture`. |
| Workboard reports cleanup pressure | Run `maintenance digest --dry-run`; inspect IDs before prune/forget. |
| File references may be stale | Run `query files --format table --limit 50`; repair/supersede the owning rows. |
| File readers need refreshed context | Run `repo inject --workspace "$PWD" --mode local --compact`; never hand-edit generated wiki files. |
| A human needs bulk inspection | Run `query all --format html --out .octocode/awareness/index.html`. |
| Instructions caused a wrong turn | Run `reflect developer-review`; close the same feedback row after the instruction fix is verified. |

Wiki map after inject: `AGENTS.md` entry · `GOTCHAS.md` traps · `LEARN.md` lessons ·
`MEMORY.md` index · `BOOKMARKS.md` resources · `DEVELOPER_REVIEW.md` instruction
feedback. SQLite is canonical; `references/repo-context-management.md` owns root
pointer permissions and publication details.

## Hard ideas

For a risky judgment, run `attend --query <risk>`, then load
`references/self-reflection-dialogue.md`; use `references/subagent-rubber-duck.md`
only when independent inspection adds value. Agreement is not verification.

## Handoffs

`refinement get --state open` returns coding rows. Add `--include-handoffs` only
when resuming session handoffs. Close the same row after applying and verifying it.
