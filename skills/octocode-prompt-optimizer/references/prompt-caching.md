# Prompt Caching & Stable Prefixes

Load when repeated agent/API calls share instructions, tools, schemas, examples, or long-lived context and cost or latency matters.

**Cache stable, high-signal prefixes; keep dynamic task data at the end.** Caching reduces repeated input work, not output generation, bad context, or unbounded tool results.

## Layout contract

1. Put stable system instructions, tool definitions, output schemas, and reusable examples first.
2. Put session/user data, retrieved evidence, latest tool results, and request-specific constraints last.
3. Keep the cached prefix byte-for-byte stable: tool/schema order, optional fields, image detail, and serialization must not drift.
4. Split genuinely different workflows into different prefixes; do not pad a prompt with low-value text merely to cross a cache threshold.

## Provider controls

| Provider | Use | Observe |
|---|---|---|
| OpenAI | Automatic caching on supported prompts; use a consistent `prompt_cache_key` for a shared prefix and choose retention only after data-policy review. | `usage.prompt_tokens_details.cached_tokens`, hit rate, latency, and cost. |
| Anthropic | Use top-level automatic `cache_control` for growing conversations or explicit breakpoints for a chosen shared prefix. | `cache_read_input_tokens`, `cache_creation_input_tokens`, misses, latency, and cost. |

## Agent efficiency rules

- Cache the invariant instruction/tool layer; retrieve and filter live evidence for the actual decision.
- Do not vary tool definitions, output schemas, or decorative prompt prose per request unless behavior truly differs.
- Pre-warm only latency-critical, repeatedly used prefixes after comparing cache-write cost against expected hits.
- Treat a cache miss as normal: continue safely, log the diverging prefix/version, and avoid retries that resend no new information.
- Cache telemetry belongs beside task metrics: request count, input/cached/write/output tokens, hit rate, p50/p95 latency, and task success.
- Review retention and data controls separately from token savings. A cache is not a permission boundary or a substitute for compaction.

## Sources
- OpenAI, [Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching) — exact-prefix layout, automatic caching, cache keys/retention, and `cached_tokens` telemetry.
- Anthropic, [Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) — prefix breakpoints, automatic caching, TTLs, and cache usage fields.
- Anthropic, [Tool use with prompt caching](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-use-with-prompt-caching) — stable tool definitions and cache invalidation considerations.
