---
name: agentic-flow-best-practices
description: Use when designing, reviewing, or implementing agentic flows, multi-agent orchestration, LLM caching, memory/state passing, schema contracts, tool routing, Google ADK agents, LangChain/LangGraph agents, or reusable agent workflows. Produces a flexible flow design with Zod contracts, cache/memory policy, framework mapping, gates, and verification steps.
---

# Agentic Flow Best Practices

Design agent systems as stateful workflows with explicit contracts, deliberate memory, and cheap reuse before expensive reasoning.

## Octocode-First Research

When Octocode MCP tools are available, use them before relying on generic search or manual browsing:

- External research: use Octocode GitHub/package tools to find repos, inspect repository structure, read relevant files, and verify framework or skill patterns from source.
- Local code understanding: use Octocode local tools for file discovery and content search, then LSP for definitions, references, and call flow when the question depends on symbols or execution paths.
- Architecture and quality checks: use AST/scanner-style evidence when available for duplicated flow logic, unsafe boundaries, schema gaps, cache/state ownership, and dead or unreachable paths.
- Evidence rule: carry forward exact paths, symbols, lines, repo names, branches, and tool findings into the flow design. Do not invent framework behavior from memory when source evidence is reachable.

Core mental model:

```text
arr = [1, 2, 3]        product = 6
new item = 4

Bad cache use: 1 * 2 * 3 * 4
Good cache use: cached(product=6) * 4
```

For LLMs, cache the proven intermediate result, validate whether the new input invalidates it, then compute only the delta. Do not ask the model to rediscover facts that are already stable, cited, and fresh.

## When To Use

Use this skill when the user asks for any of:

- Agentic flow, agent workflow, multi-agent orchestration, subagent handoff, or agent routing.
- Memory, caching, context passing, session state, long-term memory, durable execution, or checkpointing.
- Structured output, Zod schemas, typed contracts, tool input/output schemas, or validation.
- Framework guidance for Google ADK, LangChain, LangGraph, CrewAI, OpenAI Agents SDK, or similar.
- A repeatable skill/workflow that should be descriptive, flexible, and not over-rigid.

Do not use this for a single one-shot prompt unless the output feeds another step or should be reused later.

## Operating Flow

Use as a checklist, not a strict sequence. Simple flows may skip or merge steps; complex flows may revisit them.

`INTENT` → `MODE` → `FLOW MODEL` → `STATE MAP` → `CACHE PLAN` → `PROTOCOLS` → `CONTRACTS` → `CONTEXT & TOKENS` → `PROMPT PLAN` → `FRAMEWORK MAP` → `FLOW DESIGN` → `GATES` → `VERIFY`

Think in contracts, not transcripts. A good agentic flow is understandable from its nodes, state, schemas, prompts, tool permissions, and traces without reading the entire chat history.

### 1. Intent

Write a one-paragraph goal contract before designing the flow:

- User-visible outcome.
- Inputs and expected outputs.
- Tools, APIs, files, or data sources.
- Safety constraints and forbidden actions.
- Latency/cost/quality preference.
- Whether the flow is prototype-only or production-bound.

**Intake gate — do I have enough to design this?** Check before proceeding:

- [ ] User-visible outcome is stated (not just a feature name).
- [ ] At least one concrete input and one expected output are known.
- [ ] Side effects (writes, deploys, messages) are identified or explicitly absent.
- [ ] Safety constraints or forbidden actions are known or assumed none.
- [ ] Prototype vs production requirement is clear.

If two or more are missing and their absence would change the architecture, ask one focused question. Otherwise proceed with stated assumptions.

### 2. Output Mode

Pick the smallest output that answers the user. Do not produce the production template by default.

| Mode | Use When | Include | Skip |
|---|---|---|---|
| `sketch` | Early idea, low risk, no implementation request | goal, 3-6 nodes, key state, main risks | full schemas, long examples, framework deep dive |
| `implementation` | User is about to build or asks how to structure the flow | state map, contracts, cache/memory, framework fit, verification | exhaustive observability and retention policy unless relevant |
| `production` | Multi-user, persistent memory, side effects, durable execution, or compliance risk | all sections, gates, evals, observability, privacy, failure recovery | nothing safety-critical |

