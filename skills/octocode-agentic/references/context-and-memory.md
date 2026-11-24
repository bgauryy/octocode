Load when deciding what enters the context window, diagnosing context-driven quality loss, using prompt caching, designing persistent memory, compacting a long-running session, or measuring token and cost savings.

# Context and memory

## Contents

- [Attention first](#attention-first)
- [Context failures](#context-failures)
- [Context budget](#context-budget)
- [Caching](#caching)
- [Prompt caching](#prompt-caching)
- [Memory](#memory)
- [Compaction](#compaction)
- [Cost](#cost)
- [Sources](#sources)

## Attention first

The question is not how much fits but which information deserves attention now. A large window is no advantage when most of it is irrelevant, redundant, stale or contradictory, and capacity does not guarantee every included fact is used correctly.

| Tier | Contents | Handling |
|---|---|---|
| Critical | Objective, acceptance criteria, safety constraints | Always salient |
| Active | Current plan, relevant code, verified findings | In context |
| Supporting | Related files, dependencies, earlier decisions | Brief or referenced |
| Available | Large logs, full documents, broad search results | Load on demand |

These are engineering priorities, not measured attention weights: context selection shapes what the model sees but does not control transformer attention.

- Keep the current goal and constraints salient; maintain a short active-state summary.
- Prefer exact symbols, relevant line ranges and concise evidence over whole files.
- Remove duplicated and obsolete material; do not reinject unchanged instructions or tool results.
- Keep large artifacts reachable through references; retrieve details when they can change a decision.

## Context failures

| Failure | Symptom | Prevention |
|---|---|---|
| Overflow | Request exceeds the context limit | Budget + compaction |
| Pollution | Irrelevant information dominates | Filter and rank |
| Context rot | Performance declines as context grows | Prune and refresh |
| Lost-in-the-middle | Important information neglected | Focused retrieval, salient structure |
| Stale context | Agent uses outdated information | Versions and freshness checks |
| Summary drift | Compaction changes facts | Structured, source-backed summaries |
| Instruction collision | Conflicting directions | Explicit instruction precedence |
| Tool-output flooding | Huge results obscure the task | Pagination and projections |
| Cross-task contamination | Unrelated task details influence output | Isolated contexts |
| Evidence laundering | Inference treated as fact | Provenance and confidence labels |

## Context budget

Separate window, occupancy (all sent, tool definitions included), usable input (window minus output reserve and margin), and billed tokens; cached tokens still occupy the window. Count tokens with the provider's count endpoint or production tokenizer, never infer them from characters. UTF-8 bytes can compare serialized surfaces; label them as bytes, not tokens or billed savings. For MCP discovery, use the [served-context comparison](tools-and-mcp.md#measure-the-served-surface).

Send the smallest actionable evidence, never a corpus "in case":

- Known target: one focused lookup; more only if a needed handle is missing.
- Search: query, scope, small limit and needed fields; widen if evidence is thin or conflicting.
- Large collection: server-side filter, sort, aggregate and page; next page only if the task needs it. [Tool results](tools-and-mcp.md#schema-and-result) and [handoffs](models-and-delegation.md#handoff-protocols) carry detail only when the next step needs it.
- Copy emitted continuation state unchanged; do not invent a different pagination field or offset.
- Finished search: scope, count, decisive evidence and gaps. Filter bulky intermediates outside the model, keep source evidence, and measure prompt plus tool output together.

Reserve output and headroom for tool results, metadata and unexpected expansion using the actual model and endpoint limits. Measure occupancy; do not fill remaining capacity or copy a fixed percentage split.

## Caching

Different caches solve different problems:

| Cache | What is cached | Key concern |
|---|---|---|
| Prompt / prefix | Reusable model input prefix | Prefix stability |
| KV | Attention key/value states | Model and runtime support |
| Tool result | External API and tool responses | Freshness and permissions |
| Retrieval | Search results | Index version and query |
| Embedding | Text → vectors | Embedding model version |
| Semantic | Answers to similar queries | False semantic matches |
| Session | Session working information | Expiration and isolation |
| Memory | Frequently recalled facts | Staleness |
| Skill | Skill instructions and resources | Skill version |
| MCP discovery | Tool and resource definitions | Capability changes |
| Computation | Deterministic intermediate results | Complete input identity |
| Artifact | Files, builds, analyses | Content hashes |
| Negative | Known absent results | Short expiration |
| Subagent result | Completed delegated findings | Scope and evidence validity |

Version and expire cached data, and scope caches by user and tenant.

## Prompt caching

```text
Stable prefix (cache-friendly):
  system instructions → developer constraints → stable tool definitions → shared reference material
Dynamic suffix:
  current task → retrieved evidence → new tool results → latest user input
```

- Keep the cached prefix byte-identical: tool and schema order, optional fields, JSON key order, image detail and serializer; no per-request prose. A tool-definition change invalidates everything after it.
- Append new conversation content instead of rewriting earlier content, where supported. Keep session data, evidence and request constraints last; timestamps and changing identifiers stay out of stable prefixes.
- One prefix per workflow; no padding to cross a cache threshold; pre-warm only latency-critical prefixes. Use deferred tool loading when available.
- Mode, breakpoints, TTL, minimum length and price come from the provider's current docs for that model and endpoint, never memory.
- Cache only when writes plus expected reads (with expiry and misses) beat no-cache cost for `N` repeats, without adding retries, staleness or base drift.
- Check: record mode, keys and retention → byte-diff cold vs identical-prefix requests → run cold, warm and tail-only change, reading cache-read/write fields, not latency → classify the miss (too short, prefix drift, key change, expiry, routing, telemetry misread) before editing the prompt.
- On a miss, continue and log the diverging prefix; never retry with nothing new. Log one line per attempt (`task`, `arm`, `success`, `latencyMs`, `costUsd`, `tokens` {`input`, `cacheRead`, `cacheWrite`, `output`}) and run `node <skill-folder>/scripts/run-stats.mjs <runs.jsonl>` ([run-stats.mjs](../scripts/run-stats.mjs)); report each arm's `cacheHitRate`, `latencyMs.p50`/`p95` and token buckets beside `successRate`.

Compaction can change the prefix and reduce reuse. A cache is neither a permission boundary, compaction nor long-term memory; review retention separately. A new base version starts a new cache lineage ([frozen base](models-and-delegation.md#frozen-base-prompt)).

## Cost

- Cost per successful task: record each attempt's `costUsd` with its retries and recovery included, then read `costPerSuccess` from run-stats.mjs. It is `null` when a run lacks a cost or none succeeded: fill the gap instead of estimating.
- Price by vendor usage buckets (uncached input, cache write, cache read, output and reasoning, paid tools) at current prices, never a fixed multiplier. Freeze task, quality target, model, tools and traffic; count prefix, dynamic input, history and output separately; price cold, warm, miss, retry and compaction paths; record model, endpoint, tokenizer and date.
- Drop no-op instructions, defer rare tools, and use a cheaper model for bounded work.
- Keep a change only if held-out success and safety hold and cost per success or latency improves; report absolute cost with model, requests, context length, output size and cache telemetry.
- Token-saving claims compare tokens per fact with uncompressed text on your own tasks (simultaneous facts, unresolved work, delayed retrieval), with model, context length and variance. Plain sentences beat `!EV12` codes or dropped vowels; shorter but missing identifiers loses.

## Memory

| Type | Content | Persistence |
|---|---|---|
| Working | Current task state | One task or session |
| Episodic | Past runs, decisions, outcomes | Cross-session |
| Semantic | Stable facts and project knowledge | Cross-session |
| Procedural | Workflows, policies, skills | Versioned |
| Artifact | Code, files, reports, test results | External storage |
| Conversation | Relevant dialogue history | Session or persistent |
| Task | Plan, progress, blockers | Until completion |
| User preference | Explicit durable preferences | Subject to user controls |
| Failure | Failed approaches and their evidence | Task or project scope |

Lifecycle: observe → extract → validate → store → retrieve → use → refresh or forget.

- Keep each durable fact's source and scope; add freshness, confidence or supersession information when needed to decide whether it is still usable.
- Extract reusable, validated information; do not store every interaction.
- Provide deletion, expiration, isolation and correction. Validate before writing: unvalidated writes enable memory poisoning.

## Compaction

| Strategy | Best use | Danger |
|---|---|---|
| Tool-result clearing | Old bulky tool outputs | Removing needed evidence |
| Structured summary | Long conversations | Losing critical details |
| Checkpoint notes | Long engineering tasks | Stale progress information |
| Hierarchical summaries | Extensive research and code exploration | Accumulated drift |
| Task-specific compaction | Details the next action needs | Wrong relevance prediction |
| Externalized artifacts | Large logs, diffs, datasets | Broken references |
| Subagent isolation | Separate exploration contexts | Incomplete handoffs |
| Rolling working set | Recent active context | Forgetting older constraints |
| Provider-native compaction | Supported long-running sessions | Provider-specific semantics |

Compaction, structured notes and subagents are complementary techniques for long-horizon work.

### Checkpoint template

Keep the current goal and constraints, verified findings with source references, consequential decisions, changed artifacts, pending work and next action. Preserve exact handles, commands, approval state or failure details only where resumption depends on them; do not invent checkpoint IDs or metadata fields.

- **Discard:** redundant narration, repeated logs, duplicate or stale raw results, banners, help text, unchanged headers, retry noise and repeated explanations. For failed attempts keep the one-line lesson; for closed hypotheses keep conclusion and evidence ID. Remove subtasks only when the evidence store can re-derive them.
- Compact at a subtask boundary on measured occupancy and quality, not a fixed percentage; recount after. Mid-derivation, checkpoint assumptions, evidence, open branches and next action first.
- Rebuild from original evidence, never a summary of a summary. Hierarchical summaries need rehydration checks against originals to avoid accumulated drift.
- Keep long-task progress, test status and todos outside the window and reload them after compaction.
- Evidence tiers: active context (goal, constraints, key results, next action) → evidence index (IDs to passages, paths, outputs) → cold originals. Reopen the primary source before a consequential conclusion.
- Never let compaction turn an inference into a confirmed fact. Recurrent lossy compaction or filters need evidence that task quality holds, not just fewer tokens.
- Test continuity: after compaction, can the agent still name the next correct action and its supporting evidence?

## Sources

- [Anthropic — Effective context engineering for AI agents](https://www.anthropic.com/research/effective-context-engineering-for-ai-agents)
- [OpenAI — Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
- [OpenAI — Compaction](https://developers.openai.com/api/docs/guides/compaction)
