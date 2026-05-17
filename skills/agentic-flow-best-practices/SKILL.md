---
name: agentic-flow-best-practices
description: Use when the user asks to design, review, implement, debug, or evaluate an agentic workflow or AI-agent harness: MCP tools/resources/prompts, multi-agent routing or handoffs, agent memory/context/cache, Zod/JSON-schema protocols, human gates, observability, evals, or production safety. Do not use for ordinary app code, generic prompt writing, or model comparison unless an agentic flow boundary is involved.
---

# Agentic Flow Best Practices

This skill is a compact thinking framework for agentic systems. It should help an agent choose the simplest reliable design, reason about the harness, and avoid wasting context.

## Reference Routing

Load references when they help the current decision:

- `references/resources.md`: research launchpad with official docs, GitHub/source repos, and search hints for MCP, agent frameworks, context, memory, caching, and long-context risks.
- `references/pr-review-agent-example.md`: compact build-ready workflow example.
- `references/agent-collaboration-patterns.md`: compact guide to flows between agents, roles, handoffs, agents-as-tools, routing, review, and shared workspaces.

When framework/protocol behavior matters, verify from official docs or source. Prefer project-local conventions first; use Octocode MCP tools when available. Carry evidence only when it changes the design.

## Operating Loop

Use this loop for substantial agentic-flow work:

1. Understand the user-visible outcome, non-goals, risk, and deployment surface.
2. Map what each model/agent sees and what each tool, memory, cache, or side effect can change.
3. Choose the smallest pattern that satisfies the outcome; name the rejected heavier option when useful.
4. Define protocols at boundaries: inputs, outputs, tools, handoffs, memory writes, gates, traces, and errors.
5. Design state, context, memory, cache, permissions, and ownership before adding agents.
6. Add gates, evals, observability, and rollout controls proportional to risk.
7. Output a compact design or review with assumptions, unknowns, and next verification steps.

Stop when the chosen pattern, contracts, gates, and verification plan are clear enough for the user's next decision. Ask one focused question only when a missing fact changes architecture, permissions, or side effects.

## Agentic Flow Brain

For any agentic flow, focus on the dimensions that affect the decision:

| Dimension | Question |
|---|---|
| Outcome | What user-visible job is done, and what is out of scope? |
| User experience | What should the user approve, see, interrupt, resume, or recover from? |
| Autonomy | Prompt, workflow, graph, tool-using agent, or multi-agent? |
| Runtime | Who owns the loop: code, protocol client, framework runtime, graph engine, or custom harness? |
| Tools/MCP | What tools/resources/prompts exist, who may call them, and what needs approval? |
| Identity | Which user, tenant, agent, service account, or policy grants each action? |
| Visible surface | What does each agent actually see: messages, instructions, MCP prompts/resources, tools, schemas, memory, retrieval, and examples? |
| Protocols | Which Zod schemas govern input, node output, tool result, handoff, memory, cache, gate, trace, and errors? |
| Context | What is static, retrieved, summarized, offloaded, isolated in subagents, or passed by artifact reference? |
| Memory | What is session state, checkpoint, agent-private, shared, user/project/org memory, artifact, or cache? |
| Data | What data can cross tenants, tools, models, logs, caches, memories, and artifacts? |
| Models | Which model class, reasoning effort, output budget, temperature, and fallback fit each node? |
| Safety | Where can the flow leak data, trust poisoned context, take irreversible action, or write bad memory? |
| Reliability | What retries, idempotency keys, timeouts, cancellation, and concurrency controls exist? |
| Observability | Can traces show route, schema version, model config, tool calls, cache decisions, errors, tokens, cost? |
| Evals | What golden, adversarial, schema, memory, and tool-failure tests prove behavior? |
| Rollout | How will versions, migrations, flags, canaries, and rollback work? |

Think first, then choose the smallest design that satisfies the outcome. The answer should feel like tradeoff reasoning, not a completed form.

## Pattern Choice

Start simple; add autonomy only when it solves a real constraint.