Ask one focused question only when the missing answer changes the mode, framework, persistence model, or safety gate.

### 3. Flow Model

Model the flow in layers before selecting a framework:

| Layer | Question | Examples |
|---|---|---|
| Trigger | Why does the flow start? | user request, cron, event, queue, PR update |
| Planner | What decides the route? | classify, plan, choose tools, split tasks |
| Workers | Who does the work? | deterministic code, LLM node, subagent, external API |
| State | What is carried forward? | schemas, artifacts, cache keys, memory references |
| Protocols | How do parts communicate? | tool contracts, event envelopes, handoff packets |
| Gates | Where can execution pause? | human approval, risk check, budget stop, schema failure |
| Observability | How do we debug it? | trace IDs, node logs, evals, cache decisions |

Use this model to explain the flow before writing prompts or code. If the flow cannot be explained at this level, it is not ready to implement.

### 4. State Map

Classify every piece of information before passing it forward:

| Kind | Lifetime | Storage | Example | Rule |
|---|---:|---|---|---|
| `input` | One run | request payload | user request, uploaded file | Validate before use. |
| `working_state` | One run | graph/session state | current plan, extracted entities | Pass explicitly between nodes. |
| `checkpoint` | Retry/resume | durable checkpoint | completed node result | Store after expensive or side-effectful steps. |
| `cache` | Until invalidated | key/value or semantic cache | API response, retrieved docs, model summary | Reuse only with freshness and input-hash checks. |
| `memory` | Cross-session | memory service/vector store/db | user preference, project facts | Search, cite, and update deliberately. |
| `artifact` | Long-lived output | file/blob/db | report, generated code, trace | Version and link from state. |

Keep state narrow. Each node receives only the fields it needs plus references to retrievable artifacts.

**Never confuse `cache` and `memory`:**

| | Cache | Memory |
|---|---|---|
| Lifetime | Until invalidated (keyed) | Cross-session (persistent) |
| Keyed by | Input hash + versions | User / project / concept |
| Populated by | Deterministic recompute | Agent writes after reasoning |
| Privacy | Scoped by key; must not cross tenants | Requires consent and retention policy |
| Wrong use | Storing user preferences | Storing a model's intermediate computation |

### 5. Cache Plan

Caching is not "store everything"; it is "reuse stable work safely."

For every expensive step, define:

- `cacheKey`: deterministic key from normalized inputs, tool versions, model, prompt version, and data-source versions.
- `value`: the smallest validated result that can be reused.
- `freshness`: TTL, source commit/date, ETag, issue timestamp, or explicit invalidation rule.
- `scope`: per-run, per-user, per-repo, per-tenant, or global.
- `privacy`: whether the value can cross users/workspaces.
- `validation`: schema parse plus optional semantic check before reuse.
- `deltaPath`: how to update when new data arrives instead of recomputing everything.

Prefer incremental cache updates:

```text
cached_summary + new_diff -> updated_summary
cached_embedding_index + new_docs -> append/reindex changed docs only
cached_tool_result + unchanged input hash -> reuse
cached_plan + new requirement -> patch affected steps, do not regenerate whole plan
```

Never cache secrets, unredacted credentials, raw private user data without an explicit policy, or outputs that depend on hidden runtime state you cannot key.

### 6. Protocols

Protocols define how agents, tools, memory, caches, and humans communicate. Do not rely on prose-only handoffs for anything that another step must parse.

Define protocols for:

- `FlowInput`: the starting payload.
- `NodeInput` and `NodeOutput`: each step boundary.
- `ToolCall` and `ToolResult`: tool requests, outputs, errors, and retryability.
- `AgentHandoff`: subagent task packet and return packet.
- `MemoryQuery` and `MemoryWrite`: what may be searched or persisted.
- `CacheRead` and `CacheWrite`: key, freshness, scope, and validation.
- `HumanGate`: approval prompt, preview, options, and timeout behavior.
- `TraceEvent`: node name, timings, model/tool used, cache decision, and result status.

Protocol rules:

- Every protocol has an owner and a version.
- Every protocol has a Zod schema or equivalent runtime validator.
- Every protocol records provenance for facts that affect decisions.
- Every protocol distinguishes `missing`, `empty`, `unknown`, and `not_allowed`.
- Every protocol has an error shape; errors are data, not hidden side chatter.
- Protocols should be small and composable. Avoid one giant "agent state" blob.

