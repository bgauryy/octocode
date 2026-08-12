# Family Playbooks (optional examples)

Load only when:

- An **installed** family needs special invoke flags (thinking, vision order, sampling), or
- Two installed families both fit and you want example tie-breaks.

**Not required.** Do not treat tags below as must-install. Always start from live `ollama list`.
Portable routing lives in [model-selection.md](./model-selection.md).
Catalog / RAM kits: [ollama-local-models.md](./ollama-local-models.md).

Sources for examples: [gemma4](https://ollama.com/library/gemma4), [qwen2.5](https://ollama.com/library/qwen2.5), and other library families as they appear on the machine.

## Example preference patterns (if those families are installed)

| Job pattern | Prefer-first *pattern* | Then | Avoid |
|---|---|---|---|
| Classify / tiny triage | Smallest instruct (≤3B) | Small edge multimodal | embed, OCR-as-chat |
| Summarize / extract JSON | Mid instruct strong at JSON (~7–9B) | Mid multimodal / other mid instruct | thinking-on bulk without need |
| Draft code / tests | Mid–strong instruct or `*coder*` | Larger instruct | Older sibling if newer installed |
| Cascade after verify fail | Next larger installed chat model | — | Skipping verify |
| Vision caption | Any installed `vision` model | Other multimodal | Text-only models |
| OCR scans | OCR-specialized tags | High-res vision multimodal | Tiny text models |

If neither Gemma nor Qwen is installed, ignore brand columns entirely — use size/capability tiers only.

## Gemma 4 notes (when `gemma4:*` is installed)

Useful for reasoning, coding drafts, multimodal caption. Still: **no tool loops** on the worker path.

| Example tag | Role | Typical ctx |
|---|---|---|
| `gemma4:e2b` / `e4b` | Edge / small | ~128K |
| `gemma4:12b` | Mid workstation | ~256K |
| `gemma4:26b` / `31b` | Strong local | ~256K |
| `gemma4:latest` | Alias — confirm with `ollama show` | varies |
| `gemma4:*-cloud` | Cloud — only if user opts in | — |

Library sampling often ships as `temperature=1.0`, `top_p=0.95`, `top_k=64`.

**Thinking:** for bulk shards prefer off:

```bash
ollama run --think=false "$MODEL" < packet.txt
# never: ollama run --think false MODEL  (false becomes the model name)
```

**Vision:** put image before text; variable vision token budgets (70–1120) for detail vs speed.

```bash
# example only — use a tag from YOUR ollama list
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --think false --job draft --input shard.txt
```

## Qwen notes (when `qwen*:` is installed)

Often strong at instruction following, structured/JSON extract, and classify. Context and tags vary by generation (`qwen2.5` ~32K; newer Qwen3.x often longer — trust `ollama show`).

```bash
# example only — low temp for JSON fidelity
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" \
  --format-json --temperature 0.2 --keepalive 5m \
  --job extract --input shard.txt
```

## Other families

Llama, Granite, DeepSeek, Mistral, GPT-OSS, coder specialists, etc. — if installed, bucket by size + `ollama show` capabilities the same way. No special playbook required unless flags differ.

## Pull suggestions (ask user first)

When inventory lacks a usable mid-tier chat model, suggest a **class**, not a locked brand:

- ~7–12B general instruct, or
- a coder-oriented mid model, or
- a vision model if the job needs images

Example (only if user wants a concrete command): `ollama pull <tag-they-chose>`.  
Do not pull large (26B+) models without confirmed hardware headroom.