- Direct answer or augmented LLM: low-risk response, one model call, retrieval, tools, or memory.
- Deterministic pipeline or prompt chain: fixed parsing, transforms, validation, idempotency, and gated subtasks.
- Router/model router: known categories need distinct prompts, tools, or models.
- Parallel, map-reduce, or orchestrator-workers: independent facets, large inputs, speed, or runtime-discovered subtasks.
- Evaluator-optimizer or reviewer: clear criteria and independent quality/safety review help.
- Graph/state machine or autonomous loop: durable execution, checkpoints, resume, human gates, or `plan -> act -> observe -> update -> stop`.
- Agents as tools, handoff, supervisor, or blackboard: separation needs distinct context, tools, permissions, ownership, or shared artifacts.
- Background consolidation: memory, evals, or summaries can run after the user-facing response.


Framework fit is contextual, not a ranking: plain code for deterministic/small workflows; MCP for exposing tools/resources/prompts, not orchestration; SDKs or ADK-style runtimes when lifecycle, sessions, tools, tracing, and structured outputs are modeled; graph runtimes for durable state, persistence, human-in-loop, and long-running agents. Use names like OpenAI Agents SDK, Google ADK, LangChain, and LangGraph as examples to research, not required dependencies.

## Coverage Checks

Check these areas when they affect the decision:

- Product: user intent, interruption/resume, preview UX, explanation, and escalation path.
- Access: auth, tenant isolation, tool allowlists, service accounts, and audit trails.
- Data: PII/secrets handling, retention, redaction, training/logging policy, and cross-model sharing.
- Operations: deployment target, queueing, concurrency, cancellation, idempotency, rollback, and cost/latency budgets.
- Governance: human gates, policy checks, memory approval, provenance, and delete/export behavior.
- Evolution: prompt/schema/tool versioning, cache invalidation, migrations, eval drift, and compatibility.

## Nodes And State

Describe the flow as nodes, not transcript history. For important nodes, capture:

- `purpose`: why this node exists.
- `inputSchema` / `outputSchema`: Zod schemas or artifact contracts.
- `tools`: allowed tools and why.
- `state`: fields read/written and owner.
- `cache`: key, scope, freshness, invalidation, or `none`.
- `memory`: search/write/ignore plus consent and retention.
- `failure`: retry, ask, fallback, stop, or gate.
- `sideEffects`: files, APIs, messages, deploys, payments, memory writes.
- `runControl`: timeout, cancellation, resume, idempotency, and concurrency behavior.

Common nodes: `classify`, `retrieve`, `compute`, `reason`, `delegate`, `review`, `act`, `observe`.

## Zod Protocols

Agentic systems communicate through protocols. Agent-to-agent, node-to-node, agent-to-tool, cache, memory, gate, trace, and side-effect boundaries should have runtime-validated schemas. In TypeScript, use Zod as the source of truth; in other runtimes, use an equivalent schema library and keep JSON Schema interoperability.

Define the contracts that exist:

- `FlowInput`, `NodeInput`, `NodeOutput`
- `AgentTask`, `AgentResult`, `AgentHandoff`
- `ToolCall`, `ToolResult`, `ErrorEnvelope`
- `MemoryQuery`, `MemoryCandidate`, `MemoryWrite`
- `CacheEntry`, `HumanGate`, `TraceEvent`
- `PolicyDecision`, `RunControl`, `ArtifactRef`

Zod rules:

- Define Zod first; infer TypeScript types from schemas.
- `safeParse` runtime boundaries before trusting data.
- Generate JSON Schema from Zod/equivalent schemas for model/tool/MCP structured-output APIs.
- Version schemas with prompts, tools, cache, memory, and model changes.
- Use discriminated unions for routes, outcomes, and errors.
- Represent ambiguity explicitly: `unknown`, `not_found`, `not_allowed`, `redacted`.
- Keep schemas small and composable; avoid giant shared state blobs.
- Attach provenance to decision facts: source path/URL/tool id/trace id/timestamp.

