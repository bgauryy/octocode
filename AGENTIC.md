# Agentic Engineering — The Complete Developer Cheat Sheet

Context Engineering · Agent Architecture · MCP · Skills · Caching · Memory · Tool Routing · Compaction · Evaluation · Token Efficiency

Core principle: An effective agent is not the one that reads the most information, invokes the most tools, or produces the longest answer. It is the one that selects sufficient evidence, performs the necessary actions, verifies the outcome, and stops.

Modern agent engineering is increasingly about managing what the model sees, what it can do, what it remembers, and when it should act. This aligns with current research on context engineering and tool design.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.anthropic.com\&sz=32)

Anthropic+1



## 1. Agentic Architecture: The Big Picture

User Request / Task

Intent · Scope · Constraints · Success criteria

Agent Orchestrator

Plan · Route · Budget · Permissions · Checkpoints

Context

Retrieval, memory, compaction

Models

Fast, reasoning, verifier

Capabilities

Tools, MCP, skills

Execution + Evidence

Read · Search · Modify · Test · Observe

Verify → Return → Stop

Grounded result · Artifacts · Residual uncertainty

## 2. Core Agentic Concepts

| Concept             | Purpose                                                     |
| ------------------- | ----------------------------------------------------------- |
| Agent Loop          | Observe → decide → act → evaluate                           |
| ReAct               | Interleave reasoning and tool actions                       |
| Plan-and-Execute    | Separate planning from execution                            |
| Planner–Executor    | Use specialized planning and action components              |
| Router Agent        | Choose model, tool, skill or subagent                       |
| Supervisor–Worker   | Delegate bounded tasks to specialist agents                 |
| Evaluator–Optimizer | Critique and improve outputs                                |
| Reflection          | Reassess work using new evidence                            |
| Human-in-the-Loop   | Request approval for consequential actions                  |
| Tool-Calling Agent  | Execute typed external operations                           |
| Stateful Agent      | Maintain task state between steps                           |
| Stateless Agent     | Reconstruct necessary state per request                     |
| Agent Handoff       | Transfer responsibility and relevant context                |
| Checkpoint / Resume | Persist progress across interruptions                       |
| Durable Execution   | Recover ongoing workflows without blindly repeating actions |
| Agent Sandbox       | Isolate execution and limit permissions                     |
| Agent Termination   | Stop after success, failure or budget exhaustion            |

Default: Use a single bounded agent loop. Introduce more agents only when the work genuinely benefits from specialization, parallelism or isolated context.

## 3. Context Window & Attention Management

A context window is finite capacity. It is not a guarantee that every included fact will be used correctly.

| Technique              | What it solves                                         |
| ---------------------- | ------------------------------------------------------ |
| Context Budgeting      | Reserve space for input, tools, reasoning and output   |
| Context Selection      | Include only task-relevant content                     |
| Progressive Disclosure | Load details when required                             |
| Just-in-Time Retrieval | Retrieve information at point of use                   |
| Context Pruning        | Remove obsolete or redundant material                  |
| Context Compression    | Reduce representation size                             |
| Context Compaction     | Preserve essential working state across long sessions  |
| Context Isolation      | Keep unrelated tasks in separate contexts              |
| Context Anchoring      | Keep goals, constraints and evidence easy to locate    |
| Context Rehydration    | Restore referenced information when needed             |
| Context Deduplication  | Avoid repeated documents and tool outputs              |
| Provenance Tracking    | Record which source supports which fact                |
| Context Versioning     | Detect outdated repository or document state           |
| Attention Management   | Reduce competing, irrelevant instructions and evidence |

### Common context failures

| Failure                  | Symptom                                 | Prevention                              |
| ------------------------ | --------------------------------------- | --------------------------------------- |
| Context overflow         | Request exceeds context limit           | Budget + compaction                     |
| Context pollution        | Irrelevant information dominates        | Filter and rank                         |
| Context rot              | Performance declines as context grows   | Prune and refresh                       |
| Lost-in-the-middle       | Important information is neglected      | Focused retrieval and salient structure |
| Stale context            | Agent uses outdated information         | Versions and freshness checks           |
| Summary drift            | Compaction changes facts                | Structured, source-backed summaries     |
| Instruction collision    | Conflicting directions                  | Explicit instruction precedence         |
| Tool-output flooding     | Huge results obscure task               | Pagination and projections              |
| Cross-task contamination | Unrelated task details influence output | Isolated contexts                       |
| Evidence laundering      | Inference becomes treated as fact       | Provenance and confidence labels        |

## 4. Context Budget Allocation

## Interactive Context Budget

Illustrative

Available context

128K tokens

System / rules / schemas

10% · 12,800 tokens

Conversation / task state

15% · 19,200 tokens

Retrieved files / tool results

35% · 44,800 tokens

Output allowance

15% · 19,200 tokens

Remaining headroom

25% · 32,000 tokens

Unallocated headroom can absorb tool outputs, metadata, model-specific token usage and unexpected expansion. Actual usable budgets depend on the API and model.

Important: These percentages are adjustable planning examples, not universal best-practice ratios. Do not fill the context window simply because capacity remains.

## 5. Caching Architecture

Caching in agentic systems is not one mechanism. Different caches solve different problems.

| Cache                 | What is cached                         | Key concern                 |
| --------------------- | -------------------------------------- | --------------------------- |
| Prompt / Prefix Cache | Reusable model input prefix            | Prefix stability            |
| KV Cache              | Transformer attention key/value states | Model/runtime support       |
| Tool Result Cache     | External API/tool responses            | Freshness and permissions   |
| Retrieval Cache       | Search results                         | Index version and query     |
| Embedding Cache       | Text → vector representations          | Embedding model version     |
| Semantic Cache        | Answers to similar queries             | False semantic matches      |
| Session Cache         | Session-specific working information   | Expiration and isolation    |
| Memory Cache          | Frequently recalled facts              | Staleness                   |
| Skill Cache           | Reusable skill instructions/resources  | Skill version               |
| MCP Discovery Cache   | Tool/resource definitions              | Capability changes          |
| Computation Cache     | Deterministic intermediate results     | Complete input identity     |
| Artifact Cache        | Files, builds, analyses                | Content hashes              |
| Negative Cache        | Known absent results                   | Short expiration            |
| Subagent Result Cache | Completed delegated findings           | Scope and evidence validity |

### Prompt cache optimization

Recommended prompt structure

Stable prefix

Cache-friendly

System instructions → developer constraints → stable tool definitions → shared reference material

Dynamic suffix

Current task → retrieved evidence → new tool results → latest user input

Recommended practices:

- Keep stable tool schemas and instructions consistently ordered.
- Append new conversation content rather than rewriting previous content when supported.
- Avoid inserting timestamps or changing identifiers into otherwise stable prefixes.
- Use deferred tool loading when available.
- Measure cached-token counts, total cost and latency.

OpenAI's current prompt-caching documentation describes model-dependent prefix matching, configurable caching behavior, and additional controls for newer models. It also notes that compaction may change prefixes and reduce cache reuse.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://developers.openai.com\&sz=32)

OpenAI API+1



Important distinction: A prompt cache is a performance optimization, not long-term agent memory.

## 6. Agent Memory Architecture

| Memory type            | Content                            | Persistence                    |
| ---------------------- | ---------------------------------- | ------------------------------ |
| Working Memory         | Current task state                 | One task/session               |
| Episodic Memory        | Past runs, decisions, outcomes     | Cross-session                  |
| Semantic Memory        | Stable facts and project knowledge | Cross-session                  |
| Procedural Memory      | Workflows, policies and skills     | Versioned                      |
| Artifact Memory        | Code, files, reports, test results | External storage               |
| Conversation Memory    | Relevant dialogue history          | Session or persistent          |
| Task Memory            | Plan, progress, blockers           | Until completion               |
| User Preference Memory | Explicit durable preferences       | Subject to user controls       |
| Failure Memory         | Failed approaches and evidence     | Appropriate task/project scope |

### Memory lifecycle

Observe

Extract

Validate

Store

Retrieve

Use

Refresh / Forget

For durable facts, useful memory metadata includes `source`, `scope`, `created_at`, `updated_at`, `confidence`, `version`, `expires_at`, and `supersedes`.

Do not store every interaction as memory. Extract reusable, validated information. Provide deletion, expiration, isolation and correction mechanisms.

## 7. Compaction & Long-Horizon Execution

| Strategy                   | Best use case                           | Danger                         |
| -------------------------- | --------------------------------------- | ------------------------------ |
| Tool Result Clearing       | Old bulky tool outputs                  | Removing needed evidence       |
| Structured Summary         | Long conversations                      | Losing critical details        |
| Checkpoint Notes           | Long engineering tasks                  | Stale progress information     |
| Hierarchical Summaries     | Extensive research and code exploration | Accumulated summary drift      |
| Task-Specific Compaction   | Preserve details needed for next action | Incorrect relevance prediction |
| Externalized Artifacts     | Large logs, diffs, datasets             | Broken references              |
| Subagent Isolation         | Separate exploration contexts           | Incomplete handoffs            |
| Rolling Working Set        | Maintain recent active context          | Forgetting older constraints   |
| Provider-Native Compaction | Supported long-running model sessions   | Provider-specific semantics    |

