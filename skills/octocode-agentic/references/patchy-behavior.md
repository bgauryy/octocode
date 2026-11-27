Load when deciding what a script should compute instead of the model, or correcting patchy coding behavior.

# Scripts and patchy behavior

## Script first, agent second

If an operation can be a reliable deterministic program, use the program.

Use tested parsers, LSP/indexers, graph algorithms, schema validators, formatters, analyzers, test runners and profilers for exact work. The agent chooses the question, inputs, permissions and acceptance, then interprets the structured result and provenance. Write reusable automation only when an existing tool does not cover the need, and run it to validate behavior.

Scripts do not resolve ambiguous intent or prove a design appropriate. For destructive operations validate inputs, constrain permissions, inspect effects and obtain the applicable authorization.

## Patchy behavior

| Anti-pattern | Failure | Better |
|---|---|---|
| Patch-first coding | Fix before understanding the cause | Reproduce and trace behavior |
| Shotgun editing | Speculative edits in many files | Find the minimal causal change |
| Happy-path fix | Edge cases ignored | Verify invariants and failures |
| Context thrashing | Information reloaded repeatedly | Persist evidence and checkpoints |
| Tool thrashing | Switching tools without progress | Name the information the next call must return, then pick the tool for it |
| Over-compaction | Important task state lost | Preserve invariants and provenance ([checkpoint template](compaction.md#checkpoint-template)) |
| Over-delegation | Coordination overhead exceeds the work | [Delegate only separable work](delegation.md#delegate-only-separable-work) |
| Speculative refactoring | Scope expands | Keep changes bounded |
| Duplicate policy logic | Competing implementations | One policy owner |
| Fake completion | Success claimed from plausibility | Run the verification |
| Prompt accretion | Endless rules and exceptions | Merge or delete a rule before adding one ([write and repair](prompt-writing.md#write-and-repair)) |
| Architecture drift | Module boundaries violated | Architecture tests |

Repeats, stalls and unbounded searching are classified in the [loop types](execution-control.md#loop-types). For a fix, work through the [issue-hunting loop](mental-models.md#issue-hunting-loop).