### 7. Contracts With Zod

Every boundary needs a schema:

- User input.
- Tool input and output.
- LLM structured output.
- State between nodes.
- Cache entries.
- Memory entries.
- Final answer payload when another system consumes it.

Use Zod as the single source of truth in TypeScript. Infer types from schemas and validate at runtime.

```typescript
import * as z from "zod";

/** Agent flow input */
export const FlowInput = z.object({
  task: z.string().min(1),
  constraints: z.array(z.string()).default([]),
  freshness: z.enum(["live", "recent", "cached-ok"]).default("recent"),
});
export type FlowInput = z.infer<typeof FlowInput>;

/** Cached node result */
export const CacheEntry = z.object({
  key: z.string().min(1),
  value: z.unknown(),
  inputHash: z.string().min(1),
  createdAt: z.iso.datetime(),
  expiresAt: z.iso.datetime().optional(),
  sources: z.array(z.string()).default([]),
});
export type CacheEntry = z.infer<typeof CacheEntry>;

/** Agent handoff packet */
export const AgentHandoff = z.object({
  goal: z.string().min(1),
  inputs: z.record(z.string(), z.unknown()).default({}),
  knownFacts: z.array(z.object({
    fact: z.string().min(1),
    source: z.string().min(1),
  })).default([]),
  constraints: z.array(z.string()).default([]),
  expectedOutput: z.string().min(1),
  tokenBudget: z.number().int().positive().optional(),
});
export type AgentHandoff = z.infer<typeof AgentHandoff>;
```

Validation rules:

- Use `safeParse` at runtime boundaries; fail loudly with structured errors.
- Use discriminated unions for routed events and tool outcomes.
- Use strict objects when extra fields are dangerous; loose objects when envelopes evolve.
- Use defaults for stable optional behavior, not to hide missing required intent.
- Keep prompts and schemas versioned together when outputs are cached.

### 8. Context & Tokens

Context sharing is a design problem, not a bigger prompt problem. Token budget is part of flow design.

**Pass context as packets, not chat history dumps.** Each packet:

```text
goal:           what this step must accomplish
inputs:         validated data only
knownFacts:     stable cited facts (with sources)
decisions:      prior choices that must be preserved
openQuestions:  unresolved items (state them explicitly)
constraints:    safety, style, budget, user preferences
artifacts:      file paths, URLs, cache keys, trace IDs
expectedOutput: schema and success criteria
```

**Before expanding context, try in order:**
1. Reuse a validated cache entry.
2. Retrieve the smallest relevant artifact chunk.
3. Summarize with citations and omissions noted.
4. Ask a focused question.
5. Only then pass broader context.

**Rules:**
- Share artifact references, not large content inline.
- Share decisions and constraints separately — downstream agents must know what is fixed.
- Strip secrets, stale facts, and speculative notes before handoff.
- Do not send invariant instructions to every subagent — put them in the protocol or system skill.
- Do not ask an LLM to sort, filter, count, diff, or validate what deterministic code can handle.
- Accept only schema-valid return packets plus evidence.

### 9. Prompt Quality

Prompts are flow interfaces. They must be testable.

A high-quality node prompt includes:

- Role: what expertise the model should apply.
- Goal: the exact job of this node, not the entire product vision.
- Inputs: validated fields and artifact references.
- Constraints: safety, style, budget, tool permissions, and framework rules.
- Decision criteria: how to choose between options.
- Output schema: the exact structure expected.
- Failure behavior: when to ask, retry, fallback, or stop.
- Evidence requirements: what sources or traces must support the result.

Prompt checks:

- Can another engineer tell what this node owns and what it must not do?
- Is the expected output parseable without reading prose?
- Are tool permissions explicit?
- Are hidden assumptions converted into inputs, defaults, or gates?
- Can this prompt be evaluated with golden examples?
- Does it avoid asking the model to both decide and verify its own risky action without a gate?

### 10. Framework Map

Pick the smallest framework shape that preserves the flow.