Anthropic describes compaction, structured note-taking, and subagents as complementary techniques for long-horizon tasks. OpenAI also documents server-side and explicit compaction options for its Responses API.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.anthropic.com\&sz=32)

Anthropic+1



### What a compactor must preserve

Example structured checkpoint

```

task:
  goal: "Fix duplicate payment processing"
  status: "implementation"
  success: "one logical charge per idempotency key"

confirmed:
  - "Retry path can repeat an external request"
  - "Database transaction does not cover provider call"

decisions:
  - "Use persistent idempotency records"

changed:
  - "src/payments/service.ts"

evidence:
  - "tests/payments/retry.test.ts"
  - "trace: run-4821"

pending:
  - "Run integration tests"
  - "Verify timeout recovery"

failed_attempts:
  - "In-memory lock: insufficient across workers"

constraints:
  - "Preserve existing public API"

next_action: "Run targeted integration tests"
  
```

Preserve the goal, constraints, decisions, precise identifiers, verified findings, failed approaches, modified artifacts, unfinished work, and next action.

Discard redundant narration, obsolete intermediate reasoning, already-consumed tool output and repeated explanations. Keep source references so facts can be rehydrated.

Compaction must not silently transform an inference into a confirmed fact. Test continuity after compaction by asking whether the agent can still identify the next correct action and its supporting evidence.

## 8. Tools, MCP, Skills & Triggers

These concepts serve distinct roles.

| Component          | What it represents                       | Trigger                          |
| ------------------ | ---------------------------------------- | -------------------------------- |
| Tool               | Executable capability                    | Required external data or action |
| MCP Server         | Standardized capability provider         | Relevant connected service       |
| MCP Resource       | Readable contextual data                 | Need to inspect external context |
| MCP Prompt         | Reusable prompt template                 | User/workflow invokes it         |
| Agent Skill        | Domain-specific procedure and references | Task matches skill purpose       |
| Subagent           | Delegated worker with its own context    | Bounded independent subtask      |
| Workflow           | Predetermined sequence or graph          | Known repeatable process         |
| Hook               | Event-triggered processing               | Lifecycle event                  |
| Policy / Guardrail | Constraint and validation                | Before or after selected actions |
| Scheduler          | Time- or event-based invocation          | Scheduled trigger                |
| Router             | Chooses capabilities                     | Task classification              |

MCP standardizes communication among hosts, clients, and servers and exposes capabilities such as tools, resources and prompts. Agent Skills use progressive disclosure to load instructions and resources as needed.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub+1



### Recommended tool triggering logic

| Situation                                     | Preferred action                       |
| --------------------------------------------- | -------------------------------------- |
| Answer supported by existing verified context | Answer directly                        |
| Need current external facts                   | Search or retrieve                     |
| Know exact resource identifier                | Read exact resource                    |
| Know topic, not location                      | Search metadata or index               |
| Need only a subset                            | Filter and paginate                    |
| Multiple independent reads                    | Parallelize when safe                  |
| Need a reusable procedure                     | Load matching skill                    |
| Need substantial isolated analysis            | Delegate subagent                      |
| Need to modify external state                 | Validate permissions, then execute     |
| Consequential or irreversible action          | Require applicable approval            |
| Evidence is sufficient                        | Stop retrieval                         |
| A tool repeatedly fails                       | Diagnose, use bounded fallback or stop |

The model should not invoke a tool merely because it exists.

## 9. MCP Server Design Best Practices

| Concern          | Good design                                   |
| ---------------- | --------------------------------------------- |
| Naming           | Clear namespace and action verbs              |
| Discovery        | Concise descriptions and typed schemas        |
| Tool selection   | Distinct capabilities, minimal overlap        |
| Input validation | Reject invalid or unauthorized arguments      |
| Output schema    | Structured and predictable                    |
| Error semantics  | Typed, actionable failure results             |
| Pagination       | Opaque cursors and bounded responses          |
| Filtering        | Server-side filtering and projection          |
| Authentication   | Scope permissions by identity                 |
| Authorization    | Enforce permissions at execution time         |
| Idempotency      | Support safe retries where applicable         |
| Cancellation     | Stop unnecessary long-running work            |
| Timeouts         | Explicit execution deadlines                  |
| Observability    | Request IDs, latency, cost and error metadata |
| Versioning       | Stable contracts and compatibility            |
| Security         | Treat server-delivered content as untrusted   |

Good agent-oriented tools return information useful for a decision rather than huge raw payloads. Anthropic's tool-engineering guidance specifically emphasizes distinct tools, meaningful return context, and token-efficient responses.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.anthropic.com\&sz=32)

Anthropic



## 10. Agent Skills & Progressive Disclosure

A useful skill structure:

```

skills/
  architecture-review/
    SKILL.md
    references/
      review-checklist.md
      coupling-metrics.md
      examples.md
    scripts/
      analyze-dependencies.py
    assets/
      adr-template.md

```

### Three-stage loading

1. Discover

   Read short name and description to assess relevance.
2. Activate

   Read `SKILL.md` only when needed for the task.
3. Execute

   Open specific references, scripts, and templates only as required.

The Agent Skills specification recommends keeping the core skill concise and using separately loaded reference files for detail.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



Strong skill descriptions explain when to use the skill, when not to use it, required inputs, and expected outputs. Avoid making every skill claim to handle all engineering tasks.

Also distinguish trusted local instructions from remotely supplied skills. MCP-served skill content introduces a prompt-injection and execution-trust boundary.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://skills.extensions.modelcontextprotocol.io\&sz=32)

MCP Skills Extension



## 11. Retrieval, Pagination & Selective Reading

Principle: Discover broadly, inspect narrowly, expand only when evidence requires it.

| Technique                  | Preferred use                                    |
| -------------------------- | ------------------------------------------------ |
| Metadata-First Retrieval   | Find relevant files without reading all contents |
| Hierarchical Search        | Navigate repo → module → symbol                  |
| Hybrid Search              | Combine keyword and semantic retrieval           |
| Exact-Match Search         | Locate symbols, identifiers and error messages   |
| Semantic Search            | Discover conceptually related material           |
| Reranking                  | Prioritize the most relevant results             |
| Top-K Retrieval            | Limit initial results                            |
| Cursor Pagination          | Browse large result sets                         |
| Range Reading              | Read relevant lines or sections                  |
| Projection                 | Retrieve only required fields                    |
| Incremental Expansion      | Load more around relevant matches                |
| Deduplication              | Remove repeated hits                             |
| Evidence Coverage Tracking | Determine which questions remain unresolved      |
| Stop Criteria              | End retrieval when sufficient evidence exists    |

MCP supports cursor-based pagination for list operations. Clients must treat cursors as opaque and follow `nextCursor` when more pages are needed.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://ts.sdk.modelcontextprotocol.io\&sz=32)

MCP TypeScript SDK



### Adaptive retrieval procedure

```
def retrieve_evidence(query, search, fetch):    hits = search(        query=query,        limit=20,        fields=["id", "title", "snippet", "score"]    )    ranked = rerank(hits, query)    evidence = []    seen = set()    for hit in ranked:        if hit["id"] in seen:            continue        seen.add(hit["id"])        if sufficient(evidence, query):            break        item = fetch(            id=hit["id"],            relevant_ranges=True,            max_tokens=2500        )        evidence.append(item)    return evidence
```

Conceptual pseudocode: `rerank` and `sufficient` represent application-specific policies. A reliable implementation also handles timeouts, authorization, stale search hits, and incomplete coverage.

### Pagination rules

1. Start with a small, relevant page.
2. Prefer server-side filters and sort order.
3. Preserve stable cursors and snapshot/version information where supported.
4. Continue only when relevant evidence is still missing.
5. Deduplicate items across pages.
6. Bound total pages, time, tokens and requests.
7. Do not claim an exhaustive search unless the required pages or scope were actually examined.

## 12. Let the Agent Determine What It Needs

Avoid rigid rules such as “always read the first ten files,” “always call three tools,” or “always retrieve five pages.”

Instead, use a need-driven retrieval policy.

## Evidence-Driven Decision Loop

1

1\. Define question

What decision or claim needs evidence?

2

2\. Inventory knowledge

What is already verified?

3

3\. Identify gaps

Which missing fact could change the answer?

4

4\. Choose cheapest adequate action

Exact lookup, metadata search, selective read or experiment

5

5\. Update evidence

Mark findings as confirmed, inferred or unresolved

6

6\. Evaluate sufficiency

Can the next action be justified?

7

7\. Continue or stop

Retrieve more only for a material gap

A useful heuristic for the next retrieval action is:

\\[ \text{Utility}=\frac{\text{Expected reduction in decision uncertainty}}{\text{Latency}+\text{Token cost}+\text{Operational risk}} \\]

This is a conceptual scoring function, not a calibrated mathematical guarantee. Practical implementations can estimate relevance, evidence gaps, freshness and cost.

## 13. Multi-Model / Dual-LLM Architecture

A dual-LLM system uses different models for different roles. It does not necessarily mean both models run on every request.

