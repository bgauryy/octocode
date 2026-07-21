# Model Selection

Load on every **ROUTE** / model select. Portable: works on any `ollama list` — do not assume Gemma, Qwen, or any tag is installed.

Load ladder:

1. This file — always.
2. `family-playbooks.md` — only if an installed family needs special flags, or two families both fit and you need examples.
3. `ollama-local-models.md` — only for RAM kits, catalog, MCP/tools matrix, or pull suggestions (ask before pull).

## Absolute rules

1. Run `ollama list` first. Use **exact** listed names (including tags).
2. Never invent or assume a default tag (e.g. do not assume `llama3.2` or `gemma4:12b` exists).
3. Never use embedding-only models (`*embed*`) as chat/summarize workers.
4. Never use OCR/vision-only models for pure-text jobs unless the input needs that modality.
5. Prefer the **smallest** installed chat model that can meet acceptance. Escalate once on verify fail.
6. Named tags elsewhere in this skill are **examples**, not requirements.

## Job → tier (capability, not brand)

| Job pattern | Tier | Why |
|---|---|---|
| Classify / label / short triage | `small` | Speed; low creativity |
| Translate short text | `small` or `balanced` | Prefer warm; escalate if fidelity fails |
| Article / web-body summarize (already fetched) | `balanced` | Quote grounding; escalate if grounded_rate < 1 |
| Small one-shot summarize / extract | `small` if warm + simple; else `balanced` | Latency for tiny jobs |
| Summarize, extract JSON, checklist | `balanced` | Instruction following + structure |
| Draft code / tests | `balanced` (prefer coder/instruct signals) | Code structure |
| Hard local synthesis (rare, still allowlisted) | `strong` | Else keep on orchestrator |
| Image caption / OCR | `special` | Needs vision / OCR capability |

## Bucket installed models

After `ollama list`, optionally `ollama show <name>`. Bucket by **signals**:

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

Confirm ambiguous sizes with `ollama show` (`parameters`, `context length`, capabilities).

### Heuristics when several models fit

1. Prefer already-warm (`ollama ps`) if same tier.
2. For JSON/extract/classify: models known for structured output (many Qwen/instruct tags) **if installed**.
3. For draft/code: coder or strong instruct tags **if installed**.
4. For vision: any installed model with `vision` (or multimodal) capability.
5. Newer family generation over older sibling **when both installed** (e.g. gemma4 over gemma3) — only as a tie-break.

### Example mapping (illustrative only)

If a machine happened to have a Gemma + Qwen mix, routing might look like: tiny Qwen → classify; mid Qwen → JSON; mid Gemma → draft/vision; large anything → cascade. **Recompute from the live list every session.**

## Selection algorithm

```text
1. tier = f(job pattern)
2. candidates = installed chat models in tier ∪ stronger
3. drop embed / wrong-modality
4. optional family tie-break (family-playbooks.md) if useful
5. pick smallest remaining that meets structure needs
6. if empty → solo; suggest a size class to pull (ask user)
7. export OLLAMA_WORKER_MODEL=<exact name from list>
8. if model has thinking: default --think=false for bulk
```

## Cascade (one step)

On verify `fail` after one tighter packet:

1. Next stronger **installed** chat model
2. Else → orchestrator solo

## Hardware sanity

- Slow/thrashing → drop tier or shrink shards.
- JSON keep failing on a tiny model → cascade once, don’t spin.
- When comparing two installed models, prefer the smaller download/params that still passes verify (`ollama list` sizes).

## Report line

`model=<exact> tier=<t> reason=<smallest fit | warm | cascade | solo> think=<on|off>`