| Need | Good Fit | Guidance |
|---|---|---|
| Durable state machine, retries, human review, long-running flow | LangGraph | Model nodes and edges explicitly; use checkpoints for resume and state inspection. |
| Composable LLM app with tools, retrievers, structured output | LangChain | Keep chains small; validate structured outputs; add observability/evals for trajectories. |
| Google Cloud agent lifecycle, ADK samples, deployment/evals/observability | Google ADK | Scaffold/enhance first; define agents, tools, callbacks, sessions, memory services, evals, and deployment separately. |
| Lightweight deterministic pipeline | Plain TypeScript/Python + Zod/Pydantic | Prefer this when the flow is mostly deterministic transforms and tool calls. |
| Multi-agent delegation | Any framework | Delegate only when the subtask has separate tools, context, or expertise; pass a compact contract and expected artifact. |

Google ADK memory distinction:

- Use `Session` / `State` for one ongoing conversation.
- Use `MemoryService` for searchable long-term knowledge across sessions.
- Use in-memory services only for local development or tests.
- Share session and memory services across runners when agents must see the same state.

LangGraph/LangChain distinction:

- Use graph state for the current run.
- Use checkpointers for durable execution and retries.
- Use long-term memory or vector stores for cross-session recall.
- Use tracing/evals to inspect state transitions, tool calls, and trajectory quality.

### 11. Flow Design

Describe the flow as flexible nodes, not a rigid script.

For each node, specify what applies (not all fields are required for simple nodes):

- `name`: stable identifier.
- `purpose`: why this node exists.
- `inputSchema` / `outputSchema`: required for every LLM or side-effecting node.
- `cachePolicy`: read/write/skip and invalidation rule — only when the step is expensive or reused.
- `memoryPolicy`: search/write/ignore and privacy rule — only when crossing session boundaries.
- `tools`: allowed tools only.
- `failureMode`: retry, ask user, fallback, or stop.
- `sideEffects`: list when the node writes files, calls external APIs, or sends messages.

Default node types:

- `classify`: route intent and risk.
- `retrieve`: gather code/docs/logs/memory.
- `compute`: deterministic transform or tool call.
- `reason`: LLM synthesis with schema.
- `delegate`: subagent with bounded contract.
- `review`: human or automated quality gate.
- `act`: side effect.
- `observe`: trace, metrics, eval, or cache/memory update.

### 12. Gates

> **STOP — before executing any `act`, `delegate`, or side-effecting node, confirm the relevant gate condition is not triggered.**

Stop and ask the user before continuing when:

- The flow would persist personal/private memory without a clear retention policy.
- A cache may leak data across users, tenants, repos, or workspaces.
- A side effect is destructive, costly, externally visible, or hard to undo.
- The framework choice changes deployment, auth, billing, or cloud ownership.
- Evidence conflicts and the next decision would encode one side as truth.
- The schema cannot represent required ambiguity without lying.
- Token limits force dropping required evidence or safety constraints.
- A prompt is too vague to evaluate but will drive side effects or memory writes.

### 13. Verification

Run before shipping. For each check, record evidence and a fix when status is `warn` or `fail`.

| Check | What to verify |
|---|---|
| Completeness | Every trigger has a terminal outcome or explicit wait state. |
| Schema coverage | Every boundary (input, output, cache, memory, handoff) has a validator. |
| State ownership | Each state field has one writer or an explicit conflict policy. |
| Cache safety | Every cache has key, scope, freshness, privacy, and invalidation defined. |
| Memory safety | Every memory write has consent, purpose, retention, and delete behavior. |
| Context discipline | Each node receives only the fields it needs plus artifact references. |
| Token budget | Large context is summarized, chunked, or retrieved — not copied inline. |
| Prompt quality | Prompts include role, goal, constraints, output schema, and failure behavior. |
| Tool safety | Side-effecting tools have risk classification and a gate when needed. |
| Observability | Traces show node inputs, outputs, model/tool, cache decision, and errors. |

For production flows: add schema tests, cache hit/miss/stale/invalidation tests, "should not remember" memory tests, tool-failure and idempotency tests, golden trajectories, and latency/cost budgets.

## Output Template

Choose the smallest template that matches flow complexity and user intent.

### Small Flow (prototype, single-agent, no persistent memory)