Minimal packet shape: `AgentTask` should include `goal`, `inputs`, `knownFacts`, `constraints`, `allowedTools`, `expectedOutput`, and `stopConditions`. `AgentResult` should include `status`, `output`, `evidence`, `unknowns`, and `memoryCandidates`.

## Context Engineering

Attention is finite. Large context windows raise the ceiling but do not guarantee the model uses all tokens well; relevant facts buried in the middle can be missed. More context can lower signal-to-noise, duplicate facts, increase cost/latency, and create conflicting instructions.

Pass packets, not full history:

```text
goal, inputs, knownFacts, decisions, openQuestions, constraints,
artifacts, expectedOutput
```

Before expanding context:

1. Reuse validated cache.
2. Retrieve the smallest relevant chunk.
3. Summarize with sources and omissions.
4. Ask if the missing fact changes architecture.
5. Pass broader context only when narrower context fails.

Token efficiency rules:

- Dedupe, rank, chunk, summarize, and retrieve on demand.
- Keep one source of truth per fact; pass the freshest cited version plus references.
- Prefer artifact paths, URLs, cache keys, trace ids over copied content.
- Keep stable instructions/examples before dynamic input for prompt caching; do not rewrite cacheable prefixes casually.
- Strip secrets, stale facts, speculation, and irrelevant history.
- Use context isolation: subagents inspect heavy artifacts and return compact Zod-valid results.
- Reserve output budget for visible answer and hidden reasoning tokens.

## Agent-Visible Surface

Review what each agent/model call actually receives, not only what the architecture diagram intends:

- Messages: system, developer, user, framework wrappers, skill text, node prompt.
- MCP surface: server instructions, tool names/descriptions, input schemas, output schemas, prompts, resources, and error text.
- Context surface: retrieved chunks, memory snippets, cache summaries, examples, prior decisions, artifacts, and hidden runtime facts exposed to the LLM.
- Action surface: allowed tools, approval gates, side effects, retries, and fallback instructions.

Check for duplicated or conflicting instructions across prompts, skills, MCP prompts, tool descriptions, schemas, memory, and retrieved docs. Keep one source of truth; place stable invariant instructions in the harness or stable prefix, and pass changing facts through compact task packets.

## Memory Scopes

Separate memory, state, cache, and artifacts.

- Runtime and LLM-visible context: clients/auth/request ids stay outside the model unless needed; task packets and retrieved chunks stay small and fresh.
- Working state, session state, and checkpoints: keep run/thread/resume data separate from durable facts.
- Agent-private, shared-flow, and user/project/org memory: define owner, scope, consent, retention, and conflict behavior.
- External knowledge, artifacts, and cache: cite retrieval, version artifacts, and key caches by tenant/scope/freshness without secrets.


Memory rules:

- Prefer working/session state until a fact proves it should persist.
- Long-term memory needs scope, owner, source, createdAt, retention, delete behavior, confidence.
- Read shared memory as evidence unless it is a trusted policy source.
- Prefer one writer per memory scope; workers propose `MemoryCandidate`s.
- Use append-only or conflict-aware shared memory when multiple agents write.
- Test should-remember and should-not-remember cases.

## MCP, Tools, And Skills

MCP exposes capabilities to AI apps:

- `tools`: actions such as search, file/API/db calls, comments, deploys.
- `resources`: context data such as files, schemas, tickets, logs.
- `prompts`: reusable templates or interaction starters.

Design tools like APIs: narrow names, clear docs, Zod/JSON schemas, example usage, error shapes, auth/retention notes, tool allowlists, and approval gates for destructive/costly/external actions.

Use skills for reusable procedural knowledge. Keep `SKILL.md` concise; move optional depth to references/scripts.

## Multi-Agent Flow

Use multiple agents when separation helps: expertise, tools, permissions, context windows, latency, review, or user-facing ownership. If specialists do not need separate tools/context, they may be prompt sections. For collaboration techniques, read `references/agent-collaboration-patterns.md`.

