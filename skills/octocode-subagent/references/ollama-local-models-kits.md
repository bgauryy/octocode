# Ollama Starter Kits (by RAM)

Load when RAM/VRAM headroom decides what to run or pull. Why: unified memory, not model quality, is the binding constraint on a laptop.

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

Use portable tiers in `references/model-selection.md`; optional family examples in `references/family-playbooks.md`. Do not copy brand-specific defaults from this catalog into routing unless those tags are installed.

Next: per-tag capabilities in `references/ollama-local-models-matrix.md`; heavy/cloud classes in `references/ollama-local-models-heavy.md`; back to the catalog in `references/ollama-local-models.md`.