| Pattern                     | Architecture                                                       | Best use                   |
| --------------------------- | ------------------------------------------------------------------ | -------------------------- |
| Fast → Strong               | Small model handles easy tasks; strong model handles hard cases    | Cost-efficient routing     |
| Planner → Executor          | Reasoning model plans; execution model performs bounded steps      | Complex workflows          |
| Generator → Verifier        | One produces; another checks against evidence                      | High-consequence outputs   |
| Retriever → Synthesizer     | Lightweight retrieval agent gathers context; strong model reasons  | Research and code analysis |
| Router → Specialists        | Router delegates to domain-specific models                         | Heterogeneous workloads    |
| Draft → Refine              | Fast draft followed by selective refinement                        | Writing or transformation  |
| Parallel Candidates → Judge | Several approaches compared by an evaluator                        | Hard reasoning tasks       |
| Independent Cross-Check     | Different models independently analyze evidence                    | Error detection            |
| Local → Cloud               | Private/local model for limited tasks, remote model when necessary | Privacy or cost control    |
| Escalation Cascade          | Increase capability only when uncertainty remains                  | Efficient inference        |

### Default escalation policy

Fast model

Classify, route, extract, summarize simple structured data

Complexity / uncertainty gate

Missing evidence? Complex reasoning? High consequences?

No

Return verified result

Yes

Escalate to stronger model

Selective verifier

Check evidence, tests or risky assertions; do not automatically add another model call

### Avoid common multi-agent inefficiencies

- Do not delegate trivial subtasks.
- Do not let every agent read the same large context.
- Pass precise task contracts and evidence references.
- Return concise findings instead of full transcripts.
- Run independent tasks in parallel, dependent tasks sequentially.
- Avoid recursive delegation without a depth or cost limit.
- Evaluate whether multiple agents outperform a strong single agent on the actual workload.

## 14. Subagent Handoffs: Return Contracts

An effective subagent should return a bounded result, not its entire process history.

```

type Finding = {
  claim: string;
  source: string;
  confidence: "confirmed" | "inferred" | "unknown";
};

type AgentResult = {
  taskId: string;
  status: "done" | "blocked" | "failed";
  summary: string;
  findings: Finding[];
  changedArtifacts: string[];
  tests: {
    name: string;
    result: "passed" | "failed" | "not_run";
  }[];
  remainingRisks: string[];
  recommendedNextAction?: string;
};

```

Require a clear status, evidence references, changed artifacts, test results, uncertainties, and follow-up needs.

Do not confuse “the worker returned successfully” with “the task was proven correct.”

## 15. Efficient Tool Output Design

Tool outputs should support the next decision.

| Poor response            | Better response                                                |
| ------------------------ | -------------------------------------------------------------- |
| Entire repository tree   | Relevant paths, counts and matching modules                    |
| Full build log           | Status, errors, first actionable locations, artifact reference |
| Entire database rows     | Requested fields, filtered records, cursor                     |
| Unbounded search hits    | Ranked top results and `hasMore`                               |
| 10,000-line file         | Symbol metadata and relevant ranges                            |
| Full test output         | Pass/fail counts, failures, logs reference                     |
| Raw exception flood      | Typed failure, cause, retryability                             |
| Entire webpage           | Title, relevant excerpt, source reference                      |
| Full subagent transcript | Evidence-backed summary and outstanding risks                  |

A practical tool result envelope:

```

{
  "status": "ok",
  "summary": "Found 3 relevant handlers",
  "items": [
    {
      "path": "src/api/orders.ts",
      "symbol": "createOrder",
      "lineStart": 48,
      "lineEnd": 91
    }
  ],
  "nextCursor": "opaque-token",
  "truncated": false,
  "sourceRevision": "commit-hash",
  "warnings": []
}

```

Return metadata that helps the agent choose the next action: identifiers, counts, locations, status, truncation flags, pagination cursors, revision IDs and errors.

## 16. Efficiency vs. Verbosity vs. Hallucinations

These are three separate optimization dimensions.

| Dimension            | Goal                                     | Anti-pattern                      |
| -------------------- | ---------------------------------------- | --------------------------------- |
| Token Efficiency     | Use minimal necessary context and output | Repeated full files               |
| Reasoning Efficiency | Perform enough analysis for correctness  | Unnecessary deliberation          |
| Tool Efficiency      | Minimize unnecessary calls               | Repeated broad searches           |
| Latency Efficiency   | Reduce critical-path duration            | Serial independent tasks          |
| Cost Efficiency      | Match model capability to task           | Strongest model for trivial tasks |
| Evidence Quality     | Support claims with reliable sources     | Unverified conclusions            |
| Answer Conciseness   | Return what the consumer needs           | Long process narration            |
| Correctness          | Satisfy actual requirements              | Superficial plausible output      |
| Recovery Efficiency  | Resume without repeating work            | No checkpoints                    |
| Operational Safety   | Limit unintended effects                 | Unbounded autonomous actions      |

### Verbosity routing

| Output destination             | Recommended format                          |
| ------------------------------ | ------------------------------------------- |
| Human asking a simple question | Direct concise answer                       |
| Developer requesting debugging | Root cause, evidence, fix, tests            |
| Calling agent                  | Structured result with references           |
| Build pipeline                 | Machine-readable status and artifacts       |
| Architecture reviewer          | Findings, impact, evidence, recommendations |
| Incident report                | Timeline, cause, impact, corrective actions |
| Autonomous workflow            | State transition, next action, blockers     |
| Detailed research              | Evidence-backed structured explanation      |

Concision does not mean omitting material uncertainty or necessary proof. Make output short by removing repetition, not by removing evidence.

## 17. Hallucination Prevention & Grounding

| Technique                   | Purpose                                    |
| --------------------------- | ------------------------------------------ |
| Grounded Generation         | Tie conclusions to retrieved evidence      |
| Citation / Provenance       | Identify supporting sources                |
| Claim Verification          | Check important factual statements         |
| Abstention                  | Avoid unsupported conclusions              |
| Uncertainty Representation  | Separate known and unknown                 |
| Tool-Based Verification     | Validate through external state            |
| Schema Validation           | Enforce output shape                       |
| Type Checking               | Detect invalid code or contracts           |
| Execution-Based Testing     | Validate actual behavior                   |
| Independent Verification    | Reduce some generation errors              |
| Contradiction Checking      | Detect inconsistent evidence               |
| Source Freshness Validation | Avoid outdated conclusions                 |
| Adversarial Testing         | Expose brittle assumptions                 |
| Faithfulness Evaluation     | Measure whether statements follow evidence |

### Evidence classification

Confirmed

Directly supported by inspected code, trusted source, test or observation.

Inferred

Reasonable interpretation of evidence, not directly verified.

Unknown

Insufficient evidence; investigation or qualification required.

For coding agents: never claim a test passed if it was not executed. Similarly, do not report that all references were inspected if search coverage was partial.

An LLM judge can be useful, but it is not a substitute for compilation, integration tests, runtime evidence, or authoritative data.

## 18. Avoid Patchy Agent Behavior

“Patchy” agents make local fixes without understanding the system, repeat tool calls, lose state, or produce inconsistent changes.

| Anti-pattern            | Failure                              | Better approach                    |
| ----------------------- | ------------------------------------ | ---------------------------------- |
| Patch-First Coding      | Fix before understanding cause       | Reproduce and trace behavior       |
| Shotgun Editing         | Modify many files speculatively      | Identify minimal causal change     |
| Happy-Path Fix          | Ignore edge cases                    | Verify invariants and failures     |
| Blind Retry             | Repeat same failed call              | Classify error before retry        |
| Context Thrashing       | Continually reload information       | Persist evidence and checkpoints   |
| Tool Thrashing          | Alternate tools without progress     | Select explicit information goal   |
| Speculative Refactoring | Expand scope unnecessarily           | Keep changes bounded               |
| Duplicate Policy Logic  | Multiple competing implementations   | Establish one policy owner         |
| Fake Completion         | Claim success from code plausibility | Execute verification               |
| Over-Delegation         | Excessive coordination overhead      | Delegate only separable work       |
| Over-Compaction         | Lose important task information      | Preserve invariants and provenance |
| Prompt Accretion        | Endless rules and exceptions         | Consolidate into coherent policy   |
| Unbounded Exploration   | Search indefinitely                  | Set sufficiency and budget gates   |
| Premature Optimization  | Optimize without evidence            | Profile and benchmark              |
| Architecture Drift      | Changes violate module boundaries    | Enforce architecture tests         |

### Better engineering workflow

1. Understand — State the problem, acceptance criteria and constraints.
2. Inspect — Identify ownership, boundaries, call paths and relevant source.
3. Form hypotheses — Compare the likely cause with a plausible rival.
4. Reproduce — Establish a failing test or observable baseline.
5. Change — Implement the smallest coherent correction.
6. Verify — Run relevant checks and inspect unintended consequences.
7. Return — Summarize changed artifacts, evidence and remaining risks.

## 19. Loop Control, Triggering & Stopping

Every production agent needs explicit execution boundaries.

| Control                  | Why                                    |
| ------------------------ | -------------------------------------- |
| Max Steps                | Prevent infinite action loops          |
| Max Tool Calls           | Bound external operations              |
| Max Token Budget         | Control model usage                    |
| Max Cost                 | Control financial exposure             |
| Deadline                 | Bound end-to-end duration              |
| Retry Budget             | Prevent repeated failures              |
| Delegation Depth         | Avoid recursive agent explosions       |
| Concurrency Limit        | Protect external systems               |
| Progress Detection       | Detect repeated nonproductive behavior |
| No-Progress Counter      | Trigger escalation or termination      |
| Cancellation Propagation | Stop unnecessary downstream actions    |
| Checkpointing            | Preserve recoverable state             |
| Circuit Breaker          | Avoid repeatedly failing services      |
| Approval Gate            | Control consequential mutations        |
| Completion Criteria      | Define objective success               |