Handoff packet:

```text
goal, inputs, knownFacts, constraints, allowedTools,
expectedOutput, stopConditions
```

Common issues:

| Issue | Fix |
|---|---|
| vague ownership | one owner per state field/artifact |
| full-history handoff | handoff packet + artifact refs |
| supervisor bottleneck | workers decide locally inside bounds |
| tool bleed | per-agent tool allowlist |
| infinite delegation | max turns, budgets, terminal states |
| prose return | Zod-valid `AgentResult` |
| self-approval | separate reviewer/test/human gate |
| memory conflict | one memory writer or approval gate |

## Prompting And Models

Node prompts should define role, node goal, inputs, tool permissions, decision criteria, output schema, failure behavior, and evidence rules. Leave thinking room for ambiguous architecture, memory scope, tool choice, or tradeoffs; use firm instructions at schema/tool/memory/cache/side-effect boundaries.

Model/config checks:

- Smaller model: classification, extraction, formatting.
- Stronger reasoning model: planning, coding, tradeoffs, high-risk decisions.
- Long-context model: retrieval-heavy tasks, still curated.
- `max_output_tokens`: includes visible output and, for reasoning models, hidden reasoning.
- `reasoning effort`: more deliberation for harder decisions, more cost/latency.
- `temperature`: higher for ideation, lower for structured consistency.
- Trace model, prompt/schema version, tool version, cache decision, token use, latency, cost.

## Cache, Gates, Verification

Cache small validated results keyed by normalized input, prompt/model/tool/data versions, scope, and freshness. Cache safety invariant: no secrets, raw private data without policy, tenant-crossing data, or hidden unkeyed state. Memory and cache solve different problems.

Prompt-cache safety:

- Provider prompt caching depends on stable message/prefix/tool-schema bytes; changing system/developer messages, tool descriptions, schemas, MCP prompts, or prefix order can invalidate reuse or make cache assumptions unsafe.
- Keep cacheable instructions and examples stable, then append volatile user input, retrieval, memory, and runtime facts later.
- Include prompt version, model/config, tool description/schema version, MCP server/tool version, data source version, and tenant/scope in app cache keys.
- Do not cache LLM outputs when hidden inputs, retrieved context, or tool results that shaped the answer are missing from the key.

Consider gates before destructive/costly/external side effects, durable memory writes, cross-tenant cache risk, new auth/billing/deploy ownership, conflicting evidence, or vague prompts that trigger action.

Verification by risk:

- Sketch: chosen architecture, rejected alternative, biggest risk.
- Implementation: Zod tests, cache hit/miss/stale, golden path, tool permissions, gates.
- Production: evals, adversarial cases, traces, memory privacy, idempotency, retry, cost/latency budgets.

## Output Scheme

Use this as a flexible schema, not a required template. Select fields that help the user understand and build the flow.

Include the useful fields: `Goal`, `Decision`, `Runtime`, `Roles`, `Protocols`, `Context`, `Memory`, `Tools/MCP`, `Lifecycle`, `Models`, `Flow`, `Run control`, `Rollout`, and `Verification`.

## Recovery

- If requirements are broad, first return a sketch with assumptions and one decisive question.
- If framework behavior is uncertain, mark it unknown and verify against official docs/source before locking architecture.
- If tool, auth, tenant, or memory policy is unclear, gate side effects and design for least privilege.
- If context is too large, switch to retrieval, map-reduce, or subagent summaries with cited artifact refs.
- If eval criteria are missing, propose golden, adversarial, and tool-failure cases before implementation.

## Anti-Patterns

- Multi-agent before the core loop is proven.
- Full history where a compact packet works.
- Session state, memory, cache, and artifacts mixed together.
- Agent communication through prose instead of Zod protocols.
- Broad MCP/tools without auth, retention, validation, and gates.
- More context instead of better retrieval/dedupe/summarization.
- Larger model instead of fixing context, tools, schemas, or evals.
- A `reason` node deciding and executing risky action.
