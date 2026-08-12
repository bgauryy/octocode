# Ollama Capability Matrix

Legend: **Y** = library tag and/or `ollama show`; **—** = not claimed; **?** = verify after pull. Sizes/context from [ollama.com/library](https://ollama.com/library) (re-check with `ollama show <tag>`).

**Thinking vs reasoning:** `thinking` = explicit CoT channel Ollama can toggle. “Reasoning quality” is not a binary library flag — use LiveCodeBench / SWE / your verify gate.

### A. Local-first families (best defaults)

| Model / tag | Approx size | Context | Tools | Thinking | Vision | Audio | Best for | MCP / Skills / agents |
|-------------|-------------|---------|-------|----------|--------|-------|----------|------------------------|
| `gemma4:e2b` | ~7.2 GB | 128K | Y | Y | Y | Y | Edge laptop | Tools → MCP OK; weak for big repos |
| `gemma4:e4b` / `latest` | ~9.6 GB | 128K | Y | Y | Y | Y | Laptop default | Strong general + multimodal |
| `gemma4:12b` | ~7.6 GB | **256K** | Y | Y | Y | Y | **Workstation default** | Best all-rounder for local agents |
| `gemma4:26b` (MoE ~3.8B active) | ~18 GB | 256K | Y | Y | Y | — | Quality / speed tradeoff | Prefer when 31B too slow |
| `gemma4:31b` | ~20 GB | 256K | Y | Y | Y | — | Peak Gemma 4 local | Best coding/reasoning in family; library: text+image only |
| `qwen3.5:0.8b`–`4b` | 1–3.4 GB | **256K** | Y | Y | Y | — | Tiny workers | Tools yes; keep jobs simple |
| `qwen3.5:9b` | ~6.6 GB | 256K | Y | Y | Y | — | Mid-tier value | Strong tool/agent score in community ABS benches |
| `qwen3.5:27b` / `35b` | 17–24 GB | 256K | Y | Y | Y | — | Strong dense / MoE | Coding + multilingual |
| `qwen3.6:27b` / `35b` | 17–24 GB | 256K | Y | Y | Y | — | Agentic coding upgrade | Prefer over 3.5 for long agent runs |
| `qwen3-coder:30b` | ~19 GB | 256K | Y | ?† | — | — | Repo / SWE agents | MoE ~3.3B active; great for OpenCode/Claude Code |
| `gpt-oss:20b` | ~14 GB | 128K | Y | Y | — | — | OpenAI open weights | Native agent tooling; reasoning effort levels |
| `lfm2.5:8b` | ~5.2 GB | 125K | Y | Y | — | — | Fast tool calling on edge | Good MCP client brain when RAM tight |
| `north-mini-code-1.0` | ~19 GB | **~488K**‡ | Y | Y | — | — | Agentic SWE specialist | Trained for OpenCode / SWE harnesses |
| `laguna-xs-2.1` | ~20 GB | 256K | Y | Y | — | — | Long-horizon local coding | MoE ~3B active |
| `deepseek-r1:8b`–`32b` | 5–20 GB | 128K | Y | Y | — | — | Hard reasoning / math | Slow for bulk workers |
| `qwen2.5:0.5b`–`32b` | 0.4–19 GB | **32K** | Y | — | — | — | Legacy workers (installed) | Fine for JSON; short context |
| `qwen2.5-coder:7b`–`32b` | 4.7–20 GB | **32K** | Y | — | — | — | Classic code gen | Prefer qwen3-coder / gemma4 for new pulls |
| `devstral:24b` | ~14 GB | 128K | Y | — | — | — | Older SWE agent | Superseded for most new setups |
| `codestral:22b` | ~13 GB | 32K | — | — | — | — | FIM / completion era | No tools tag → weak MCP brain |
| `granite4.1:3b` / `8b` / `30b` | 2.1 / 5.3 / 17 GB | 128K | Y | — | — | — | Enterprise JSON / RAG | Strong tool scores in ABS |
| `llama3.2-vision` | ~7.8 GB | 128K | Y | — | Y | — | Older vision chat (installed) | Prefer gemma4 for new vision work |
| `gemma3:12b` | ~8.1 GB | 128K | — | — | Y | — | Legacy Gemma (installed) | Prefer gemma4 when both present |
| `nomic-embed-text` | ~274 MB | 2K (num_ctx 8K) | — | — | — | — | Embeddings only | **No** chat / MCP brain |
| `deepseek-ocr` | ~6.7 GB | 8K | — | — | Y | — | Document OCR (installed) | Special modality |
| `glm-ocr` | ~2.2 GB | 128K | Y | — | Y | — | Document OCR (lighter) | vision+tools; not a general coder |

† Library emphasizes tools/agentic coding; confirm `thinking` with `ollama show` after pull.  
‡ Library tags list ~488K; model card also cites 256K training / long-horizon — use tag value after pull.

### B. Often cloud / heavy (local only if you have big iron)

| Model | Notes |
|-------|--------|
| `qwen3.5:122b`, `397b-cloud` | Flagship Qwen; cloud or multi-GPU |
| `qwen3-coder:480b` | ≥250 GB memory claimed for local |
| `gpt-oss:120b` | ~65 GB download class |
| `nemotron-3-super:120b` | MoE 12B active; multi-agent efficiency |
| `minimax-m2.*` / `m3`, `glm-5.*`, `kimi-k2.*`, `deepseek-v4-*` | Strong coding/agents; many **cloud**-tagged on Ollama |
| `mistral-medium-3.5:128b` | Large dense; workstation+ |

Use `ollama launch <agent> --model <tag>` with these when cloud is acceptable; do not assume they fit a laptop.

### Appendix — sample inventory from one workstation (not required)

Recorded 2026-07-20 on an author machine (Ollama server ~0.31.x). Other setups will differ — always trust live `ollama list`.

| Example installed tag | Params | Context | Capabilities (then) |
|---------------|--------|---------|--------------|
| `gemma4:12b` | 11.9B | 262144 | completion, vision, audio, tools, thinking |
| `gemma4:latest` | 8.0B | 131072 | completion, vision, audio, tools, thinking |
| `qwen2.5:0.5b` / `7b` / `32b` | 0.5–32.8B | 32768 | completion, tools (no thinking) |
| `nomic-embed-text` | 137M | 2048 | **embedding** only |
| `deepseek-ocr` | 3.3B | 8192 | completion, vision |
| `llama3.2-vision` | 10.7B | 131072 | completion, vision, tools |
| `gemma3:12b` | 12.2B | 131072 | completion, vision |

Always re-run `ollama show <MODEL>` after pull — tags and capabilities change.