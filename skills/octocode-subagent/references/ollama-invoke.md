# Ollama Invoke

Load when running the worker scripts, calling HTTP directly, or hitting serving gotchas. Why: the invoke knobs (keepalive, format, ctx) decide whether output is parseable at all.

For the CLI command catalog load `references/ollama-cli.md` first; for which model to pass load `references/model-selection.md`.

**Prerequisites:** Ollama installed and reachable (`ollama serve` or OS service); at least one **chat-capable** model installed (`ollama list`); selected model exported as `OLLAMA_WORKER_MODEL` (exact name from list); `OLLAMA_HOST` default `http://127.0.0.1:11434`.

**Health check** — daemon only: `./scripts/ollama-health.sh` · after ROUTE (model selected): `./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"` · manual: `curl -sS "${OLLAMA_HOST:-http://127.0.0.1:11434}/api/tags"` or `ollama list`.

## Invoke

```bash
# default
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --job summarize \
  --input /path/to/shard.txt --schema /path/to/schema-hint.txt \
  --out .octocode/worker/shard-001.json

# JSON / structured jobs (prefer low temperature via HTTP)
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --format-json --temperature 0.2 \
  --keepalive 5m --job extract --input shard.txt --out .octocode/worker/shard-001.json
```

The script builds a constrained prompt, calls `ollama run` (CLI), or `/api/generate` when `--temperature` / `--num-ctx` / `--http` is set, and writes `--out` when set. Default `--keepalive 5m` keeps the model warm across shards. <!-- style-lint: ignore-line passive-voice -->

## Server best practices

| Knob | Default / prefer | Why |
|---|---|---|
| `keepalive` / `--keepalive` | `5m` (script default) | Avoid cold reload between shards; pass `0` after tier switch to free VRAM |
| `format=json` + schema in prompt | Always for extract/classify/check | Ollama requires instructing JSON in the prompt even when `format` is set | <!-- style-lint: ignore-line passive-voice -->
| `temperature` | `0.1–0.3` for structured; model default OK for draft/caption | Creative defaults (for example Gemma `1.0`) hurt JSON fidelity |
| `num_ctx` | ≥ shard tokens + headroom | Undersized ctx **silently truncates** the start of the prompt |
| One model per batch | Prefer `ollama ps` warm hit | Swapping large models mid-job thrashes RAM/VRAM |
| `--think=false` | Bulk / map-reduce | Thinking burns tokens without helping sealed packets |

## Server caveats and artifacts

1. **Exact model id** — `llama3.2` must not match `llama3.2-vision`.
2. **Context too small** — raise through `--num-ctx`. Symptom: truncated JSON.
3. **Quantization too aggressive** — structured output dies first; cascade tier.
4. **Embed/OCR models** — not workers for text jobs.
5. **Tool-calling** — out of scope; no agent loops on Ollama.
6. **Missing keepalive** — always keep explicit keepalive for map-reduce.
7. **Vision + HTTP** — `--temperature` / `--num-ctx` force HTTP and are **not** combined with `--image`; use CLI for vision.

**Artifact directory:** write under `.octocode/worker/` (create if needed). Do not commit secrets.

Next: raw CLI/HTTP details in `references/ollama-cli-run.md` (prefer these scripts for agents); check the return with `references/verify-gate.md`.
