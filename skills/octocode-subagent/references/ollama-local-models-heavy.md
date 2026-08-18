# Ollama Heavy / Cloud Tags

Load when a candidate tag looks too big for the machine, or when reading the sample inventory this skill was dogfooded against. Why: flagship tags are often cloud-only, and one machine’s list is never the contract.

## Often cloud / heavy (local only if you have big iron)

| Model | Notes |
|-------|--------|
| `qwen3.5:122b`, `397b-cloud` | Flagship Qwen; cloud or multi-GPU |
| `qwen3-coder:480b` | ≥250 GB memory claimed for local |
| `gpt-oss:120b` | ~65 GB download class |
| `nemotron-3-super:120b` | MoE 12B active; multi-agent efficiency |
| `minimax-m2.*` / `m3`, `glm-5.*`, `kimi-k2.*`, `deepseek-v4-*` | Strong coding/agents; many **cloud**-tagged on Ollama |
| `mistral-medium-3.5:128b` | Large dense; workstation+ |

Use `ollama launch <agent> --model <tag>` with these when cloud is acceptable; do not assume they fit a laptop.

## Appendix — sample inventory from one workstation (not required)

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

Next: local-first capability rows in `references/ollama-local-models-matrix.md`; what fits your RAM in `references/ollama-local-models-kits.md`; evidence links in `references/ollama-local-models-sources.md`.
