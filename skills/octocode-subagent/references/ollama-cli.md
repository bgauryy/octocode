# Ollama CLI and Invoke

Load when you inventory models, invoke the worker, or debug CLI or serving behavior. Exact tags and serving knobs decide whether output parses.

Docs: https://github.com/ollama/ollama · https://ollama.com/library. `ollama -v` can show a client build that differs from the server; trust live `run`/`show` behavior and upgrade the CLI if flags diverge.

Env: `OLLAMA_HOST` (default `127.0.0.1:11434`) · `OLLAMA_WORKER_MODEL` (exact selected name) · `OLLAMA_WORKER_KEEPALIVE` (script default `5m`).

- Inventory: `ollama list` (`ls`) · `ollama ps` (loaded; warmth never overrides tier fit) · `ollama show MODEL` (`--parameters`, `--system`, `--modelfile`, `--template`, `-v`) for unfamiliar models.
- Lifecycle: `serve` (often already a service) · `pull` · `rm` (**ask the requester first**) · `cp SRC DST` · `create NAME -f Modelfile` · `stop MODEL` · `signin`/`signout` (not needed locally).
- Run: `ollama run MODEL [PROMPT]`, non-interactive only: prompt argument or `< packet.txt` (stdin, what the script uses). No REPL or TTY loops.

| Run flag | When |
|---|---|
| `--format json` | JSON jobs; also instruct JSON in the prompt |
| `--keepalive 5m` | Required for map-reduce; `0` unloads (after a tier switch, to free VRAM) |
| `--think=false` | Default for bulk; one argv (`--think false` makes `false` the model name) |
| `--think=true` / `--hidethinking` | Deeper reasoning / hide thinking spans |
| `--verbose` / `--nowordwrap` | Timing debug / cleaner capture |

`ollama run` has no `temperature` or `num_ctx` flag: use the script's `--temperature` / `--num-ctx` (HTTP `/api/generate`) or a Modelfile. HTTP: `$OLLAMA_HOST/api/tags` (list), `/api/ps`, `/api/generate`, `/api/chat`; prefer the skill scripts.

```bash
./scripts/ollama-health.sh                                   # daemon only
./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"    # after ROUTE
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --think=false --keepalive 5m --job summarize \
  --input shard.txt --schema schema-hint.txt --out .octocode/worker/shard-001.json
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --format-json --temperature 0.2 \
  --keepalive 5m --job extract --input shard.txt --out .octocode/worker/shard-001.json
```

The script builds a constrained prompt and calls `ollama run`, or `/api/generate` when `--temperature`, `--num-ctx`, or `--http` is set; it writes `--out` when set. Keep artifacts under `.octocode/worker/`; never commit secrets.

- `temperature` 0.1–0.3 for structured jobs; the model default suits draft and caption.
- One model per map-reduce job; swapping 7B↔32B mid-set thrashes RAM/VRAM. `--temperature` / `--num-ctx` force HTTP and do not combine with `--image`; use the CLI for vision.

| Failure | Action |
|---|---|
| Truncated or empty (undersized `num_ctx` silently cuts the prompt start) | Shrink the shard or raise `--num-ctx` above shard tokens plus headroom; retry once |
| Invalid JSON (aggressive quantization breaks it first) | `--format-json` + `--temperature 0.2`; else cascade a tier or solo |
| Cold shards | `--keepalive` |

Next: gate the return with `references/local-ollama.md`.
