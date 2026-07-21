# Ollama CLI Reference

Load when inventorying models, invoking the worker, or debugging CLI behavior.
Commands and flags verified against local `ollama --help` / subcommand help (Ollama CLI on the agent host).

Official docs: https://github.com/ollama/ollama · library hub: https://ollama.com/library  
Key families: [gemma4](https://ollama.com/library/gemma4) · [qwen2.5](https://ollama.com/library/qwen2.5)

Gotcha: `ollama -v` may show a **client** build different from the **server** (`ollama serve`). Prefer live `run`/`show` behavior; upgrade the CLI if flags diverge.

## Global

```bash
ollama --help
ollama -v          # version
```

Env (common):

| Variable | Meaning |
|---|---|
| `OLLAMA_HOST` | Server address (default `127.0.0.1:11434`) |
| `OLLAMA_WORKER_MODEL` | Exact model name this skill selected (skill-specific) |
| `OLLAMA_WORKER_KEEPALIVE` | Default keepalive for `ollama-worker.sh` (default `5m`) |

## Lifecycle

| Command | Use |
|---|---|
| `ollama serve` | Start server (often already running as a service) |
| `ollama pull MODEL` | Download a model — **ask user before pulling** |
| `ollama rm MODEL` | Delete a local model — **ask user** |
| `ollama cp SRC DST` | Copy/rename a local model |
| `ollama create NAME -f Modelfile` | Build custom model from Modelfile |
| `ollama stop MODEL` | Stop a running model |
| `ollama signin` / `signout` | ollama.com auth (not required for local run) |

## Inventory (required every offload)

```bash
ollama list          # alias: ollama ls
ollama ps            # models currently loaded in memory
ollama show MODEL
ollama show MODEL --parameters
ollama show MODEL --system
ollama show MODEL --modelfile
ollama show MODEL --template
ollama show MODEL -v # verbose
```

**Agent rules**

- Copy model names **exactly** from `ollama list` (include `:tag`).
- Use `show` before selecting an unfamiliar model (size, context, capabilities).
- `ps` helps avoid loading a huge model when a small one is already warm — optional optimization, not a substitute for tier fit.

## Run (worker invoke)

```bash
ollama run MODEL [PROMPT] [flags]
```

Useful flags:

| Flag | When |
|---|---|
| `--format json` | Extract/classify jobs that require JSON (**also** instruct JSON in the prompt) |
| `--verbose` | Timing / debug slow runs |
| `--keepalive 5m` | **Required for map-reduce** — keep model loaded across shards (`0` unloads) |
| `--think=false` | **Default for bulk shards** — must be one argv (`--think=false`), not `--think false` |
| `--think=true` | Enable thinking when the packet needs deeper reasoning |
| `--hidethinking` | Hide thinking spans if the model emits them |
| `--nowordwrap` | Cleaner capture for scripts |

`ollama run` does **not** expose `temperature` / `num_ctx` as flags — use `scripts/ollama-worker.sh --temperature` / `--num-ctx` (HTTP `/api/generate`) or a Modelfile.

Gemma 4 library sampling defaults (Modelfile/API): `temperature=1.0`, `top_p=0.95`, `top_k=64` — already present on local `gemma4:*` via `ollama show --parameters`. For JSON extract/classify, override with `--temperature 0.2`.

### Non-interactive patterns (preferred for agents)

```bash
# prompt as argument
ollama run "$MODEL" "Summarize the following: ..."

# prompt on stdin (what scripts/ollama-worker.sh uses)
ollama run "$MODEL" < packet.txt

# force JSON
ollama run --format json "$MODEL" < packet.txt
```

MUST NOT use interactive REPL sessions for this skill (no TTY chat loops).

## HTTP equivalents

When CLI is awkward, same host:

```bash
curl -sS "$OLLAMA_HOST/api/tags"           # ≈ list
curl -sS "$OLLAMA_HOST/api/ps"             # ≈ ps
curl -sS "$OLLAMA_HOST/api/generate" -d '{...}'
curl -sS "$OLLAMA_HOST/api/chat" -d '{...}'
```

Prefer skill scripts for generate/chat. See `ollama-invoke.md`.

## Safe defaults for this skill

```bash
# 1) health (daemon only)
./scripts/ollama-health.sh

# 2) inventory
ollama list

# 3) select model → export
export OLLAMA_WORKER_MODEL='<exact-tag-from-ollama-list>'

# 4) verify model resolves (exact match)
./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"

# 5) run worker
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --job summarize --input shard.txt --out .octocode/worker/out.txt
```

## Do / Don’t

| Do | Don’t |
|---|---|
| `ollama list` then pick | Hardcode `llama3.2` without checking |
| Exact tagged names | Prefix-match (`llama3.2` ≠ `llama3.2-vision`) |
| `--format json` + schema text for structured jobs | Ask embed models to summarize |
| `--keepalive` on every shard invoke | Rely on accidental warm loads |
| Size shards to `num_ctx` (+ headroom) | Stuff a huge page into default ctx and hope |
| One model per map-reduce job | Swap 7B↔32B mid-shard set (VRAM thrash) |
| Ask before `pull` / `rm` | Download multi-GB models silently |
