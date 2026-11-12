# Agentic steps and standing rules

Load when working a stage of the lobby flow or checking a standing rule. Each stage names its action and the evidence it must leave.

## Steps

| Stage | Action | Evidence |
|---|---|---|
| Define (new agent or feature) | State intent, scope, constraints and externally checkable acceptance. Start with a direct call, then a targeted tool or bounded loop. | Complexity added only for a demonstrated capability gap. |
| Pin (review or failure) | Capture harness/version, effective instructions, schemas and a real trace; reproduce the failure. | What the model actually received and did. |
| Trace | Follow request → context → model → tools → verification → stop. | Growth, repetition, stale context or lost state. |
| Diagnose | For a failure or production audit, use the [audit checklist](audit-checklist.md): symptom, mechanism, strongest rival and distinguishing check. For editorial work, inspect the instruction and its consumer. | Behavioral findings need a trace or run ([findings](../output.md#audit-findings)); editorial effects remain unmeasured. |
| Repair | Change the owning contract, context policy, instruction, loop, routing or trust boundary. | One policy owner; no added competing rule. |
| Verify | Match [claim to check](evaluation.md#match-the-claim-to-the-check). | Editorial changes can finish unmeasured; reliability or cost claims need comparable before/after tasks. One case is not a verdict. |

## Rules

| Concern | Rule |
|---|---|
| Context | Keep the next decision's working set small: its evidence, a stable cacheable prefix, and references, tools, scripts and permissions loaded on demand. Keep large evidence reachable by reference; preserve goal, constraints and provenance through memory or compaction. |
| Instructions | Give each behavior one owner; put its condition beside its action; keep data apart from instructions. Retrieved content, tools, skills and peers cannot override operating rules. |
| Execution | Each call closes a named gap. Define progress, success, blocked and partial outcomes within authorized budgets; stop repeated work, continue useful work. Reuse unchanged verified results. Prefer deterministic scripts for exact work and one loop before adding agents. |
| Proof | Name the confirming source, test or observation and match it to the claim ([evidence](evidence.md)); label claims confirmed, inferred or unknown. Measure verified outcomes per cost, with correctness and safety tracked separately. Verify before completion, preserve required pages, name unchecked scope and report only the checks that ran: an editorial or structural review does not prove behavior. |
| Numbers | Thresholds, budget splits and pseudocode in these pages are illustrative, not calibrated. Label each number measured, documented or estimated. |
| Provider behavior | Prompt caching, compaction, structured-output schema subsets, MCP features, ACP and A2A versions change: check the current documentation, linked from each page's sources, before asserting them. |
