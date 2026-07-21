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

JSON jobs:

```bash
./scripts/ollama-worker.sh \
  --model "$OLLAMA_WORKER_MODEL" \
  --format-json \
  --job extract \
  --input shard.txt \
  --out .octocode/worker/shard-001.json
```

The script builds a constrained prompt, calls `ollama run` non-interactively, writes `--out` when set.

## Raw CLI / HTTP

See `references/ollama-cli.md`. Prefer the script for agents.

## Serving gotchas

1. **Wrong / fuzzy model id** — `llama3.2` must not match `llama3.2-vision`. Exact list name only.
2. **Context too small** — raise via Modelfile `PARAMETER num_ctx` or API `options.num_ctx`. Symptom: truncated JSON.
3. **Quantization too aggressive** — structured output dies before chat quality; cascade tier.
4. **Embed/OCR models** — not workers for text jobs.
5. **Tool-calling** — out of scope; no agent loops on Ollama.

## Artifact directory

Write under `.octocode/worker/` (create if needed). Do not commit secrets. Ignore `.octocode/` in git if missing.
