# Spawn Gate

Load before spawning. Why: multi-agent overhead is only worth independent work, long isolation, or adversarial coverage.

Activate orchestration for an explicit substantial delegation request or a consequential task with at least two independently useful workstreams beyond batchable reads. Explicit agent wording never overrides the value/cost gate.

## Decision

| Situation | Do |
|---|---|
| Dependent steps, shared context, ordinary edits/synthesis | Stay in **parent** |
| Independent tool calls, known inputs | **Batch** in one turn |
| Skill/prompt pack already covers the job | Load skill in **parent** — do not spawn |
| Low-risk summarize/extract/classify/… on saved text; save tokens | **Local Ollama** — `references/local-ollama.md` |
| Named specialist role (research, plan, review, …) | Delegate **typed specialist** through host API |
| Purpose-built objective; custom tools + brief | Spawn **clean worker** with minimal tools |
| Independent remote peer | **A2A** — `a2a.md` |
| Specialist must own next user turns | **Handoff** packet (filtered history + return rule) |

IF parent, skill, or one batch finishes cheaply THEN do not spawn.
IF subtasks need each other's live context THEN keep serial in parent.
IF workers are independent THEN spawn all before waiting on any.
IF approval is pending, authority is missing, or every step consumes the prior evolving result THEN stop or stay serial in the parent.

## Anti-patterns
- Spawning for one file read/search.
- Parallel writers on the same path without ownership rules.
- Treating “phase done” / idle as acceptance — check packet criteria.
- Recursive workers unless the host documents nesting and you need it.
- Ceremonial fan-out for named cheap reads, tiny edits, or predetermined deterministic work.

Next: `decompose.md` · `patterns.md` · `synthesize.md`.
