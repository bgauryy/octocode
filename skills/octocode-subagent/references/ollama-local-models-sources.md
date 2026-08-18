# Ollama Local Models Sources

Load when a catalog claim needs its source or confidence stated. Why: library tags plus your own hardware measurements outrank blog rankings.

## Community / secondary evidence (not authority)

| Claim | Source | Confidence |
|-------|--------|------------|
| Gemma 4 vs Qwen 3.5 tradeoffs (context, multilingual, size ladder) | [MindStudio comparison](https://www.mindstudio.ai/blog/gemma-4-vs-qwen-3-5-open-weight-comparison) (2026-04) | Medium |
| Qwen often preferred for agents; Gemma for efficient local | [Codersera 2026 comparison](https://codersera.com/blog/gemma-4-vs-qwen-3-5-comparison-2026/) | Medium |
| Local ABS bench: `qwen3.5:9b` top overall; all ≥7B saturate tool pick | [JConradoN/local-llm-benchmark](https://github.com/JConradoN/local-llm-benchmark) | Medium |
| Gemma 4 31B strong LiveCodeBench / Codeforces vs prior Gemma | Ollama Gemma 4 readme + [ai.rs writeup](https://ai.rs/ai-developer/gemma-4-vs-qwen-3-5-vs-llama-4-compared) | Medium (benchmarks move) |
| Gemma 4 31B often more token-efficient than Qwen3.5 27B when thinking | [Kaitchup Substack](https://kaitchup.substack.com/p/gemma-4-31b-vs-qwen35-27b-inference) | Medium |

Primary decisions should follow **library tags + your latency/quality on your hardware**, not blog rankings alone.

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

Next: capability rows in `references/ollama-local-models-matrix.md`; kits in `references/ollama-local-models-kits.md`; back to the catalog in `references/ollama-local-models.md`.
