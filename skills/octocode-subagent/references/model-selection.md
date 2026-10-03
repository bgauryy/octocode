# Model Selection

Load when you route a local offload or select a model, or the question is RAM kits, catalog browse, tools or MCP capability, or pull advice. Only the live inventory says which tag exists; the smallest fitting one wins. Recompute from the live list every session. The catalog below is not an install list.

## Rules

1. Run `ollama list` first. Use exact listed names, including the `:tag`. Prefix matches are wrong (`llama3.2` ≠ `llama3.2-vision`).
2. Never assume a default tag exists (for example `llama3.2` or `gemma4:12b`).
3. Never use embedding-only models (`*embed*`) as chat or coder workers. Use OCR or vision-only models only for jobs that need that modality.
4. Prefer the smallest installed chat model that meets acceptance. Escalate once on verify fail.
5. Authority: Ollama library tags plus `ollama show` on this machine; blogs are secondary. Re-check live tags before a pull. Do not pull multi-GB models unless the requester asks.

| Job | Tier |
|---|---|
| Classify, label, short triage | `small` |
| Translate short text | `small` or `balanced` (`balanced` if user-facing); prefer warm; escalate on fidelity fail |
| Small one-shot summarize or extract | `small` if warm, else `balanced` |
| Summarize, extract JSON, checklist, fetched article | `balanced` (article: warm `small` to skim; escalate if grounded_rate < 1) |
| Draft code or tests | `balanced`; prefer coder or instruct signals |
| Hard local synthesis (rare) | `strong`; else keep on the parent |
| Image caption, OCR | `special` |

| Signal (`ollama list`, `ollama show`) | Bucket |
|---|---|
| `embed` / embedding-only | skip |
| `ocr` in name or OCR-specialized | `special`, OCR jobs only |
| `vision` capability | `special` for images; text use if chat-capable |
| ≤ ~3B (`0.5b`, `1b`, `2b`, `3b`, `e2b`) | `small` |
| ~4B–14B (`7b`, `8b`, `9b`, `12b`, mid `latest`) | `balanced` |
| ~20B+ (`26b`, `27b`, `30b`, `31b`, `32b`, `70b`) | `strong` |
| `coder` in name | prefer for `draft` within the tier |
| `thinking` capability | OK; think off for bulk |

## Algorithm

1. Derive the tier from the job. Keep installed chat models at that tier or stronger.
2. Drop embedding and wrong-modality models.
3. Ties: prefer a warm model (`ollama ps`); structured-output models for JSON, extract, classify; coder or strong instruct for drafts; multimodal for images; a newer family generation last.
4. Pick the smallest model that meets the structure needs. Set `OLLAMA_WORKER_MODEL` to the exact name. Default `--think=false` for bulk.
5. None fits: work solo; suggest a size class (~7–12B general instruct, a mid coder, or a vision model) and pull only with user approval; then wait. Never pull 26B+ without confirmed hardware headroom.

- Cascade: on verify `fail`, next stronger installed chat model, else solo. Slow or thrashing: drop a tier or shrink shards.
- Report: `model=<exact> tier=<t> reason=<smallest fit | warm | cascade | solo> think=<on|off>`.

## Family flags (only if installed)

- Gemma 4 (`gemma4:*`): library sampling `temperature=1.0`, `top_p=0.95`, `top_k=64`; use `--temperature 0.2` for JSON. Vision: image before text; vision token budget 70–1120 trades detail for speed. `*-cloud` tags only if the requester opts in; confirm `latest` with `ollama show`. Apple Silicon: prefer `-mlx` tags and Ollama ≥0.31.
- Qwen (`qwen*`): strong at JSON extract and classify; context varies by generation (`qwen2.5` ~32K; trust `ollama show`).
- Other families: bucket by size and capabilities; no playbook unless flags differ. Example tags are never must-install; with no listed family installed, route by size tier only, never by catalog brand defaults.

## Capability layers

