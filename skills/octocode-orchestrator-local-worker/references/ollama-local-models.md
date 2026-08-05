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

See `references/ollama-local-models-kits.md` for RAM/VRAM recommendations and copy-paste pull commands.

## Capability matrix (developer-relevant Ollama models)

See `references/ollama-local-models-matrix.md` for the full capability matrix (tools, thinking, context sizes) and sample workstation inventories.

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

---

## MCP, Skills, and coding agents — practical rules

1. **MCP works when the model has `tools`.** The MCP server list lives in the host (e.g. Cursor MCP, this repo’s mcp-host). Model choice does not install MCP; it only enables calling tools reliably.
2. **Agent Skills (`SKILL.md`) are host instructions**, not model weights. A stronger tools+thinking model follows skills better; a tiny model may ignore complex skill flows.
3. **Local worker skill ≠ MCP agent.** `octocode-orchestrator-local-worker` forbids tool loops on the worker. Keep MCP/tool agents on the orchestrator (or `ollama launch` coding apps).
4. **Thinking:** default **off** for bulk/classify/JSON (`--think=false`). On for hard reasoning, North Mini Code-style agents, and deepseek-r1.
5. **Context:** prefer ≥128K for repo work. Avoid 32K-era models (`qwen2.5-coder`, older codestral) for large codebases unless shards are tiny.
6. **Apple Silicon:** prefer MLX tags + recent Ollama (≥0.31) for Gemma 4 speedups with coding agents.

---

## Sources & Evidence

See `references/ollama-local-models-sources.md` for community evidence, benchmarks, and Ollama library links.
