# Agent Collaboration Patterns

Use this to reason about flows between agents, roles, ownership, and communication. Supervisor is one option; choose the collaboration shape that fits the work.

## Technique Map

| Technique | Shape | Use When | Watch For |
|---|---|---|---|
| Prompt roles | One agent, named internal roles | Separation is conceptual only | Fake multi-agent complexity |
| Router | `classify -> specialist/path` | Known categories map to tools/prompts/models | Overrouting ambiguous work |
| Agents as tools | Manager calls specialist and keeps control | Specialist returns artifact/result | Specialist leaking conversation control |
| Handoff | Agent transfers user-facing turn | Specialist should own next interaction | Full-history transfer, unclear return |
| Orchestrator-workers | Planner creates dynamic subtasks | Work is discovered at runtime | Weak task packets, duplicate work |
| Parallel specialists | Same/different facets run concurrently | Latency, coverage, independent evidence | Merge conflicts, inconsistent criteria |
| Reviewer/evaluator | Separate judge checks output | Quality/safety needs independent review | Self-approval |
| Debate/red-team | Advocate + critic + judge | High-stakes tradeoff or risk analysis | Performative disagreement |
| Blackboard | Agents read/write shared workspace | Coordination through artifacts/state | Concurrent writes, stale facts |
| Hierarchical team | Leads coordinate subteams | Large domain with nested ownership | Bottlenecks, hidden state |
| Human-gated team | Agent proposes, human approves | Trust, policy, cost, destructive action | Vague approval prompt |
| Background worker | Async consolidation/evals/memory | Work can happen after response | Hidden side effects |

Choose **agents-as-tools** when the manager should keep control and specialists return artifacts. Choose **handoff** when a specialist should own the next user-facing turn. Choose **blackboard/shared workspace** when agents coordinate through artifacts instead of direct conversation.

## Role Design

Define roles by responsibility, not titles:

- `owner`: one agent/node owns each state field and artifact.
- `planner`: decomposes work and assigns packets.
- `specialist`: performs bounded work with scoped tools/context.
- `reviewer`: checks output, evidence, risk, and schema.
- `memoryWriter`: approves durable memory writes.
- `actor`: performs side effects after gates.
- `observer`: traces, metrics, evals, cache/memory decisions.

Avoid roles that differ only by wording. If two roles share tools, context, and output type, they may be one role with a clearer prompt.

## Communication Protocol

Each agent boundary should use Zod-backed packets:

```text
AgentTask: goal, inputs, knownFacts, constraints, allowedTools,
expectedOutput, stopConditions

AgentResult: status, output, evidence, unknowns, actionNeeded,
memoryCandidates, traceId
```

Pass artifact references over full history. Pass decisions separately from evidence. Let workers make local choices inside bounds; return to coordinator when scope, tool permission, confidence, or side effects exceed bounds.

## Common Failure Fixes

| Failure | Fix |
|---|---|
| duplicate work | disjoint ownership and task ids |
| context bloat | packet + artifact refs + input filters |
| tool bleed | per-agent allowlists |
| infinite delegation | max turns, budget, terminal states |
| prose result | Zod-valid `AgentResult` |
| weak merge | evidence ids and conflict surfacing |
| memory pollution | one approved memory writer |
| self-approval | reviewer, eval, or human gate |

## Output Mini-Scheme

```markdown
Roles: <role -> responsibility/tools/context>
Collaboration: <router | agents-as-tools | handoff | orchestrator-workers | blackboard | reviewer | hybrid>
Packets: <AgentTask/AgentResult/Zod schemas>
State: <owner per field/artifact>
Flow: <agent -> agent/tool/artifact>
Gates: <approval/memory/side-effect stops>
Failure controls: <loop limits, conflict policy, retries>
```