### Suggested stop conditions

Stop successfully when acceptance criteria are satisfied and the required verification is complete.

Stop as blocked when a material dependency, permission, or required input is unavailable.

Stop with partial findings when the budget is exhausted, clearly identifying what remains unverified.

Avoid arbitrary tool-call counts as a substitute for assessing progress. Budgets provide safety boundaries; evidence determines whether more work is useful.

## 20. Agentic Security & Trust Boundaries

| Risk                       | Mitigation                                            |
| -------------------------- | ----------------------------------------------------- |
| Prompt Injection           | Treat retrieved content and tool outputs as untrusted |
| Tool Output Injection      | Separate data from executable instructions            |
| Excessive Agency           | Grant minimum required permissions                    |
| Data Exfiltration          | Enforce destination and data-access controls          |
| Credential Exposure        | Keep secrets outside model-visible context            |
| Cross-Tenant Leakage       | Tenant-scoped retrieval and caches                    |
| Confused Deputy            | Bind authorization to the requesting principal        |
| Unsafe Code Execution      | Sandbox, resource limits, approval                    |
| Malicious Skill            | Validate origin, contents and executable resources    |
| Tool Argument Manipulation | Server-side schema and authorization                  |
| Destructive Action         | Require explicit approval where appropriate           |
| Replay Attack              | Nonces, idempotency and authentication                |
| Untrusted Web Content      | Never promote webpage instructions to system policy   |
| Memory Poisoning           | Validate before writing durable memory                |

A connected MCP server or installed skill should not automatically be considered authoritative over the agent's operating rules.

## 21. Agent Evaluation & Observability

| Metric                       | What it measures                          |
| ---------------------------- | ----------------------------------------- |
| Task Success Rate            | Completed tasks satisfying criteria       |
| Verified Success Rate        | Success supported by external checks      |
| Hallucination Rate           | Unsupported or false claims               |
| Tool Selection Accuracy      | Correct capability choice                 |
| Unnecessary Tool Call Rate   | Avoidable tool activity                   |
| Retrieval Precision / Recall | Evidence quality and coverage             |
| Context Utilization          | Relevant evidence per context cost        |
| Cache Hit Rate               | Reuse of eligible cached context          |
| Tokens per Successful Task   | Efficiency adjusted for quality           |
| Cost per Verified Task       | Economic efficiency                       |
| p50 / p95 / p99 Latency      | Typical and tail response time            |
| Compaction Fidelity          | Preserved essential state                 |
| Continuation Success         | Correctness after resumption              |
| Delegation Overhead          | Cost and latency of subagents             |
| Retry Amplification          | Repeated execution caused by failures     |
| False Completion Rate        | Success claims without satisfaction       |
| Security Violation Rate      | Unauthorized or unsafe actions            |
| Regression Rate              | Previously successful tasks that now fail |

### Core optimization objective

\\[ \text{Agent Efficiency} = \frac{\text{Verified Successful Outcomes}} {\text{Cost, Time and Resource Usage}} \\]

Treat this as a design objective rather than a literal single-number metric. Track correctness and safety separately so cost optimization cannot hide quality regressions.

## 22. Agentic Architecture Selector

## Problem → Agentic Pattern

17 patterns

Simple, well-defined task

&#x20;Direct model call

Avoid orchestration overhead

Task requires one external fact

&#x20;Single targeted tool call

Minimize retrieval and latency

Complex task with tools

&#x20;Bounded agent loop

Observe and adapt based on results

Long-running task

&#x20;Checkpoint + compaction

Keep critical state across context limits

Many tools available

&#x20;Tool discovery + routing

Avoid loading irrelevant schemas

Large repository

&#x20;Hierarchical search + selective reads

Reduce context pollution

Large paginated API

&#x20;Cursor pagination + evidence stopping

Avoid fetching unnecessary pages

Stable repeated prompts

&#x20;Prompt caching

Reuse eligible prefix state

Repeated deterministic calculations

&#x20;Computation cache

Avoid duplicate processing

Persistent user/project knowledge

&#x20;Validated external memory

Retain useful information across sessions

Easy and difficult mixed workloads

&#x20;Model cascade

Route by complexity and cost

Independent complex subtasks

&#x20;Parallel specialist subagents

Isolate context and accelerate execution

High-stakes output

&#x20;Evidence + external verifier

Require stronger correctness guarantees

Frequent hallucinations

&#x20;Claim grounding + abstention

Prevent unsupported assertions

Frequent unproductive tool calls

&#x20;Progress-based stopping

Terminate or change strategy

Patchy coding changes

&#x20;Reproduce → root cause → test

Prioritize coherent fixes

Too much output

&#x20;Structured return contract

Return only decision-relevant information

## 23. Reference Implementation: Agent Control Loop

This is framework-independent pseudocode showing how the parts fit together.

```
def run_agent(request, services, limits):    task = define_task(        request,        success_criteria=True    )    state = load_checkpoint(task.id)    budget = Budget(limits)    while budget.can_continue():        # Build only the context needed now.        context = select_context(            task=task,            state=state,            token_budget=budget.context_allowance()        )        # Select model based on task complexity.        model = route_model(task, context)        decision = model.decide(            task=task,            context=context,            available_tools=discover_relevant_tools(task)        )        if decision.kind == "finish":            verdict = verify_completion(                task, state, decision            )            if verdict.passed:                return build_result(                    verdict=verdict,                    state=state                )
```

This is a reference design rather than an executable SDK implementation. A production version also needs durable action IDs, protection against repeated side effects after crashes, input/output validation, cancellation, tracing, and approval policies.

## 24. Agentic Developer Review Checklist

## Production Agent Audit

0 / 30

Context & Attention

Load only task-relevant context

Use precise retrieval before broad reads

Reserve context/output headroom

Preserve authoritative instructions and constraints

Track evidence provenance and source revisions

Caching & Memory

Maintain stable cacheable prefixes

Separate cache from durable memory

Version and expire cached data

Isolate users and tenants

Preserve decisions and unresolved work during compaction

Tools, MCP & Skills

Use descriptive non-overlapping tools

Discover skills progressively

Validate schemas and permissions

Use pagination, filters and range reads

Handle truncation, errors, cancellation and timeouts

Model & Agent Routing

Use simple execution by default

Escalate model capability only when justified

Delegate bounded independent tasks

Define structured subagent return contracts

Bound recursive delegation and concurrency

Correctness & Efficiency

Distinguish confirmed, inferred and unknown claims

Verify actual outcomes with tests or observations

Avoid redundant calls and repeated investigations

Stop when evidence and acceptance criteria are sufficient

Return concise findings and remaining risks

Security & Operations

Treat retrieved data and skills as untrusted

Enforce least-privilege execution

Protect destructive actions with approval

Persist safe checkpoints and action IDs

Measure task success, latency, cost and regressions

Reset Copy checklist

## 25. The Most Important 15 Rules

1. Context is a working set, not a data dump.
2. Retrieve on demand. Keep references to large content rather than repeatedly loading it.
3. Optimize for verified outcomes per unit cost, not for token count alone.
4. Preserve stable prefixes to benefit from prompt caching when supported.
5. Separate durable memory from transient context and caches.
6. Compact carefully. Preserve constraints, evidence, decisions, and unfinished work.
7. Let the agent choose what evidence it needs, within bounded resources.
8. Use pagination intelligently. Never mistake the first page for the complete dataset.
9. Design tools for agent decisions, not merely as thin wrappers around large APIs.
10. Trigger skills selectively, using clear descriptions and progressive disclosure.
11. Use a stronger model when necessary, not automatically for every request.
12. Delegate independent work, not trivial actions or tightly coupled decisions.
13. Require real verification. Plausible model output is not proof.
14. Avoid patch-first engineering. Understand, reproduce, fix, test and inspect impact.
15. Know when to stop. Return results with supporting evidence and clearly identify unresolved issues.

## Essential References

These are particularly valuable primary sources for implementing and refining agentic systems:

