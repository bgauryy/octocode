Load when choosing how to structure an agent or harness: its components, loop shape or the pattern for a problem.

# Agent architecture

Default: one bounded agent loop, as in the [reference control loop](control-loop.md#reference-control-loop). Add an agent only for a named gap: work that is separable, parallel or needs an isolated context ([delegate only separable work](delegation.md#delegate-only-separable-work)).

## Choose a pattern

| Problem | Pattern | Why | Read |
|---|---|---|---|
| Simple, well-defined task | Direct model call | No orchestration overhead | — |
| Needs one external fact | Single targeted tool call | Minimal retrieval and latency | [Retrieval](retrieval.md) |
| Complex task with tools | Bounded agent loop | Observe and adapt to results | [Execution control](execution-control.md) |
| Long-running task | Checkpoint + compaction | Keep critical state across context limits | [Compaction](compaction.md#compaction) |
| Many tools available | Tool discovery + routing | Avoid loading irrelevant schemas | [Tool count](mcp-context.md#choose-a-small-coherent-tool-set) |
| Large repository | Hierarchical search + selective reads | Less context pollution | [Retrieval](retrieval.md#retrieval-techniques) |
| Large paginated API | Cursor pagination + evidence-based stopping | Skip unneeded pages | [Pagination](evidence-loop.md#pagination) |
| Stable repeated prompts | Prompt caching | Reuse the eligible prefix | [Prompt caching](caching.md#prompt-caching) |
| Repeated deterministic calculations | Computation cache | No duplicate processing | [Caching](caching.md#caching) |
| Persistent user or project knowledge | Validated external memory | Useful facts survive sessions | [Memory](memory.md#memory) |
| Mixed easy and hard workloads | Model cascade | Route by complexity and cost | [Escalation](model-routing.md#escalation-policy) |
| Independent complex subtasks | Parallel specialist subagents | Isolated context, faster execution | [Delegation](delegation.md#delegate-only-separable-work) |
| High-stakes output | Evidence + external verifier | Stronger correctness guarantees | [Grounding](grounding.md#grounding) |
| Frequent hallucinations | Claim grounding + abstention | No unsupported assertions | [Grounding](grounding.md#grounding) |
| Frequent unproductive tool calls | Progress-based stopping | Stop or change strategy | [Loop types](execution-control.md#loop-types) |
| Patchy coding changes | Reproduce → root cause → test | Coherent fixes | [Patchy behavior](patchy-behavior.md#patchy-behavior) |
| Too much output | Structured return contract | Only decision-relevant information | [Return contracts](delegation.md#return-contracts) |

## Design principles

Reuse verified results only while their complete input identity remains unchanged. Key reusable artifacts by content and recompute only changed inputs. Record enough execution context for deterministic replay. Prefer reversible, idempotent effects, with explicit recovery for effects that rollback cannot undo.
