# Model Selection

Load when routing any local offload — every **ROUTE** / model select. Why: only the live inventory says which tag exists, and the smallest fitting one wins.

Portable: works on any `ollama list` — do not assume Gemma, Qwen, or any tag is installed. Load ladder: this file always; `references/family-playbooks.md` only if an installed family needs special flags or two families tie; `references/ollama-local-models.md` only for RAM kits, catalog, MCP/tools matrix, or pull suggestions (ask before pull). <!-- style-lint: ignore-line passive-voice -->

## Absolute rules

1. Run `ollama list` first. Use **exact** listed names (including tags).
2. Never invent or assume a default tag (do not assume `llama3.2` or `gemma4:12b` exists); named tags elsewhere in this skill are **examples**, not requirements.
3. Never use embedding-only models (`*embed*`) as chat/summarize workers, nor OCR/vision-only models for pure-text jobs unless the input needs that modality.
4. Prefer the **smallest** installed chat model that can meet acceptance. Escalate once on verify fail.

## Job → tier (capability, not brand)

| Job pattern | Tier | Why |
|---|---|---|
| Classify / label / short triage | `small` | Speed; low creativity |
| Translate short text | `small` or `balanced` | Prefer warm; escalate if fidelity fails |
| Article / web-body summarize (already fetched) | `balanced` | Quote grounding; escalate if grounded_rate < 1 |
| Small one-shot summarize / extract | `small` if warm +; else `balanced` | Latency for tiny jobs |
| Summarize, extract JSON, checklist | `balanced` | Instruction following + structure |
| Draft code / tests | `balanced` (prefer coder/instruct signals) | Code structure |
| Hard local synthesis (rare, still allowlisted) | `strong` | Else keep on orchestrator |
| Image caption / OCR | `special` | Needs vision / OCR capability |

**Bucket installed models** — after `ollama list`, optionally `ollama show <name>` (confirm ambiguous `parameters`, `context length`, capabilities). Bucket by **signals**:

| Signal | Bucket |
|---|---|
| `embed` / embedding-only capability | **skip** for workers |
| `ocr` in name, or OCR-specialized | `special` (OCR jobs only) |
| `vision` capability + image input | `special` for vision; also usable as text if chat-capable |
| params / name ≤ ~3B (`0.5b`, `1b`, `2b`, `3b`, `e2b`) | `small` |
| ~4B–14B (`7b`, `8b`, `9b`, `12b`, mid `latest`) | `balanced` |
| ~20B+ (`26b`, `27b`, `30b`, `31b`, `32b`, `70b`) | `strong` |
| `coder` / code-focused name | prefer for `draft` when in tier |
| `thinking` capability | OK; default **think off** for bulk |

For ties, prefer an already-warm model (`ollama ps`) at the same tier. Prefer structured-output models for JSON, extraction, or classification; coder or strong instruct models for drafting and code; and multimodal models for images. Use a newer family generation as the last tie-break. Illustrative only: on a Gemma and Qwen mix, tiny Qwen → classify, mid Qwen → JSON, mid Gemma → draft/vision, large anything → cascade. **Recompute from the live list every session.**

**Algorithm:** derive the tier from the job pattern. Keep installed chat models in that tier or stronger. Drop embedding and wrong-modality models, apply optional family tie-breaks, and pick the smallest model that meets the structure needs. If none remain, work solo. Suggest a size class to pull with approval. Set `OLLAMA_WORKER_MODEL` to the exact listed name. For bulk work, default thinking-capable models to `--think=false`.

- **Cascade:** on verify `fail` → next stronger installed chat model, else orchestrator solo.
- **Hardware:** slow/thrashing → drop tier or shrink shards; JSON fail on tiny model → cascade once; prefer smaller download/params that passes verify.
- **Report:** `model=<exact> tier=<t> reason=<smallest fit | warm | cascade | solo> think=<on|off>`

Next: invoke with `references/ollama-invoke.md`; family flags or tie-break examples in `references/family-playbooks.md`; gate the return with `references/verify-gate.md`.