```markdown
## Flow: <name>
Goal: <one sentence>
Framework: <plain code | LangChain | LangGraph>

Nodes:
1. <classify|retrieve|reason|act>: <input> → <output> | fail: <retry|stop>
2. ...

Gates: <none | list stop conditions>
Verify: <schema tests, golden example>
```

### Implementation Flow (build-ready, moderate risk)

```markdown
## Flow: <name>
Goal: <one paragraph with inputs, outputs, constraints>
Framework: <recommendation + why>

State Map:
- <item> → <kind>, <lifetime>, <owner>, <storage>

Contracts: <FlowInput, NodeOutput, ToolResult, CacheEntry>
Cache & Memory: <keys, freshness, scope, memory search/write policy>

Flow:
1. <node>: <input> → <action> → <output> | fail: <mode>

Gates: <approval or stop conditions>
Verification: <schema tests, cache tests, golden examples>
```

### Production Flow (multi-step, memory, side effects, evals)

```markdown
## Flow: <name>
Goal: <one paragraph — outcome, inputs, outputs, constraints, prototype vs prod>

State Map:
- <item> → <kind>, <lifetime>, <owner>, <storage>

Cache & Memory:
- Cache: <key, freshness, scope, invalidation>
- Memory: <session vs long-term, search/write policy>

Contracts: <schema names and boundaries each validates>
Protocols: <handoff/tool/cache/memory/trace protocols>

Context & Tokens:
- Packet: <goal, knownFacts, decisions, constraints, artifacts, expectedOutput>
- Budget: <cache/retrieve/summarize policy, high-token nodes>

Prompt Quality: <role, output schema, failure behavior per node>

Framework: <recommendation + 2-4 bullets why>

Flow:
1. <node>: <input> → <action> → <output> | cache: <policy> | fail: <mode>

Gates: <stop conditions requiring user approval>

Verification: <schema tests, cache tests, golden trajectories, latency budgets>
```

### Filled Example (small flow)

```markdown
## Flow: Repo Security Scanner
Goal: Scan a GitHub repo for hardcoded secrets and report findings.
Framework: Plain TypeScript + Zod

Nodes:
1. classify: { repo_url } → risk_level (low/med/high) | fail: stop if url invalid
2. retrieve: { repo_url, branch } → file_list (cache: input_hash, TTL 1h) | fail: retry x2
3. compute: { file_list } → matches[] via ripgrep — deterministic, no LLM | fail: stop
4. reason: { matches[], repo_url } → findings_report (JSON schema) | fail: fallback empty report
5. act: { findings_report } → post GitHub issue | GATE: requires user approval if high risk

Gates: act node requires approval when risk_level = high or findings.length > 5
Verify: schema tests for findings_report, cache hit/miss for retrieve, golden fixture with known secrets
```

## Worked Example

> `references/pr-review-agent-example.md` — a build-ready PR Review Agent showing all template fields filled in: State Map, Cache & Memory, Contracts, Flow nodes, Gates, and Verification. Use as a quality bar before producing your own output.

## Anti-Patterns

- Recomputing stable LLM work because no cache key exists.
- Passing full chat history to every agent instead of a compact context packet.
- Storing raw conversation turns or unvalidated reasoning in long-term memory — only distilled, cited facts belong there.
- Writing memory before the user confirms that the fact is worth remembering.
- Letting a subagent choose tools outside its typed contract.
- Delegating to a subagent without an `AgentHandoff` schema and expected output defined.
- Letting a `reason` node both decide and execute a side-effecting action — put a gate between them.
- Hiding schema failures by asking the LLM to "try again" indefinitely.
- Using a graph framework when a deterministic function pipeline is enough.
- Using a one-shot chain when durable state, retries, or human review are required.
- Using semantic memory for session-scoped data — use graph state or checkpoints instead.
- Skipping observability because the flow "looks simple" — every production agent needs traces.
- Building multi-agent before the single-agent core loop is proven to work.

## References & Resources

Before implementing any node, cache plan, memory pattern, or framework choice, consult:

> `references/resources.md` — curated repos organized by Memory, RAG, Frameworks, Education, and Cookbooks, each mapped to the relevant section of this skill.

Use these as golden-trajectory examples, implementation references, and eval baselines.
