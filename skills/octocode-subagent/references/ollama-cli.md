# Ollama CLI Reference

Load when inventorying models, invoking the worker, or debugging CLI behavior. Why: exact tags and lifecycle flags decide whether an offload runs at all.

Commands and flags verified against local `ollama --help` / subcommand help (Ollama CLI on the agent host). Official docs: https://github.com/ollama/ollama · library hub: https://ollama.com/library · key families: [gemma4](https://ollama.com/library/gemma4) · [qwen2.5](https://ollama.com/library/qwen2.5). Gotcha: `ollama -v` may show a **client** build different from the **server** (`ollama serve`) — prefer live `run`/`show` behavior and upgrade the CLI if flags diverge.

**Global:** `ollama --help` · `ollama -v` (version). Env:

| Variable | Meaning |
|---|---|
| `OLLAMA_HOST` | Server address (default `127.0.0.1:11434`) |
| `OLLAMA_WORKER_MODEL` | Exact model name this skill selected (skill-specific) |
| `OLLAMA_WORKER_KEEPALIVE` | Default keepalive for `scripts/ollama-worker.sh` (default `5m`) |

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

**Inventory (required every offload)**
```bash
ollama list          # alias: ollama ls
ollama ps            # models currently loaded in memory
ollama show MODEL    # add --parameters | --system | --modelfile | --template | -v
```

**Agent rules:** copy model names **exactly** from `ollama list` (include `:tag`); use `show` before selecting an unfamiliar model (size, context, capabilities); `ps` helps avoid loading a huge model when a small one is already warm — an optional optimization, not a substitute for tier fit.

**Safe defaults for this skill:** 1) Health: `./scripts/ollama-health.sh` 2) Inventory: `ollama list` 3) Select: `export OLLAMA_WORKER_MODEL='<exact-tag>'` 4) Verify: `./scripts/ollama-health.sh --model "$OLLAMA_WORKER_MODEL"` 5) Run: `./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --job summarize --input shard.txt --out .octocode/worker/out.txt`

## Do / Don't

| Do | Don't |
|---|---|
| `ollama list` then pick | Hardcode `llama3.2` without checking |
| Exact tagged names | Prefix-match (`llama3.2` ≠ `llama3.2-vision`) |
| `--format json` + schema text for structured jobs | Ask embed models to summarize |
| `--keepalive` on every shard invoke | Rely on accidental warm loads |
| Size shards to `num_ctx` (+ headroom) | Stuff a huge page into default ctx and hope |
| One model per map-reduce job | Swap 7B↔32B mid-shard set (VRAM thrash) |
| Ask before `pull` / `rm` | Download multi-GB models silently |

Next: for `ollama run` flags, non-interactive patterns, and HTTP equivalents load `references/ollama-cli-run.md`; for the script path and serving knobs load `references/ollama-invoke.md`.
