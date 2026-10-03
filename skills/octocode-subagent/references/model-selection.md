# Model Selection

Load to route a local offload, select a model, or decide a pull. Authority: ollama.com/library for tags and `ollama show` on this machine for size, context, and capabilities; blogs are secondary. Recompute the live inventory every session.

## Rules
1. Use exact names from `ollama list` with the `:tag`; prefix matches are wrong (`llama3.2` ≠ `llama3.2-vision`). Never assume a default tag exists.
2. Never use embedding-only models (`*embed*`) as chat or coder workers. Use OCR or vision-only models only for that modality.
3. Re-check live library tags before a pull.

| Job | Tier |
|---|---|
| Classify, label, short triage | `small` |
| Translate short text; one-shot summarize or extract | `small` if warm, else `balanced` (`balanced` if user-facing); escalate on fidelity fail |
| Summarize, extract JSON, checklist, fetched article | `balanced` (article: warm `small` to skim; escalate if grounded_rate < 1) |
| Draft code or tests | `balanced`; prefer coder or instruct |
| Hard local synthesis (rare) | `strong`; else keep on the parent |
| Image caption, OCR | `special` |

| Signal (`ollama list`, `ollama show`) | Bucket |
|---|---|
| `embed` / embedding-only | skip |
| `ocr` in name or OCR-specialized | `special`, OCR jobs only |
| `vision` capability | `special` for images; text use if chat-capable |
| ≤ ~3B parameters | `small` |
| ~4B–14B | `balanced` |
| ~20B+ | `strong` |
| `coder` in name | prefer for drafts within the tier |
| `thinking` capability | OK |

## Algorithm
1. Derive the tier from the job; keep installed chat models at that tier or stronger, minus embedding and wrong-modality models.
2. Ties: warm (`ollama ps`); structured-output models for JSON, extract, classify; coder or strong instruct for drafts; multimodal for images; newer family generation last.
3. Pick the smallest installed chat model that meets acceptance and the structure needs.
4. None fits: work solo; suggest a size class (~7–12B general instruct, a mid coder, or a vision model); pull only with user approval, then wait. Never pull 20B+ without confirmed hardware headroom (memory ≈ download size + KV cache).
5. Cascade once on verify `fail`: next stronger installed chat model (never invent a tag), else solo. Slow or thrashing: drop a tier or shrink shards.
6. Report: `model=<exact> tier=<t> reason=<smallest fit | warm | cascade | solo> think=<on|off>`.

## Capabilities
- Native function calling needs `tools`; thinking needs `thinking`; images or audio need `vision` or `audio` (all from `ollama show`).
- MCP and skills are host features; an `ollama launch` coding app can host tool agents (needs `tools` and long context).
- Prefer ≥128K context for repository work; use 32K-context models only with tiny shards.
- `*-cloud` tags run remotely: use them only on requester opt-in.

Next: write the packet with `references/local-ollama.md`.
