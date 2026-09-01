# Ollama CLI run reference

Load when invoking the worker or debugging CLI run behavior. Why: `run` flag shape (and what it does not expose) decides between CLI and HTTP.

## Run (worker invoke)

```bash
ollama run MODEL [PROMPT] [flags]
```

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

Gemma 4 library sampling defaults (Modelfile/API): `temperature=1.0`, `top_p=0.95`, `top_k=64` — already present on local `gemma4:*`, as shown by `ollama show --parameters`. For JSON extract/classify, override with `--temperature 0.2`.

### Non-interactive patterns (preferred for agents)

```bash
ollama run "$MODEL" "Summarize the following: ..."   # prompt as argument
ollama run "$MODEL" < packet.txt                     # stdin (what scripts/ollama-worker.sh uses)
ollama run --format json "$MODEL" < packet.txt        # force JSON
```

Use non-interactive invocations; REPL sessions and TTY chat loops do not fit sealed worker packets.

## HTTP equivalents

When CLI is awkward, same host:

```bash
curl -sS "$OLLAMA_HOST/api/tags"           # ≈ list
curl -sS "$OLLAMA_HOST/api/ps"             # ≈ ps
curl -sS "$OLLAMA_HOST/api/generate" -d '{...}'
curl -sS "$OLLAMA_HOST/api/chat" -d '{...}'
```

Prefer skill scripts for generate/chat.

Next: script invoke and serving knobs in `references/ollama-invoke.md`; full command catalog in `references/ollama-cli.md`.
