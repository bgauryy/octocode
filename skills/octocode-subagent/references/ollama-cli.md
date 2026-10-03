# Ollama CLI and Invoke

Load when you inventory models, invoke the worker, or debug CLI or serving behavior. Exact tags and serving knobs decide whether output is parseable.

Docs: https://github.com/ollama/ollama · https://ollama.com/library. `ollama -v` can show a client build that differs from the server; trust live `run`/`show` behavior and upgrade the CLI if flags diverge.

Env: `OLLAMA_HOST` (default `127.0.0.1:11434`) · `OLLAMA_WORKER_MODEL` (exact selected name) · `OLLAMA_WORKER_KEEPALIVE` (script default `5m`).

## Commands

- Inventory (every offload): `ollama list` (`ls`) · `ollama ps` (loaded models) · `ollama show MODEL` (`--parameters`, `--system`, `--modelfile`, `--template`, `-v`). Inspect unfamiliar models with `show`. Use `ps` to prefer a warm fitting model; this never replaces tier fit.
- Lifecycle: `serve` (often already a service) · `pull` and `rm` (**ask the requester first**) · `cp SRC DST` · `create NAME -f Modelfile` · `stop MODEL` · `signin`/`signout` (not needed for local run).
- Run: `ollama run MODEL [PROMPT]`. Agents use non-interactive forms only: prompt argument or `< packet.txt` (stdin, what the script uses). No REPL or TTY loops.

| Run flag | When |
|---|---|
| `--format json` | JSON jobs; also instruct JSON in the prompt |
| `--keepalive 5m` | Required for map-reduce; `0` unloads (use after a tier switch to free VRAM) |
| `--think=false` | Default for bulk; one argv. `--think false` makes `false` the model name |
| `--think=true` / `--hidethinking` | Deeper reasoning / hide thinking spans |
| `--verbose` / `--nowordwrap` | Timing debug / cleaner script capture |
`ollama run` has no `temperature` or `num_ctx` flag. Use the script's `--temperature` / `--num-ctx` (HTTP `/api/generate`) or a Modelfile. HTTP equivalents: `$OLLAMA_HOST/api/tags` (list), `/api/ps`, `/api/generate`, `/api/chat`; prefer the skill scripts.

## Script

```bash
./scripts/ollama-health.sh                                   # daemon only
./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"    # after ROUTE
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --think=false --keepalive 5m --job summarize \
  --input shard.txt --schema schema-hint.txt --out .octocode/worker/shard-001.json
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --format-json --temperature 0.2 \
  --keepalive 5m --job extract --input shard.txt --out .octocode/worker/shard-001.json
```

The script builds a constrained prompt and calls `ollama run`, or `/api/generate` when `--temperature`, `--num-ctx`, or `--http` is set. It writes `--out` when set. Write artifacts under `.octocode/worker/`; never commit secrets.

## Server knobs and recovery

- `temperature` 0.1–0.3 for structured jobs; the model default is OK for draft and caption.
- One model per map-reduce job; swapping 7B↔32B mid-set thrashes RAM/VRAM. `--temperature` / `--num-ctx` force HTTP and do not combine with `--image`; use the CLI for vision.

| Failure | Action |
|---|---|
| Ollama down or no fitting model | Solo; report or suggest a size class |
| Truncated or empty (undersized `num_ctx` silently cuts the prompt start) | Shrink the shard or raise `--num-ctx` above shard tokens plus headroom; retry once |
| Invalid JSON (aggressive quantization breaks it first) | `--format-json` + `--temperature 0.2`; else cascade a tier or solo |
| Cold shards | `--keepalive`; prefer a warm model from `ollama ps` |
| Ungrounded quotes or bad paths | Discard; cascade or redo on the parent |

Next: gate the return with `references/local-ollama.md`.
