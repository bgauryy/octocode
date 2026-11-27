Load when choosing a cache, using prompt caching, or diagnosing a cache miss.

# Caching

| Cache | What is cached | Invalidate or recheck on |
|---|---|---|
| Prompt / prefix | Reusable model input prefix | Any change in the prefix |
| KV | Attention key/value states | Model or runtime change; usable only where the model and runtime support it |
| Tool result | External API and tool responses | Freshness limit; recheck the reader's permission before serving an entry |
| Retrieval | Search results | Index version or query change |
| Embedding | Text → vectors | Embedding model version change |
| Semantic | Answers to similar queries | A false match: a similar query that needs a different answer |
| Session | Session working information | Expiry; isolate per session |
| Memory | Frequently recalled facts | Staleness |
| Skill | Skill instructions and resources | Skill version change |
| MCP discovery | Tool and resource definitions | Capability change |
| Computation | Deterministic intermediate results | Any change in the complete input identity |
| Artifact | Files, builds, analyses | Content hash change |
| Negative | Known absent results | A short expiry |
| Subagent result | Completed delegated findings | Scope change or evidence no longer valid |

Version and expire cached data, and scope caches by user and tenant.

## Prompt caching

```text
Stable prefix (cache-friendly):
  system instructions → developer constraints → stable tool definitions → shared reference material
Dynamic suffix:
  current task → retrieved evidence → new tool results → latest user input
```

- Keep the cached prefix byte-identical: tool and schema order, optional fields, JSON key order, image detail and serializer; no per-request prose.
- Append new conversation content instead of rewriting earlier content. Keep session data, evidence and request constraints last; timestamps and changing identifiers stay out of stable prefixes.
- One prefix per workflow; no padding to cross a cache threshold; pre-warm only latency-critical prefixes.
- Restrict tools mid-session without touching their definitions: adding, removing or editing a definition in the prefix invalidates the cache from there on. Limit which tools are callable instead (OpenAI `allowed_tools` or `tool_choice`; an Anthropic `tool_choice` change keeps the tools and system cache but not the message cache). [Deferred tool loading](mcp-context.md#choose-a-small-coherent-tool-set) keeps the prefix: a discovered definition is appended to the conversation (Anthropic, OpenAI and Gemini document this).
- Mode, breakpoints, TTL, minimum length and price come from the provider's current docs for that model and endpoint, never memory.
- Cache only when writes plus expected reads (with expiry and misses) beat no-cache cost for `N` repeats, without adding retries, staleness or base drift.
- Check: record mode, keys and retention → byte-diff cold vs identical-prefix requests → run cold, warm and tail-only change, reading cache-read/write fields, not latency → classify the miss (too short, prefix drift, key change, expiry, routing, telemetry misread) before editing the prompt.
- On a miss, continue and log the diverging prefix; never retry with nothing new. Log one line per attempt (`task`, `arm`, `success`, `latencyMs`, `costUsd`, `tokens` {`input`, `cacheRead`, `cacheWrite`, `output`}) and run `node <skill-folder>/scripts/run-stats.mjs <runs.jsonl>` ([run-stats.mjs](../scripts/run-stats.mjs)); report each arm's `cacheHitRate`, `latencyMs.p50`/`p95` and token buckets beside `successRate`.

Compaction can change the prefix and reduce reuse. A cache is neither a permission boundary, compaction nor long-term memory; review retention separately. A new base version starts a new cache lineage ([frozen base](handoffs.md#frozen-base-prompt)).

## Sources

- Anthropic: [prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [tool use with prompt caching](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-use-with-prompt-caching); [OpenAI: prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
