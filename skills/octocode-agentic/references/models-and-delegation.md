Load when routing work across models, deciding whether to delegate to subagents, defining what a worker returns, writing a handoff, assigning task ownership, or sharing a frozen base prompt.

# Models and delegation

## Contents

- [Multi-model patterns](#multi-model-patterns)
- [Escalation policy](#escalation-policy)
- [Delegate only separable work](#delegate-only-separable-work)
- [Return contracts](#return-contracts)
- [Handoff protocols](#handoff-protocols)
- [Structured handoff](#structured-handoff)
- [Frozen base prompt](#frozen-base-prompt)
- [Task ownership](#task-ownership)

## Multi-model patterns

A multi-model system gives models different roles; both need not run on every request.

| Pattern | Architecture | Best use |
|---|---|---|
| Fast → strong | Small model for easy tasks, strong model for hard cases | Cost-efficient routing |
| Planner → executor | Reasoning model plans; execution model runs bounded steps | Complex workflows |
| Generator → verifier | One produces; another checks against evidence | High-consequence outputs |
| Retriever → synthesizer | Light agent gathers context; strong model reasons | Research and code analysis |
| Router → specialists | Router delegates to domain models | Heterogeneous workloads |
| Draft → refine | Fast draft, selective refinement | Writing and transformation |
| Parallel candidates → judge | Several approaches compared by an evaluator | Hard reasoning |
| Independent cross-check | Models analyze the same evidence separately | Error detection |
| Local → cloud | Local model for limited tasks, remote when needed | Privacy or cost |
| Escalation cascade | Raise capability only while uncertainty remains | Efficient inference |

## Escalation policy

| Trigger after fast-model classification, routing or extraction | Next action |
|---|---|
| Evidence sufficient; reasoning and consequences fit capability | Return the verified result. |
| Missing evidence, complex reasoning or high consequences | Escalate to a stronger model. |
| A risky assertion still needs proof | Selectively verify evidence or tests; no automatic extra model call. |

Use a stronger model when the task needs it, not for every request.

## Delegate only separable work

For most developer-agent systems, start with one supervisor and bounded specialist workers, not an open network of agents talking to each other.

- Do not delegate trivial subtasks or tightly coupled decisions.
- Do not let every agent read the same large context; pass precise task contracts and evidence references.
- Return concise findings, not transcripts.
- Run independent tasks in parallel, dependent ones in sequence.
- Bound recursive delegation by depth and cost; bound concurrency.
- Measure whether several agents beat one strong agent on the actual workload.

## Return contracts

A subagent returns a bounded result, not its process history.

Return outcome, deciding evidence, changed artifacts, checks and unresolved work as needed by the caller. Match any existing host result schema; do not invent an envelope for a simple handoff. A runtime-validated example is in [agent communication](agent-communication.md#typed-messages).

A successful worker return does not establish correctness: verify the artifact, test or state the next step depends on.

## Handoff protocols

Use only the host's authorized tools; check their live contracts.

- Focused subtask → typed local call; parent owns conversation and synthesis.
- Specialist assists → manager-as-tool with an explicit I/O contract; parent keeps control.
- Specialist takes over → handoff: receiver, transfer condition, filtered context, return rule.
- Independent remote agent → A2A: Agent Card, capabilities, task lifecycle, artifacts and auth.
- Model calls a service → MCP ([tool contract](tools-and-mcp.md)).
- Slow operation → host tasks or negotiated MCP Tasks: poll, update, cancel; durable handle; terminal result.

Each message kind uses one [catalog](protocol-catalog.md#catalog) protocol; the catalog owns the slots.

| Message | Protocol |
|---|---|
| Request, brief | Task contract with a Definition of Done (stop condition for open-ended work); budget contract when costly; context manifest for large background |
| Ambiguous or consequential request | Check-back when needed to settle objective, authority or completion |
| Transfer of unfinished work, stop report | Handoff packet |
| Blocker, escalation, deviation from plan | SBAR |
| Choice the receiver must make | Decision request |
| Status | Delta update |
| Result | BLUF with claim–evidence; deliverables as structured results or artifacts |
| Question, approval-needed, cancellation | Plain kind name and the one thing needed |

Outcome is done, partial, failed, refused or canceled: a terminal state, never evidence status. Needing input is not an outcome; keep the request open and ask a question or decision request. Map application outcomes and intermediate states explicitly to the chosen wire lifecycle; the [typed result](agent-communication.md#typed-messages) uses `blocked` for an interruption, not completion.

Envelope fields (protocol version, correlation ID, sender, receiver, deadline) belong only when they change a decision; see the [message envelope](agent-communication.md#message-envelope). Remote Agent Cards, messages, artifacts and links remain [untrusted](security.md#between-agents) until identity, advertised capability, schema and authorization checks pass; never forward credentials. Preserve the callee's terminal state, error code, retry guidance and stable handle.

## Structured handoff

Transfer enough to complete the delegated task without copying the conversation. Name who owns the user-facing result and who may write; pass large evidence by stable path or cursor.

State goal and scope, relevant inputs, constraints, acceptance and the expected return. Keep message or task handles only when the host needs them to route, resume or cancel the work.

## Frozen base prompt

- Workers sharing a base prompt get the released bytes, never a rewritten copy; changes need a new reviewed version. One-off delegation needs none of this.
- Retain the released prompt and settings that affect behavior. Use the deployment's existing version/artifact identity; add digests only when byte identity must be established, rather than inventing a release manifest for ordinary delegation.
- Per-task data belongs in a typed overlay after the base. Overlays specialize but never weaken higher-authority rules; keep secrets outside model-visible context.
- Before dispatch, verify that the configured loader supplies the intended released base and tool/output contracts, then append the overlay. A cache hit alone does not prove identity; scope checks to bytes and configuration the platform exposes.
- A base change needs review and relevant behavior/security checks. In-flight tasks retain their version unless the migration policy permits restart; do not silently mutate it for cache hits or context limits.

## Task ownership

```text
submitted → working → completed (deliver result)
                    → failed    (return error)
                    → canceled  (stop execution)
```

Simplified: A2A also defines interrupted states such as input-required and auth-required, and a rejected terminal state.

- One accountable owner per task at a time.
- The parent tracks child task IDs and cancellation.
- Children return independently verifiable artifacts.
- Terminal tasks do not silently restart.
- A timeout does not prove remote execution stopped; query task state before repeating side effects.
- A canceled task has not necessarily rolled back its side effects.
- A successful transport response does not prove the business operation succeeded.