- [Anthropic — Effective Context Engineering](https://www.anthropic.com/research/effective-context-engineering-for-ai-agents?utm_source=chatgpt.com)
- [Anthropic — Writing Effective Tools](https://www.anthropic.com/research/writing-tools-for-agents?utm_source=chatgpt.com)
- [OpenAI — Prompt Caching](https://developers.openai.com/api/docs/guides/prompt-caching?utm_source=chatgpt.com)
- [OpenAI — Compaction](https://developers.openai.com/api/docs/guides/compaction?utm_source=chatgpt.com)
- [Model Context Protocol Specification](https://modelcontextprotocol.io/specification?utm_source=chatgpt.com)
- [Agent Skills Specification](https://agentskills.io/specification?utm_source=chatgpt.com)

The overall architectural goal is to create a bounded, evidence-driven, context-efficient agent that can discover relevant capabilities, retrieve the minimum sufficient information, perform coherent actions, recover its state, verify the result, and terminate reliably.

This is a better foundation for a production developer agent than either an enormous prompt or an unconstrained loop of model and tool calls.



# Agentic Engineering — Advanced Design Principles

Attention-First · Script-First · Reference-First · Paginated Tools · Self-Correcting Errors · Bounded Loops · Evidence-Driven Execution

Your seven principles can form a coherent architecture for efficient developer agents. The underlying idea is:

Use deterministic computation for mechanical work, use the LLM for judgment, and keep the context focused on information that changes the next decision.

This yields seven design rules:

| # | Principle                     | Implementation                                                                                |
| - | ----------------------------- | --------------------------------------------------------------------------------------------- |
| 1 | Attention is everything       | Optimize information relevance, not context utilization                                       |
| 2 | Reference-first output        | Return summaries and resource handles before raw content                                      |
| 3 | Pagination by default         | Bound responses and retrieve further pages only as needed                                     |
| 4 | Actionable errors             | Return structured failure causes and recovery hints                                           |
| 5 | Bounded, progress-aware loops | Detect stalls, duplicates, cycles and exhaustion                                              |
| 6 | Reason-aware tool calls       | Declare the purpose, expected evidence and decision being supported                           |
| 7 | Script-first, agent-second    | Use deterministic scripts for reproducible operations; reserve agents for ambiguous decisions |

## 1. Attention-First Context Engineering

The most important question is not how much context can the model fit? It is which information deserves the model's attention right now?

An enormous context window is not necessarily an advantage when most content is irrelevant, redundant, outdated or contradictory.

### Attention hierarchy

Critical

Highest priority

Task objective, acceptance criteria, safety constraints

Active



Current plan, relevant code, verified findings

Supporting



Related files, dependencies, previous decisions

Available

Load on demand

Large logs, full documents, broad search results

Conceptual attention priorities, not measured transformer attention weights.

### Practical rules

- Keep the current goal and constraints salient.
- Prefer exact symbols, relevant line ranges and concise evidence over entire files.
- Remove duplicated and obsolete material.
- Maintain a short active-state summary.
- Keep large artifacts accessible through references.
- Retrieve details when they can alter a decision.
- Avoid repeatedly injecting unchanged instructions or tool results.

Important distinction: Transformer attention is a model mechanism. Context prioritization is an engineering strategy that influences the information presented to that mechanism; it does not directly control attention weights.

## 2. Reference-First Tool Responses

Tool results should not automatically become model context.

Instead, large results should remain in external storage, while the model receives a compact description and a way to retrieve more.

Tool executes analysis

Repository scan · Large build · Database query

Artifact / Result Store

Complete output retained under a stable reference

Compact Agent Response

Status · Summary · Relevant findings · Resource ID · Next actions

Selective Follow-Up

Read relevant range, symbol, page or artifact only if needed

### Example response contract

```

{
  "status": "success",
  "summary": "Scanned 1,240 source files",
  "findings": {
    "dependencyCycles": 3,
    "criticalViolations": 2
  },
  "result": {
    "resourceId": "scan:7f9a",
    "uri": "artifact://scans/7f9a",
    "totalItems": 1240,
    "returnedItems": 0
  },
  "recommendedNext": [
    {
      "action": "get_findings",
      "filter": "severity=critical"
    }
  ]
}

```

A resource reference must be resolvable, scoped to the correct identity, and retained long enough for the intended workflow. A file path alone is not evidence that the file exists or is still valid.

Default strategy: Return a summary and reference; inline raw data only when small enough and necessary.

## 3. Pagination as a Tool Contract

Pagination should be designed into search, listing, scanning and retrieval tools rather than added after oversized responses become a problem.

```

type PageRequest = {
  query?: string;
  cursor?: string;
  limit?: number;
  fields?: string[];
  reason: string;
};

type PageResult<T> = {
  items: T[];
  nextCursor?: string;
  hasMore: boolean;
  totalCount?: number;
  sourceRevision?: string;
  truncated: boolean;
};

```

### Pagination rules

1. Use bounded default page sizes and server-enforced limits.
2. Prefer opaque cursor-based pagination over offsets for frequently changing datasets.
3. Support filtering, projections and stable sorting.
4. Preserve source revision or snapshot identity when consistency matters.
5. Allow the agent to request a specific range, file, record or symbol.
6. Report whether results were truncated or additional pages exist.
7. Continue pagination only while a material information gap remains.

Not all pagination should be mandatory. For tiny results, returning the complete response is simpler and more efficient.

## 4. Actionable Errors & Recovery Hints

A tool should not merely report that it failed. It should help the agent determine whether to retry, change the request, inspect another resource, or stop.

### Error taxonomy

| Error type               | Agent response                                    |
| ------------------------ | ------------------------------------------------- |
| `INVALID_ARGUMENT`       | Correct parameters using schema                   |
| `RESOURCE_NOT_FOUND`     | Check identifier or search alternatives           |
| `AMBIGUOUS_MATCH`        | Narrow search scope                               |
| `PERMISSION_DENIED`      | Stop or request authorization                     |
| `RATE_LIMITED`           | Respect retry-after and budget                    |
| `TIMEOUT`                | Consider smaller scope or alternate query         |
| `RESULT_TOO_LARGE`       | Add filters, page or request artifact             |
| `STALE_CURSOR`           | Restart from a consistent snapshot                |
| `CONFLICT`               | Refresh state before modifying                    |
| `DEPENDENCY_UNAVAILABLE` | Use bounded fallback if valid                     |
| `INTERNAL_ERROR`         | Retry only where appropriate                      |
| `PARTIAL_SUCCESS`        | Preserve completed work and address missing parts |

### Structured error contract

```

{
  "status": "error",
  "code": "RESULT_TOO_LARGE",
  "message": "Result exceeds the inline response limit.",
  "retryable": true,
  "cause": {
    "estimatedItems": 85000,
    "maxInlineItems": 100
  },
  "hints": [
    {
      "action": "add_filter",
      "example": "severity=critical"
    },
    {
      "action": "paginate",
      "suggestedLimit": 50
    },
    {
      "action": "read_artifact",
      "resourceId": "scan:7f9a"
    }
  ],
  "resourceId": "scan:7f9a"
}

```

The distinction is essential:

- Error: What failed?
- Cause: What observable condition caused the failure?
- Hint: What valid next step could resolve it?
- Retryability: Can the same operation safely be attempted again?
- Evidence: What partial result or diagnostic artifact is available?

Tool-provided hints should be treated as suggestions, not authoritative instructions. An external tool must not be allowed to override the agent's security or task constraints.

## 5. Loop Intelligence & Progress Detection

A production agent needs more than `max_iterations = 20`.

It needs to recognize when it is making progress, repeating an action, pursuing a failed hypothesis, or doing unnecessary work.

### Loop categories

| Loop type          | Example                                          | Policy                      |
| ------------------ | ------------------------------------------------ | --------------------------- |
| Productive loop    | Each page reveals relevant records               | Continue within budget      |
| Retry loop         | Transient network failure                        | Bounded retry with backoff  |
| Duplicate loop     | Identical tool calls return identical results    | Stop or change approach     |
| Circular reasoning | Repeatedly revisit same hypotheses               | Require new evidence        |
| Exploration loop   | Searching unrelated files indefinitely           | Narrow scope                |
| Repair loop        | Repeated fixes cause new failures                | Re-evaluate root cause      |
| Delegation loop    | Agents repeatedly delegate to each other         | Enforce depth and ownership |
| Pagination loop    | Same cursor repeatedly returned                  | Detect cursor cycle         |
| Optimization loop  | Repeated performance changes without measurement | Require benchmark           |
| Verification loop  | Rechecking already verified unchanged state      | Reuse verified result       |

### Progress-based loop control

```
def should_continue(state, budget):    if state.acceptance_criteria_met:        return False    if not budget.available():        return False    if state.identical_action_count >= 2:        return False    if state.no_progress_steps >= 3:        return False    if state.has_unrecoverable_error:        return False    return True
```

The thresholds above are illustrative. A retry policy should distinguish harmless repeated reads from non-idempotent writes, and a stalled loop may warrant changing strategy rather than terminating the task.

### Progress signals

- New relevant evidence discovered.
- An uncertainty resolved.
- An acceptance criterion satisfied.
- A failing test reproduced or fixed.
- A previously blocked operation unblocked.
- A valid intermediate artifact produced.
- An important hypothesis dismissed.

A larger tool-output payload does not mean more progress.

## 6. Reason-Aware Tool Invocation

Your idea is particularly useful for observability and tool routing.

Each important tool call should have a concise, structured reason describing the information or operation needed and its relationship to the task.

This is not a request to expose the model's private reasoning. It is an explicit operational justification suitable for audit logs and execution tracing.

### Recommended tool invocation

```

{
  "tool": "repository.search",
  "arguments": {
    "query": "createOrder",
    "fileTypes": ["ts"],
    "limit": 20
  },
  "intent": {
    "reason": "Locate the order creation entry point.",
    "evidenceNeeded": "Definition and callers",
    "decisionSupported": "Identify the owner of idempotency logic",
    "expectedOutput": "Symbol locations, not full files"
  },
  "execution": {
    "timeoutMs": 5000,
    "maxOutputTokens": 1200
  }
}

```

### What should be logged?

| Field             | Purpose                              |
| ----------------- | ------------------------------------ |
| `taskId`          | Correlate calls to a task            |
| `stepId`          | Identify execution step              |
| `toolName`        | Identify invoked capability          |
| `reason`          | Explain the operational purpose      |
| `evidenceNeeded`  | Define expected information          |
| `inputSummary`    | Record sanitized request             |
| `outputReference` | Identify retained result             |
| `durationMs`      | Measure execution                    |
| `tokensUsed`      | Measure cost                         |
| `resultStatus`    | Track success or failure             |
| `nextDecision`    | Record resulting workflow transition |

### Critical safeguards

Do not trust a model-generated reason as proof that a call was authorized or useful. Enforce authorization independently.

Do not require the model to write a long rationale for every trivial operation. Short machine-readable intent labels are sufficient in many cases, and some calls can inherit the purpose of their parent task.

This creates a useful execution decision log without filling the conversation with explanations.

## 7. Script-First, Agent-Second

This is one of the highest-value architectural rules for a developer agent.

If an operation can be expressed as a reliable, deterministic program, use that program. Let the agent determine when to run it, what inputs it requires, and what conclusions can be drawn from its output.

### Division of responsibilities

| Task                                      | Preferred executor                      |
| ----------------------------------------- | --------------------------------------- |
| Count files, symbols or dependencies      | Script                                  |
| Parse JSON, YAML or source AST            | Parser/script                           |
| Calculate graph metrics                   | Script                                  |
| Detect dependency cycles                  | Graph algorithm                         |
| Find exact symbol references              | LSP/indexer                             |
| Compare two schemas                       | Schema diff tool                        |
| Check formatting                          | Formatter                               |
| Find code defects                         | Static analyzer                         |
| Run unit and integration tests            | Test runner                             |
| Calculate performance statistics          | Script                                  |
| Validate generated output                 | Schema validator                        |
| Infer which architectural concern matters | Agent                                   |
| Form competing debugging hypotheses       | Agent                                   |
| Select next investigation                 | Agent                                   |
| Evaluate migration trade-offs             | Agent                                   |
| Explain findings to an engineer           | Agent                                   |
| Create a missing analysis script          | Agent, then execute and validate script |

### Hybrid architecture

Agent: Understand the goal

Determine what must be measured, checked or changed

Select Existing Tool or Generate Script

Prefer existing tested automation; generate only for uncovered needs

Deterministic Execution

Parser · Compiler · Test runner · Graph algorithm · Query

Structured Result + Artifact

Counts · Findings · Errors · Provenance · References

Agent: Interpret, Decide, Verify

Apply engineering judgment and select the next action

### Why scripts are better for mechanical tasks

- Reproducible and testable behavior.
- Exact arithmetic and enumeration.
- Lower inference cost for repeated work.
- Easier regression testing.
- Transparent execution and failure diagnostics.
- Reusable results across future agent runs.
- Less risk of fabricated counts or skipped cases.

### When scripts are insufficient

Scripts do not automatically understand ambiguous business intent, choose between conflicting goals, or establish that an architectural design is appropriate.

Generated scripts may also contain bugs. For destructive operations, validate inputs, constrain permissions, inspect effects, and require appropriate approval before execution.

Refined rule: Existing tested script first; new script if worthwhile; agentic reasoning for decisions that cannot reliably be reduced to deterministic processing.

## 8. Unified Tool Contract

An effective tool interface combines your seven principles without forcing every tool to implement unnecessary complexity.

```

type ToolIntent = {
  reason: string;
  evidenceNeeded?: string;
  decisionSupported?: string;
};

type ToolRequest<T> = {
  input: T;
  intent: ToolIntent;

  options?: {
    cursor?: string;
    limit?: number;
    maxOutputTokens?: number;
    timeoutMs?: number;
    returnMode?: "inline" | "reference" | "auto";
  };
};

type ToolError = {
  code: string;
  message: string;
  retryable: boolean;
  hints?: Array<{
    action: string;
    parameters?: Record<string, unknown>;
  }>;
};

type ToolResponse<T> = {
  status: "success" | "partial" | "error";

  summary: string;
  data?: T;

  artifact?: {
    resourceId: string;
    uri: string;
    contentType?: string;
    revision?: string;
  };

  pagination?: {
    nextCursor?: string;
    hasMore: boolean;
    totalCount?: number;
  };

  error?: ToolError;

  execution: {
    durationMs: number;
    truncated: boolean;
  };
};

```

The contract separates five responsibilities: intent, execution, concise results, recoverable errors, and optional deferred retrieval.

For MCP, these fields can be implemented as application-level schemas or metadata as appropriate. They are not all required fields in the MCP specification.

## 9. Full Agent Execution Policy

```

agent_policy:
  objective: verified_success

  attention:
    prefer_relevant_context: true
    load_full_resources_only_when_needed: true
    preserve_source_references: true
    deduplicate_context: true

  execution:
    prefer_existing_scripts: true
    generate_scripts_when_justified: true
    validate_generated_scripts: true
    require_evidence_for_claims: true

  tools:
    discover_on_demand: true
    require_operational_intent: true
    prefer_structured_results: true
    prefer_references_for_large_outputs: true
    support_bounded_pagination: true
    include_actionable_errors: true

  loops:
    track_progress: true
    detect_repeated_actions: true
    detect_cursor_cycles: true
    use_bounded_retries: true
    stop_on_verified_completion: true
    stop_or_replan_on_stall: true

  context:
    compact_when_needed: true
    preserve_decisions_and_evidence: true
    preserve_unfinished_work: true
    rehydrate_on_demand: true

  output:
    concise_by_default: true
    distinguish_facts_from_inferences: true
    report_verification_status: true
    report_material_uncertainty: true
    omit_unnecessary_process_narration: true

```

## 10. Additional Principles to Add

The following complement your seven rules:

| Principle                   | Engineering implication                                               |
| --------------------------- | --------------------------------------------------------------------- |
| Evidence over confidence    | Trust executable checks and authoritative sources over fluent answers |
| Question-driven retrieval   | Every significant read should resolve an identifiable information gap |
| Result reuse                | Do not recompute verified unchanged state                             |
| Content-addressed artifacts | Reuse results based on complete content identity                      |
| Deterministic replay        | Record enough execution context to reproduce behavior                 |
| Reversible actions          | Prefer changes that can be safely rolled back                         |
| Idempotent execution        | Prevent retries from duplicating effects                              |
| Incremental computation     | Recalculate only what changed                                         |
| Information gain            | Prefer operations likely to change the next decision                  |
| Evidence completeness       | Distinguish partial coverage from exhaustive verification             |
| Separation of concerns      | Keep planning, execution, verification and presentation distinct      |
| State externalization       | Store durable state outside the active context                        |
| Budget-aware routing        | Choose models and tools based on complexity and costs                 |
| Outcome-based evaluation    | Measure verified task completion rather than agent activity           |
| Tool-contract testing       | Test pagination, errors, limits, revisions and permissions            |

## 11. The Agentic Developer's Golden Rules

1

Attention first

Give the model the information necessary for its next decision, not everything available.

2

Reference first

Store large outputs externally and return compact, resolvable handles.

3

Paginate by design

Bound tool results, expose cursors and fetch more only when needed.

4

Errors should guide recovery

Return structured causes, retryability and safe alternatives.

5

Loops require progress

Monitor evidence gained and stop repetitive, nonproductive execution.

6

Every meaningful tool call has a purpose

Record a concise operational reason and the evidence required.

7

Script first, agent second

Automate exact work deterministically and use the LLM for judgment.

8

Verification before completion

Never claim an outcome that has not been checked.

9

Compact without forgetting

Preserve verified facts, constraints, decisions, references and pending work.

10

Stop when sufficient

Avoid more tools, tokens or investigation when the task is already resolved.

The resulting architecture has a clear separation of responsibility:

The agent decides. Scripts compute. Tools execute. Artifacts preserve. Evidence verifies. Context focuses. Policies constrain.

This is the foundation of an efficient, auditable and reliable developer agent—one that spends its context and reasoning budget on engineering decisions rather than mechanical work.

# Agentic Engineering — Part IV: Agent Communication & Typed Contracts

A2A · MCP · Zod · JSON Schema · Agent Messages · Task Handoffs · Protocol Design · Streaming · Reliability

Core principle: Agents should communicate through explicit, validated contracts—not unstructured conversations or shared reasoning transcripts.

The communication layer should let agents discover capabilities, delegate work, exchange concise evidence, transfer artifacts, recover from errors, and independently verify completion.

## 1. Agent Communication Protocols

| Protocol / Technology        | Primary responsibility                               | Best use                            |
| ---------------------------- | ---------------------------------------------------- | ----------------------------------- |
| A2A (Agent2Agent)            | Agent discovery, delegation and task lifecycle       | Independent cooperating agents      |
| MCP (Model Context Protocol) | Expose tools, resources and prompts                  | Connect agents to capabilities      |
| JSON Schema                  | Language-independent data contract                   | Typed message boundaries            |
| Zod                          | TypeScript runtime validation and schema definitions | Validating agent inputs and outputs |
| JSON-RPC                     | Structured remote procedure calls                    | Method-based communication          |
| gRPC / Protobuf              | Typed high-performance RPC                           | Internal services                   |
| HTTP REST                    | Resource-oriented communication                      | APIs and task services              |
| SSE                          | Server-to-client event streaming                     | Progress updates                    |
| WebSocket                    | Bidirectional persistent messaging                   | Interactive agents                  |
| Message Broker               | Asynchronous reliable delivery                       | Durable agent workflows             |
| CloudEvents                  | Standard event envelope                              | Event-driven integration            |
| OpenTelemetry                | Distributed traces and correlation                   | Agent execution observability       |

### A2A versus MCP

Supervisor / Orchestrator Agent

Routes work, validates results, owns overall completion

A2A

Specialist Agent

Own task lifecycle, return artifacts

MCP

MCP Tool Server

Search, read, execute, retrieve resources

Shared Contract & Evidence Layer

Schema validation · Authorization · Resource references · Tracing

A2A defines independently managed agent tasks, messages, artifacts and capability discovery. MCP connects an agent application to externally exposed tools and resources. They are complementary, not interchangeable. The current published A2A specification identifies version 1.0.0 as its latest release.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://a2a-protocol.org\&sz=32)

A2A Protocol



## 2. Agent Communication Patterns

| Pattern                 | How it works                                   | Use case                            |
| ----------------------- | ---------------------------------------------- | ----------------------------------- |
| Request–Response        | One request, one result                        | Small bounded tasks                 |
| Delegation–Return       | Parent delegates, worker returns result        | Specialist agents                   |
| Supervisor–Worker       | Supervisor assigns work and aggregates results | Coordinated workflows               |
| Publish–Subscribe       | Agents receive subscribed events               | Loose coupling                      |
| Request–Reply Messaging | Correlated asynchronous messages               | Broker-based services               |
| Fan-Out / Fan-In        | Parallel workers, one aggregator               | Independent analysis                |
| Pipeline                | Each agent processes a stage                   | Sequential transformations          |
| Blackboard              | Agents contribute to shared state              | Collaborative reasoning             |
| Actor Model             | Isolated agents exchange messages              | Stateful concurrent systems         |
| Negotiation             | Agents exchange proposals and constraints      | Resource or task allocation         |
| Handoff                 | One agent transfers ownership                  | Specialized workflows               |
| Orchestration           | Central coordinator controls sequence          | Reliable business processes         |
| Choreography            | Agents react to events                         | Decentralized workflows             |
| Consensus / Voting      | Multiple candidates combined or compared       | Selected high-uncertainty decisions |

For most developer-agent systems, start with one supervisor and bounded specialist workers, not an unrestricted network of agents communicating with one another.

## 3. A2A: Important Building Blocks

| A2A concept       | Function                                                         |
| ----------------- | ---------------------------------------------------------------- |
| Agent Card        | Advertise identity, skills, interfaces and security requirements |
| Agent Skill       | Advertised capability of an A2A agent                            |
| Message           | Exchange instructions, clarification and status                  |
| Task              | Represent trackable stateful work                                |
| Task State        | Represent progress, interruption or completion                   |
| Part              | Carry text, files or structured data                             |
| Artifact          | Deliver task outputs                                             |
| Context ID        | Group related interactions                                       |
| Task ID           | Identify a specific unit of work                                 |
| Streaming         | Deliver incremental task updates                                 |
| Push Notification | Report asynchronous progress                                     |
| Agent Extension   | Add negotiated functionality                                     |

One useful distinction: an A2A Agent Skill is advertised capability metadata; it is not automatically the same thing as an executable Agent Skills `SKILL.md` package.

A2A also recommends separating ordinary communication from task outputs: messages communicate, while artifacts deliver results.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://a2a-protocol.org\&sz=32)

A2A Protocol+1



## 4. Typed Communication: Zod + TypeScript

The core technique is schema-first messaging with validation at every agent boundary.

```

import { z } from "zod";

// A minimal, application-specific agent task envelope.
// This is NOT the wire format of A2A itself.

const AgentRequest = z.strictObject({
  version: z.literal("1.0"),

  taskId: z.string().min(1),
  correlationId: z.string().min(1),

  from: z.string().min(1),
  to: z.string().min(1),

  intent: z.strictObject({
    goal: z.string().min(1),
    reason: z.string().min(1),
    expectedOutput: z.string().min(1),
  }),

  limits: z.strictObject({
    maxSteps: z.number().int().positive(),
    maxOutputTokens: z.number().int().positive(),
  }),

  contextRefs: z.array(z.string()),
  input: z.record(z.string(), z.unknown()),
});

type AgentRequest = z.infer<typeof AgentRequest>;

function acceptRequest(payload: unknown): AgentRequest {
  return AgentRequest.parse(payload);
}

```

Why runtime validation matters: TypeScript types disappear at runtime. A remote agent, tool, model, file or MCP server can still return invalid data. Zod validates actual values.

For reliable TypeScript systems, use Zod as a runtime boundary validator, and export JSON Schema when interoperability requires it.

## 5. Zod: Structured Agent Messages

Use a discriminated union instead of a single loosely defined object containing many optional fields.

```

import { z } from "zod";

const EvidenceRef = z.strictObject({
  uri: z.string().min(1),
  revision: z.string().optional(),
});

const Result = z.discriminatedUnion("status", [
  z.strictObject({
    status: z.literal("completed"),
    summary: z.string(),
    evidence: z.array(EvidenceRef),
  }),

  z.strictObject({
    status: z.literal("blocked"),
    reason: z.string(),
    retryable: z.boolean(),
    suggestedAction: z.string().optional(),
  }),

  z.strictObject({
    status: z.literal("failed"),
    code: z.string(),
    message: z.string(),
  }),
]);

type AgentResult = z.infer<typeof Result>;

function receiveAgentResult(raw: unknown): AgentResult {
  const parsed = Result.safeParse(raw);

  if (!parsed.success) {
    throw new Error(
      `Invalid agent result: ${z.prettifyError(parsed.error)}`
    );
  }

  return parsed.data;
}

```

This creates explicit contracts for successful, blocked and failed work. TypeScript can narrow the result based on `status`, and runtime validation rejects malformed responses.

### Zod techniques worth mastering

| Technique                | Purpose                                      |
| ------------------------ | -------------------------------------------- |
| `z.strictObject()`       | Reject unexpected object fields              |
| `z.discriminatedUnion()` | Define typed message variants                |
| `.safeParse()`           | Validate without throwing                    |
| `.refine()`              | Enforce custom invariants                    |
| `.superRefine()`         | Report richer cross-field issues             |
| `z.infer`                | Derive TypeScript types                      |
| `z.toJSONSchema()`       | Export interoperability contracts            |
| `.parseAsync()`          | Validate with asynchronous refinements       |
| `.optional()`            | Define optional data                         |
| `.default()`             | Supply controlled defaults                   |
| `.transform()`           | Normalize data after validation              |
| `.brand()`               | Distinguish otherwise identical domain types |

Zod 4 supports native JSON Schema generation, although some Zod types and transformations cannot be represented faithfully as JSON Schema.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://zod.dev\&sz=32)

Zod



## 6. One Schema, Multiple Boundaries

Canonical Contract

Zod / JSON Schema / Protobuf

LLM outputs

Constrain generated structure

Tool inputs

Validate parameters

Tool outputs

Validate observed data

Agent handoffs

Validate delegated results

MCP schemas

Describe compatible tool contracts

API documentation

Publish typed interfaces

A canonical contract reduces schema drift, but do not assume that every target supports the same schema dialect. OpenAI Structured Outputs, for example, supports a subset of JSON Schema when strict validation is requested.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://developers.openai.com\&sz=32)

OpenAI API



Use a provider-compatible exported schema at the model boundary, then independently validate the returned value and any business invariants in application code.

## 7. Agent-to-Agent Message Envelope

For an internal agent runtime, standardize a message envelope that is distinct from the domain payload.

```

import { z } from "zod";

const Envelope = z.strictObject({
  schemaVersion: z.literal("1.0"),

  messageId: z.uuid(),
  taskId: z.string(),
  correlationId: z.string(),
  causationId: z.string().optional(),

  sender: z.string(),
  recipient: z.string(),

  kind: z.enum([
    "command",
    "event",
    "query",
    "result",
    "error",
    "cancel"
  ]),

  createdAt: z.iso.datetime(),
  deadlineAt: z.iso.datetime().optional(),

  idempotencyKey: z.string().optional(),

  payloadSchema: z.string(),
  payload: z.unknown(),
});

type Envelope = z.infer<typeof Envelope>;

```

The envelope handles routing, tracing, lifecycle and compatibility. The `payloadSchema` identifies a separate versioned payload validator that must be applied after validating the envelope.

| Field                | Purpose                                           |
| -------------------- | ------------------------------------------------- |
| `messageId`          | Unique message identity                           |
| `taskId`             | Logical task identity                             |
| `correlationId`      | Associate related messages                        |
| `causationId`        | Identify the event that caused this message       |
| `sender / recipient` | Route and audit messages                          |
| `kind`               | Distinguish commands, events, queries and results |
| `schemaVersion`      | Version the envelope                              |
| `payloadSchema`      | Select the domain validator                       |
| `deadlineAt`         | Bound useful execution time                       |
| `idempotencyKey`     | Deduplicate logical operations                    |

Additional production metadata often includes trace context, tenant scope, content type, priority and artifact references. Trusted identity and authorization must still come from authenticated transport or verified credentials, not merely the sender field.

## 8. Task Lifecycle & Ownership

Agent communication needs explicit ownership and state, not just messages being passed back and forth.

### Simplified task lifecycle

Submitted

Working

Completed

Deliver result

Failed

Return error

Canceled

Stop execution

This is a simplified visualization. A2A also defines interrupted states such as input-required and authorization-required, and a rejected terminal state.

Design rules:

- A task has one accountable owner at a time.
- The parent tracks child task IDs and cancellation.
- Children return independently verifiable artifacts.
- Terminal tasks cannot silently restart.
- A timeout is not proof that remote execution stopped.
- A canceled task must not be assumed to have rolled back its side effects.
- A task's successful transport response is not proof that the business operation succeeded.

A2A explicitly models task lifecycle, including long-running tasks and interrupted states.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://a2a-protocol.org\&sz=32)

A2A Protocol



## 9. Structured Handoff Between Agents

A handoff should transfer enough context to complete the delegated task without copying the entire conversation.

```

{
  "taskId": "dependency-audit-42",
  "assignedTo": "architecture-agent",
  "goal": "Identify forbidden dependencies",
  "scope": {
    "module": "packages/payments",
    "revision": "commit-abc123"
  },
  "acceptanceCriteria": [
    "Enumerate confirmed violations",
    "Include exact source locations",
    "Separate inferred findings"
  ],
  "contextRefs": [
    "artifact://repo/dependency-graph"
  ],
  "constraints": {
    "readOnly": true,
    "maxResults": 100
  },
  "expectedResultSchema": "architecture-findings/v1"
}

```

The supervisor should receive a concise result containing status, findings, evidence, artifacts, uncertainty and next actions—not the worker's full reasoning transcript.

## 10. Communication Reliability & Delivery Semantics

| Concern                 | Recommended approach                                |
| ----------------------- | --------------------------------------------------- |
| Message duplication     | Stable idempotency keys                             |
| Lost messages           | Durable broker / outbox where required              |
| Out-of-order delivery   | Sequence numbers and version checks                 |
| Repeated commands       | Idempotent handlers                                 |
| Agent crash             | Durable checkpoints and resumable tasks             |
| Agent timeout           | Query task state before repeating side effects      |
| Partial completion      | Persist completed steps and remaining work          |
| Concurrent modification | Optimistic locking or transactional serialization   |
| Long-running work       | Task IDs, polling, streaming or callbacks           |
| Cancellation            | Explicit cancellation with acknowledgment           |
| Backpressure            | Bounded queues and admission control                |
| Dead-letter handling    | Quarantine failed messages with diagnostic metadata |
| Schema mismatch         | Reject, negotiate or migrate versions               |
| Duplicate results       | Deduplicate using task and artifact identity        |

Do not equate exactly-once messaging with exactly-once business effects. Prefer at-least-once delivery with idempotent processing and durable deduplication when appropriate.

## 11. Streaming vs. Final Artifacts

Streaming should communicate useful progress without flooding other agents with every intermediate token.

| Type                 | Recommended content                      |
| -------------------- | ---------------------------------------- |
| `task.started`       | Task accepted, ID assigned               |
| `task.progress`      | Meaningful stage transition              |
| `task.blocked`       | Missing input or permission              |
| `artifact.available` | Reference to produced artifact           |
| `task.completed`     | Final result and verification status     |
| `task.failed`        | Error, retryability and recovery options |
| `task.canceled`      | Confirmed cancellation state             |

Prefer semantic progress updates over streaming internal reasoning text. Where state updates are lossy, provide a way to query the authoritative task state.

In A2A, artifacts can be streamed incrementally, while task status remains queryable independently.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



## 12. Schema Versioning & Compatibility

| Change               | Compatibility consideration                          |
| -------------------- | ---------------------------------------------------- |
| Add optional field   | Usually backward-compatible for tolerant consumers   |
| Add required field   | May break existing producers                         |
| Rename field         | Typically breaking                                   |
| Remove field         | Breaks consumers relying on it                       |
| Change field type    | Usually breaking                                     |
| Add enum value       | Can break exhaustive consumers                       |
| Change field meaning | Semantic breaking change                             |
| Change units         | Breaking unless explicitly versioned                 |
| New message type     | Requires capability negotiation or tolerant handling |

A particularly useful pattern is a schema registry:

```
contracts/
  agent-envelope/
    v1.schema.json
  architecture-findings/
    v1.schema.json
    v2.schema.json
  task-result/
    v1.schema.json
```

Run compatibility checks in CI. Preserve fixtures representing older producers and consumers. Use explicit upcasters when reading historic persisted events.

## 13. Agent Capability Discovery & Routing

A supervisor should select agents by declared capabilities, not just their names.

| Capability metadata        | Why                             |
| -------------------------- | ------------------------------- |
| Agent ID                   | Stable identity                 |
| Supported skills           | Task matching                   |
| Input/output modalities    | Compatibility                   |
| Contract versions          | Schema negotiation              |
| Execution mode             | Sync, async or streaming        |
| Supported limits           | Payload and concurrency budgets |
| Authorization requirements | Access feasibility              |
| Data classification        | Privacy constraints             |
| Cost/latency profile       | Efficient selection             |
| Health status              | Availability                    |
| Version                    | Deployment compatibility        |

Route using capability fit → permission → contract compatibility → health → cost/latency.

Avoid dynamically selecting an untrusted external agent solely because its description claims to be capable.

## 14. Protocol Security

Agent-to-agent communication should be treated as communication between independently controlled services.

- Authenticate both callers and remote agent services.
- Authorize every action at the receiving boundary.
- Use transport encryption.
- Validate structured input before processing.
- Scope resource references by identity and tenant.
- Never pass credentials inside normal model-visible task context.
- Treat returned messages and artifact content as untrusted.
- Protect against recursive delegation and runaway cost.
- Validate callbacks and webhook authenticity.
- Separate transport identity, user identity and delegated authority.

A2A's specification describes agent discovery, authentication requirements, and authorization as separate concerns.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://a2a-protocol.org\&sz=32)

A2A Protocol



## 15. Agent Communication Testing

| Test                     | Validates                               |
| ------------------------ | --------------------------------------- |
| Contract test            | Message conforms to schema              |
| Consumer-driven contract | Producer meets consumer assumptions     |
| Compatibility test       | Old/new schema versions interoperate    |
| Property-based test      | Invariants hold across generated inputs |
| Fuzz test                | Malformed payload handling              |
| Duplicate-delivery test  | Idempotency                             |
| Reordering test          | Out-of-order behavior                   |
| Crash/restart test       | Durable recovery                        |
| Timeout test             | Correct deadline behavior               |
| Cancellation test        | Cancellation and side effects           |
| Authorization test       | Cross-agent permission enforcement      |
| Streaming test           | Update order and artifact assembly      |
| Load test                | Throughput, queueing and saturation     |
| Chaos test               | Partial failures and network partitions |

For Zod-based systems, validate messages on both ingress and egress, and test actual contract fixtures instead of relying only on TypeScript compilation.

## 16. Architecture Decision Guide

## Communication Pattern Selector

One agent needs to execute a capability

&#x20;MCP tool

Tool interface, structured inputs and outputs

Two independent agents must collaborate

&#x20;A2A

Discovery, tasks, messages and artifact exchange

Agents live in the same process

&#x20;Typed function calls / internal queue

Avoid unnecessary network protocol overhead

Need persistent asynchronous commands

&#x20;Message broker + durable task state

Decouple execution and support recovery

TypeScript contract validation

&#x20;Zod

Runtime parsing plus inferred types

Cross-language message exchange

&#x20;JSON Schema / Protobuf

Portable schemas and generated clients

Need asynchronous status updates

&#x20;SSE / A2A streaming

Stream progress without excessive polling

Need to return a large result

&#x20;Artifact reference

Keep agent context small

Need reliable repeated operations

&#x20;Idempotency + transactional records

Prevent repeated side effects

Schema changes across versions

&#x20;Versioned registry + compatibility tests

Prevent integration breakage

Many agents are available

&#x20;Capability registry + router

Choose based on declared contracts and permissions

## 17. Ten Golden Rules for Agent Communication

1. Schema first: Every message boundary needs a defined, validated contract.
2. A2A for agents, MCP for capabilities: Use each where its semantics are useful.
3. Separate messages from artifacts: Communicate intent and status; preserve deliverables independently.
4. Validate twice: Check outgoing and incoming data.
5. Reference large results: Do not copy full artifacts into every agent context.
6. Use correlation and causation IDs: Make distributed activity traceable.
7. Design for duplicates and failure: Retries must not duplicate logical effects.
8. Version contracts deliberately: Test compatibility before deployment.
9. Keep delegation bounded: Explicit task ownership, budgets, deadlines and completion criteria.
10. Trust verification, not declarations: An agent reporting success does not establish that the task succeeded.

### Primary references

- [A2A Protocol Specification](https://a2a-protocol.org/latest/specification/?utm_source=chatgpt.com)
- [Model Context Protocol](https://modelcontextprotocol.io/specification?utm_source=chatgpt.com)
- [Zod Documentation](https://zod.dev/?utm_source=chatgpt.com)
- [Zod — JSON Schema Conversion](https://zod.dev/json-schema?utm_source=chatgpt.com)
- [OpenAI — Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs?utm_source=chatgpt.com)

Final architecture principle: The LLM determines what to do. A typed contract defines what may be exchanged. The protocol determines how it is exchanged. The runtime controls when and whether it executes. Deterministic validators and tests determine whether the result is acceptable.