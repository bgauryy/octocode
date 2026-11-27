Load when auditing a production agent, harness or agent feature.

# Audit checklist

Mark each check **pass**, **fail** or **not assessed**, with the evidence (trace, config, code location, run). Each check names what to inspect, then what fails. Report each fail as an [audit finding](answer-formats.md#audit-findings), which owns its fields and the proven-versus-candidate rule.

## Context and attention

- In one captured request, every block is read by that step; fail: a file, tool result or instruction the trace never uses ([attention](context-attention.md#attention-first)).
- In a trace, each read follows a search or names an exact symbol, range or field; fail: a whole file or an unfiltered listing read before any narrowing ([retrieval](retrieval.md#retrieval-techniques)).
- At the largest request, measured occupancy leaves the model's output limit plus tool-result room ([budget](context-budget.md#context-budget)); fail: a truncated output or a limit error in a trace.
- After the longest run or a compaction, the request still holds the objective, acceptance criteria and safety constraints, and two conflicting rules have a stated winner; fail: a missing constraint, or a collision no rule resolves ([context failures](context-attention.md#context-failures)).
- Each claim in the result names its source and revision; fail: a fact with no source, or an inference a summary turned into a fact ([grounding](grounding.md#grounding)).

## Caching and memory

- Byte-diff the prefix of two requests of one workflow; fail: a timestamp, a changing identifier or a reordered tool definition inside it ([prompt caching](caching.md#prompt-caching)).
- Each store is named a cache or a memory; fail: a cache used as a permission boundary or as long-term memory ([prompt caching](caching.md#prompt-caching)).
- Each cache entry carries a version and an expiry; fail: an entry served after the thing it is keyed on changed ([caching](caching.md#caching)).
- Read a cache or memory entry with a second user or tenant identity; fail: the first identity's entry comes back ([caching](caching.md#caching), [memory](memory.md#memory)).
- After a compaction, the agent names the next correct action and its supporting evidence; fail: a decision, constraint or pending item absent from the checkpoint ([checkpoint template](compaction.md#checkpoint-template)).

## Tools, MCP and skills

- For each close pair of tools in the served list, one sentence decides between them; fail: a pair with no deciding sentence, or a job stated in no layer ([set audit](tool-set-audit.md#set-audit)).
- Only skill names and descriptions load before a task matches; fail: a skill body or reference in a request whose task did not need it ([skill loading](tool-set-audit.md#skills-and-progressive-disclosure)).
- Send one call with an invalid argument and one the caller may not make; fail: either one executes ([schema](tool-descriptions.md#schema), [risks](security-risks.md#risks-and-mitigations)).
- Large collections are read through server-side filters, pages or ranges, and every `hasMore` has a reachable continuation; fail: an unbounded result, or a partial page reported as complete ([pagination](tool-results.md#pagination-contract)).
- Call each tool with an oversized result, an invalid argument, a cancel and a timeout; fail: any reply without a typed result and a next step ([errors](tool-errors.md#actionable-errors), [pagination](tool-results.md#pagination-contract), [cancel and timeout](tool-design.md#mcp-server-design)).

Continue with the [routing, correctness, security and long-running checks](audit-checklist-operations.md).
