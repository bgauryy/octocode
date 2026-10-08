# Token economics and prompt caching

Load for a cost decision on length, model, caching, compaction, or tools; before claiming a token saving or picking a compression ratio; or when expected cache hits are missing. Optimize cost per successful task, not tokens per request. Occupancy: `context-budget.md`.

## Price the workflow

Use vendor usage buckets and current prices; never hardcode a multiplier. Record model ID, endpoint, tokenizer, date, and assumptions.

```text
request cost = uncached_input × input_rate + cache_write × write_rate
             + cache_read × read_rate + output_and_reasoning × output_rate
             + paid_tool_calls + storage/hosting fees
cost per success = (sum request costs + retry/recovery costs) / successful tasks
```

1. Freeze task, quality target, model, tool set, and traffic assumptions.
2. Count invariant prefix, dynamic input, retained history, and output separately.
3. Price cold, warm, miss, retry, and compaction paths; for a prefix reused `N` times, compare no-cache cost with writes plus expected reads, including TTL expiry and miss rate. A cache that adds retries, stale behavior, or base drift loses.
4. Levers: cut no-op instructions, defer rare tools, bound results, retrieve on demand, cache, compact at milestones, cheaper model for bounded work, clearer contract to cut retries.
5. Keep a change only when held-out success and safety hold and cost per success or latency improves.

Report absolute cost with its operating point (requests, model, context length, hit rate, output size, success rate).

## Measure token savings

- Compare **tokens per fact**: `!EV12` can cost more than the prose it replaced, and a shorter block that dropped identifiers did not win.
- Never invent abbreviations for the model to decode or delete vowels, articles, or sentence structure.
- Compare against **uncompressed** on your own tasks; no ratio is universally safe, so state each with model, task, and context length.
- Test simultaneous facts, unresolved work, and delayed retrieval; report **variance, not only the mean** (occasional total derailment is worse than a uniform decline).

## Prompt caching

1. Stable system instructions, tool definitions, output schemas, and reusable examples first; session data, retrieved evidence, latest results, and request constraints last.
2. Keep the cached prefix byte-for-byte stable: tool and schema order, optional fields, image detail, serialization.
3. Split different workflows into different prefixes; never pad to cross a cache threshold.

| Provider | Configure | Observe and diagnose |
|---|---|---|
| OpenAI | automatic caching, or explicit mode or breakpoints where supported; `prompt_cache_key` per model (routing aid on earlier models, isolation or accounting on newer) | `input_tokens_details.cached_tokens`, `cache_write_tokens` when exposed. Miss causes: short prefix, expiry, key split, breakpoint, prefix or config change |
| Anthropic | top-level automatic `cache_control` or explicit breakpoints; tool marker per the current tool contract; keep `tools → system → messages` order | `cache_read_input_tokens`, `cache_creation_input_tokens`, TTL buckets. A tool-definition change invalidates the whole downstream prefix |

Never copy TTLs, minimum lengths, prices, breakpoint limits, or retention from memory; resolve them for the exact model, endpoint, and region, and record source and date.

## Cache check

1. Capture model, endpoint, base-prompt, catalog, and serializer versions, cache mode, key or breakpoints, retention, and pricing.
2. Byte-diff a cold and an identical-prefix request up to the breakpoint, including tool order and JSON key order.
3. Run cold → identical warm → tail-only change; confirm reported cache reads, never latency alone.
4. Classify a remaining miss (eligibility, prefix drift, config invalidation, expiry, routing, telemetry misread) before changing the prompt.

## Agent cache rules

- Never vary tool definitions, schemas, or decorative prose per request unless behavior differs.
- Pre-warm only latency-critical, repeated prefixes.
- Treat a miss as normal: continue, log the diverging prefix, no retries that resend nothing new.
- Log cache telemetry beside task metrics: requests, input, cached, write, output tokens, hit rate, p50/p95 latency, success.
- A cache is not a permission boundary or a substitute for compaction; review retention separately.
- A new base version starts a new cache lineage (`../agents/agent-communication.md`).

Source: [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching).

Next: prove success, hit rate, and latency with `octocode-eval-benchmark`.
