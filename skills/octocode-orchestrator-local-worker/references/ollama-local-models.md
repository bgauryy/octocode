# Ollama local models for developers

Reference for choosing Ollama models on a laptop/workstation: RAM kits, capability matrix (thinking, tools, context, vision), and how that maps to MCP / Agent Skills / coding agents.

**When to load:** RAM / kit advice, catalog browse, MCP/tools capability questions, or pull recommendations.  
**When NOT to load:** routine ROUTE / model select — use [model-selection.md](./model-selection.md) (portable tiers). This file is a catalog, not a required install list.

**Authority:** Ollama library tags + `ollama show` on *the current machine*. Community blogs are secondary. Re-check live tags before pulling.

**Pull gate:** NEVER `ollama pull` multi-GB models unless the user explicitly asks. Suggest a size/class; wait for approval.

**Related:** [model-selection.md](./model-selection.md) (routing), [family-playbooks.md](./family-playbooks.md) (optional family flags/examples), [ollama-cli.md](./ollama-cli.md), [ollama-invoke.md](./ollama-invoke.md).

**One author’s sample inventory (optional, 2026-07-20):** see appendix at bottom — do not treat as the skill’s required set.

---

## Layer map (do not confuse these)

| Layer | What it is | Who provides it | Model requirement |
|-------|------------|-----------------|-------------------|
| **Completion** | Chat / generate text | Model weights | Always |
| **Tools** | Native function calling | Model + Ollama engine | `tools` in `ollama show` / library tag |
| **Thinking** | Explicit chain-of-thought channel | Model + `--think` / API | `thinking` capability |
| **Vision / audio** | Image (and sometimes audio) in | Multimodal models | `vision` / `audio` |
| **MCP** | Tool servers over Model Context Protocol | Host (Cursor, Claude Code, mcp-host, etc.) | Model must support **tools**; MCP is not a model feature |
| **Agent Skills** | `SKILL.md` instruction packs | Host agent / skill loader | Works with any chat model; quality follows model |
| **Coding agents** | Claude Code, OpenCode, Codex via `ollama launch` | Ollama app + agent harness | Prefer **tools + long context**; thinking optional |
| **Embeddings** | Vectors for RAG | Embed-only models | **Never** use as chat/coder |

This skill’s local worker is **single-shot text** (no tool loops). For MCP / agent loops, use a tools-capable model in the **host** agent, not the worker packet path.

---

## Recommended starter kits (by RAM)

Disk sizes below are typical Ollama library Q4-class downloads. **Unified memory / VRAM need ≈ download size + KV cache** (long context burns RAM fast). Prefer Apple Silicon **`-mlx`** tags when listed.

| RAM / unified memory | Daily driver | Bulk worker (classify / JSON) | Coding agent / hard tasks | Also pull |
|----------------------|--------------|-------------------------------|---------------------------|-----------|
| **8–12 GB** | `gemma4:e2b` or `qwen3.5:4b` | `qwen3.5:0.8b` / `2b` | Avoid heavy agents; chat only | `nomic-embed-text` |
| **16 GB** | **`gemma4:12b`** (~7.6 GB, 256K) or `gemma4:e4b` / `qwen3.5:9b` | `qwen3.5:4b` or installed `qwen2.5:0.5b`/`7b` | Light: `lfm2.5:8b` (~5 GB) | `nomic-embed-text` |
| **24–32 GB** | `gemma4:12b` + `qwen3.5:9b` | Same small Qwen | `gemma4:26b` (MoE) **or** `qwen3.6:27b` **or** `qwen3-coder:30b` **or** `gpt-oss:20b` | embed + optional OCR |
| **48 GB+** | Above + `gemma4:31b` or `qwen3.6:35b` | Keep a ≤9B for bulk | `north-mini-code-1.0` (~19 GB, ~488K ctx) / `laguna-xs-2.1` / large MoE | As needed |
| **Cloud only** | — | — | `*:cloud` tags (e.g. `gemma4:31b-cloud`) | Not local |

### Copy-paste pulls

```bash
# Sweet-spot workstation (≈16–32 GB)
ollama pull gemma4:12b
ollama pull qwen3.5:9b
ollama pull nomic-embed-text

# Optional coding agents (≈32 GB+)
ollama pull qwen3-coder:30b
# or
ollama pull gpt-oss:20b
# or
ollama pull north-mini-code-1.0

# Apple Silicon speed (when available)
ollama pull gemma4:12b-mlx
```

### Job → model (this skill)

Use portable tiers in [model-selection.md](./model-selection.md).  
Optional family examples: [family-playbooks.md](./family-playbooks.md).  
Do not copy brand-specific defaults from this catalog into routing unless those tags are installed.

---

## Capability matrix (developer-relevant Ollama models)

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

---

## What `ollama show` reports (how to read any machine)

Run `ollama show <MODEL>` and map:

| Field | Use |
|---|---|
| `parameters` | Size → small / balanced / strong |
| `context length` | Shard sizing; prefer longer ctx for big files |
| `Capabilities: embedding` | Never as chat worker |
| `Capabilities: vision` / `audio` | Modality jobs |
| `Capabilities: tools` | Host MCP/agent loops — **not** this skill’s worker path |
| `Capabilities: thinking` | Default off for bulk (`--think=false`) |

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

---

## MCP, Skills, and coding agents — practical rules

