# Ollama local models for developers

Load when the question is RAM / kit advice, catalog browse, MCP/tools capability, or pull recommendations — **not** routine ROUTE / model select (that is `references/model-selection.md`, portable tiers). Why: this is a catalog, not a required install list.

Choosing Ollama models on a laptop/workstation: RAM kits, capability matrix (thinking, tools, context, vision), and how that maps to MCP / Agent Skills / coding agents.

**Authority:** Ollama library tags + `ollama show` on *the current machine*. Community blogs are secondary. Re-check live tags before pulling. **Pull gate:** Do not run `ollama pull` for multi-GB models unless the user explicitly asks; suggest a size or class and wait for approval.

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

## Reading any machine — run `ollama show <MODEL>` and map

| Field | Use |
|---|---|
| `parameters` | Size → small / balanced / strong |
| `context length` | Shard sizing; prefer longer ctx for big files |
| `Capabilities: embedding` | Never as chat worker |
| `Capabilities: vision` / `audio` | Modality jobs |
| `Capabilities: tools` | Host MCP/agent loops — **not** this skill’s worker path |
| `Capabilities: thinking` | Default off for bulk (`--think=false`) |

## MCP, Skills, and coding agents — practical rules

1. **MCP works when the model has `tools`.** The MCP server list lives in the host (e.g. Cursor MCP, this repo’s mcp-host). Model choice does not install MCP; it only enables calling tools reliably.
2. **Agent Skills (`SKILL.md`) are host instructions**, not model weights. A stronger tools+thinking model follows skills better; a tiny model may ignore complex skill flows.
3. **Local worker skill ≠ MCP agent.** The local Ollama path (`references/local-ollama.md`) forbids tool loops on the worker. Keep MCP/tool agents on the orchestrator (or `ollama launch` coding apps).
4. **Thinking:** default **off** for bulk/classify/JSON (`--think=false`). On for hard reasoning, North Mini Code-style agents, and deepseek-r1.
5. **Context:** prefer ≥128K for repo work. Avoid 32K-era models (`qwen2.5-coder`, older codestral) for large codebases unless shards are tiny.
6. **Apple Silicon:** prefer MLX tags + recent Ollama (≥0.31) for Gemma 4 speedups with coding agents.

Next: RAM/VRAM kits and copy-paste pulls in `references/ollama-local-models-kits.md`; per-tag capability rows in `references/ollama-local-models-matrix.md`; cloud/heavy tags and one sample inventory in `references/ollama-local-models-heavy.md`; evidence and library links in `references/ollama-local-models-sources.md`; portable routing in `references/model-selection.md` with optional family flags in `references/family-playbooks.md`; commands and invoke in `references/ollama-cli.md` · `references/ollama-invoke.md`.
