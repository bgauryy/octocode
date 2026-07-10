# Spawn Gate

Load before spawning. Why: multi-agent overhead is only worth independent work, long isolation, or adversarial coverage.

## Decision

| Situation | Do |
|---|---|
| Dependent steps, shared context, ordinary edits/synthesis | Stay in **parent** |
| Independent tool calls, known inputs | **Batch** in one turn |
| Skill/prompt pack already covers the job | Load skill in **parent** — do not spawn |
| Named Octocode specialist | `spawnSubagent` |
| Purpose-built objective; custom tools + `systemPrompt` | `spawnAgent` (lean; **no `skills` param**) |
| Browser / CDP | `browser.md` (chrome-gated) |
| Independent remote peer | **A2A** — `a2a.md` |
| Specialist should own next user turns | **Handoff** packet (filtered history + return rule) |

IF parent, skill, or one batch finishes cheaply THEN do not spawn.
IF subtasks need each other's live context THEN keep serial in parent.
IF workers are independent THEN spawn all before waiting on any.

## Anti-patterns
- Spawning for one file read/search.
- Passing `skills` to `spawnAgent` (public API has none — use `spawnSubagent` or bake into `systemPrompt`).
- Parallel writers on the same path without Awareness exclusivity.
- Treating `[DONE]` / idle as acceptance.
- Recursive workers (Pi strips spawn tools inside workers).

Next: `decompose.md` · `patterns.md` · `synthesize.md`.