1. **MCP works when the model has `tools`.** The MCP server list lives in the host (e.g. Cursor MCP, this repo’s mcp-host). Model choice does not install MCP; it only enables calling tools reliably.
2. **Agent Skills (`SKILL.md`) are host instructions**, not model weights. A stronger tools+thinking model follows skills better; a tiny model may ignore complex skill flows.
3. **Local worker skill ≠ MCP agent.** `octocode-orchestrator-local-worker` forbids tool loops on the worker. Keep MCP/tool agents on the orchestrator (or `ollama launch` coding apps).
4. **Thinking:** default **off** for bulk/classify/JSON (`--think=false`). On for hard reasoning, North Mini Code-style agents, and deepseek-r1.
5. **Context:** prefer ≥128K for repo work. Avoid 32K-era models (`qwen2.5-coder`, older codestral) for large codebases unless shards are tiny.
6. **Apple Silicon:** prefer MLX tags + recent Ollama (≥0.31) for Gemma 4 speedups with coding agents.

---

## Community / secondary evidence (not authority)

| Claim | Source | Confidence |
|-------|--------|------------|
| Gemma 4 vs Qwen 3.5 tradeoffs (context, multilingual, size ladder) | [MindStudio comparison](https://www.mindstudio.ai/blog/gemma-4-vs-qwen-3-5-open-weight-comparison) (2026-04) | Medium |
| Qwen often preferred for agents; Gemma for efficient local | [Codersera 2026 comparison](https://codersera.com/blog/gemma-4-vs-qwen-3-5-comparison-2026/) | Medium |
| Local ABS bench: `qwen3.5:9b` top overall; all ≥7B saturate tool pick | [JConradoN/local-llm-benchmark](https://github.com/JConradoN/local-llm-benchmark) | Medium |
| Gemma 4 31B strong LiveCodeBench / Codeforces vs prior Gemma | Ollama Gemma 4 readme + [ai.rs writeup](https://ai.rs/ai-developer/gemma-4-vs-qwen-3-5-vs-llama-4-compared) | Medium (benchmarks move) |
| Gemma 4 31B often more token-efficient than Qwen3.5 27B when thinking | [Kaitchup Substack](https://kaitchup.substack.com/p/gemma-4-31b-vs-qwen35-27b-inference) | Medium |

Primary decisions should follow **library tags + your latency/quality on your hardware**, not blog rankings alone.

---

## Decision flowchart

```text
Need embeddings?     → nomic-embed-text (stop)
Need OCR?            → deepseek-ocr / glm-ocr (stop)
Need MCP / agent tools loop?
  yes → pick tools (+ thinking if hard) model ≥9–12B if possible
  no  → allowlisted worker job (this skill)
RAM ≤16 GB?          → gemma4:12b or qwen3.5:9b (+ tiny Qwen for classify)
RAM ≥32 GB coding?   → add qwen3-coder:30b | gpt-oss:20b | north-mini-code-1.0 | gemma4:26b/31b
Bulk JSON/classify?  → smallest Qwen; think off
Draft code?          → gemma4:12b+; think off unless verify fails
```

---

## Maintenance

1. `ollama list` / `ollama show` after every pull.
2. Prefer newer family tags (`gemma4`, `qwen3.5`/`3.6`) over `gemma3` / `qwen2.5` when both installed.
3. Update this page when library tags or `ollama show` capabilities change; keep [model-selection.md](./model-selection.md) as the **routing** algorithm, this file as the **catalog**.

## Sources

Primary (Ollama):

- https://ollama.com/library — catalog popularity and families
- https://ollama.com/library/gemma4 — sizes, ctx, tools/thinking/vision/audio, benchmarks
- https://ollama.com/library/qwen3.5 — size ladder, 256K, multimodal, tools/thinking
- https://ollama.com/library/qwen3.6 — agentic coding / thinking preservation
- https://ollama.com/library/qwen3-coder — 30B MoE coding agent
- https://ollama.com/library/gpt-oss — tools, thinking, agent features
- https://ollama.com/library/north-mini-code-1.0 — SWE agent MoE, long context tags
- https://ollama.com/library/laguna-xs-2.1 — local long-horizon coding MoE
- https://ollama.com/library/lfm2.5 — edge tool calling
- https://ollama.com/library/deepseek-r1 — reasoning family
- https://ollama.com/library/qwen2.5 — legacy general Qwen (32K)
- https://ollama.com/library/qwen2.5-coder — legacy coder sizes / 32K
- https://ollama.com/library/devstral — older SWE agent
- https://ollama.com/library/codestral — FIM coder (no tools tag)
- https://ollama.com/library/granite4.1 — 3b/8b/30b, 128K, tools
- https://ollama.com/library/glm-ocr — OCR, vision+tools, ~2.2 GB / 128K
- https://ollama.com/search?c=tools — tools-capable set
- https://ollama.com/search?c=thinking — thinking-capable set
- https://ollama.com/blog — MLX / Gemma 4 coding-agent speed (2026-06)
- Local: `ollama show` / `ollama list` on installed tags (2026-07-20)

Secondary (community; medium confidence — see table above):

- https://www.mindstudio.ai/blog/gemma-4-vs-qwen-3-5-open-weight-comparison
- https://codersera.com/blog/gemma-4-vs-qwen-3-5-comparison-2026/
- https://github.com/JConradoN/local-llm-benchmark
- https://ai.rs/ai-developer/gemma-4-vs-qwen-3-5-vs-llama-4-compared
- https://kaitchup.substack.com/p/gemma-4-31b-vs-qwen35-27b-inference
