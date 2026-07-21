# Ollama Invoke

Load for skill scripts, HTTP invoke, and serving gotchas.
For CLI command catalog load `references/ollama-cli.md` first.
For which model to pass load `references/model-selection.md`.

## Prerequisites

- Ollama installed and reachable (`ollama serve` or OS service)
- At least one **chat-capable** model installed (`ollama list`)
- Selected model exported as `OLLAMA_WORKER_MODEL` (exact name from list)
- `OLLAMA_HOST` — default `http://127.0.0.1:11434`

## Health check

Daemon only (no model required):

```bash
./scripts/ollama-health.sh
```

After ROUTE (model selected), verify exact name:

```bash
./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"
```

Manual:

```bash
curl -sS "${OLLAMA_HOST:-http://127.0.0.1:11434}/api/tags"
ollama list
```

## Default invoke

```bash
./scripts/ollama-worker.sh \
  --model "$OLLAMA_WORKER_MODEL" \
  --job summarize \
  --input /path/to/shard.txt \
  --schema /path/to/schema-hint.txt \
  --out .octocode/worker/shard-001.json
```

JSON / structured jobs (prefer low temperature via HTTP):

```bash
./scripts/ollama-worker.sh \
  --model "$OLLAMA_WORKER_MODEL" \
  --format-json \
  --temperature 0.2 \
  --keepalive 5m \
  --job extract \
  --input shard.txt \
  --out .octocode/worker/shard-001.json
```

The script builds a constrained prompt, calls `ollama run` (CLI) or `/api/generate` when `--temperature` / `--num-ctx` / `--http` is set, writes `--out` when set.
Default `--keepalive 5m` keeps the model warm across shards.

## Serving best practices

| Knob | Default / prefer | Why |
|---|---|---|
| `keepalive` / `--keepalive` | `5m` (script default) | Avoid cold reload between shards; pass `0` after tier switch to free VRAM |
| `format=json` + schema in prompt | Always for extract/classify/check | Ollama requires instructing JSON in the prompt even when `format` is set |
| `temperature` | `0.1–0.3` for structured; model default OK for draft/caption | Creative defaults (e.g. Gemma `1.0`) hurt JSON fidelity |
| `num_ctx` | ≥ shard tokens + headroom | Undersized ctx **silently truncates** the start of the prompt |
| One model per batch | Prefer `ollama ps` warm hit | Swapping large models mid-job thrashs RAM/VRAM |
| `--think=false` | Bulk / map-reduce | Thinking burns tokens without helping sealed packets |

## Raw CLI / HTTP

See `references/ollama-cli.md`. Prefer the script for agents.

## Serving gotchas

1. **Wrong / fuzzy model id** — `llama3.2` must not match `llama3.2-vision`. Exact list name only.
2. **Context too small** — raise via `--num-ctx` (script HTTP path), Modelfile `PARAMETER num_ctx`, or API `options.num_ctx`. Symptom: truncated / ungrounded JSON with no error.
3. **Quantization too aggressive** — structured output dies before chat quality; cascade tier.
4. **Embed/OCR models** — not workers for text jobs.
5. **Tool-calling** — out of scope; no agent loops on Ollama.
6. **Missing keepalive on shards** — each invoke may cold-load; always keep explicit keepalive for map-reduce.
7. **Vision + HTTP options** — `--temperature` / `--num-ctx` force HTTP and are **not** combined with `--image`; use CLI for vision.

## Artifact directory

Write under `.octocode/worker/` (create if needed). Do not commit secrets. Ignore `.octocode/` in git if missing.