- Tools (native function calling) need the `tools` capability; thinking needs `thinking`; images or audio need `vision` or `audio`.
- MCP and Agent Skills are host features. A model needs `tools` for MCP; a stronger tools + thinking model follows skills better. Keep MCP or tool agents on the parent or an `ollama launch` coding app (Claude Code, OpenCode, Codex), which wants tools and long context.
- Thinking on only for hard reasoning (north-mini-code-style agents, `deepseek-r1`). Prefer ≥128K context for repository work; 32K-era models (`qwen2.5-coder`, older `codestral`) only with tiny shards.
- Memory need ≈ download size + KV cache. Flagship tags (`qwen3.5:122b`, `qwen3-coder:480b`, `gpt-oss:120b`, `minimax-m2.*`, `glm-5.*`, `kimi-k2.*`, `deepseek-v4-*`) are often cloud-only or multi-GPU; never assume they fit a laptop.

## Kits by RAM (typical Q4 downloads)

| RAM | Daily driver | Bulk (classify, JSON) | Coding agent, hard tasks |
|---|---|---|---|
| 8–12 GB | `gemma4:e2b` or `qwen3.5:4b` | `qwen3.5:0.8b` / `2b` | Avoid heavy agents; chat only |
| 16 GB | `gemma4:12b` or `gemma4:e4b` / `qwen3.5:9b` | `qwen3.5:4b`, installed `qwen2.5:0.5b`/`7b` | Light: `lfm2.5:8b` |
| 24–32 GB | `gemma4:12b` + `qwen3.5:9b` | Same small Qwen | `gemma4:26b`, `qwen3.6:27b`, `qwen3-coder:30b`, or `gpt-oss:20b` |
| 48 GB+ | Above + `gemma4:31b` or `qwen3.6:35b` | Keep a ≤9B | `north-mini-code-1.0`, `laguna-xs-2.1`, large MoE |
| Cloud | — | — | `*:cloud` tags (for example `gemma4:31b-cloud`); not local |

Also pull `nomic-embed-text` for embeddings; add OCR on 24 GB+ if needed.

## Capability matrix (T tools · Th thinking · V vision · A audio; ? = verify with `ollama show`)

| Tag | Size | Ctx | Caps | Best for |
|---|---|---|---|---|
| `gemma4:e2b` / `e4b` (`latest`) | 7.2 / 9.6 GB | 128K | T Th V A | Edge / laptop default; weak for big repos |
| `gemma4:12b` | 7.6 GB | 256K | T Th V A | Workstation default, best local all-rounder |
| `gemma4:26b` (MoE) / `31b` | 18 / 20 GB | 256K | T Th V | Quality-speed tradeoff / peak Gemma 4 |
| `qwen3.5:0.8b`–`4b` / `9b` | 1–3.4 / 6.6 GB | 256K | T Th V | Tiny workers / mid-tier value |
| `qwen3.5` / `qwen3.6:27b`, `35b` | 17–24 GB | 256K | T Th V | Strong; 3.6 for long agentic coding |
| `qwen3-coder:30b` (MoE) | 19 GB | 256K | T Th? | Repository and SWE agents |
| `gpt-oss:20b` | 14 GB | 128K | T Th | Open weights; native agent tooling |
| `lfm2.5:8b` | 5.2 GB | 125K | T Th | Fast tool calling when RAM is tight |
| `north-mini-code-1.0` / `laguna-xs-2.1` | 19 / 20 GB | ~488K / 256K | T Th | Agentic SWE / long-horizon coding |
| `deepseek-r1:8b`–`32b` | 5–20 GB | 128K | T Th | Hard reasoning; slow for bulk |
| `qwen2.5:0.5b`–`32b` / `qwen2.5-coder` | 0.4–20 GB | 32K | T | Legacy JSON workers / classic code gen |
| `devstral:24b` / `codestral:22b` | 14 / 13 GB | 128K / 32K | T / — | Older SWE / FIM; codestral weak MCP brain |
| `granite4.1:3b`/`8b`/`30b` | 2.1–17 GB | 128K | T | Enterprise JSON, RAG |
| `llama3.2-vision` / `gemma3:12b` | 7.8 / 8.1 GB | 128K | T V / V | Legacy vision; prefer gemma4 |
| `deepseek-ocr` / `glm-ocr` | 6.7 / 2.2 GB | 8K / 128K | V / T V | Document OCR only |
| `nomic-embed-text` | 274 MB | 2K (num_ctx 8K) | — | Embeddings only; no chat |

Next: write the packet with `references/local-ollama.md`.
