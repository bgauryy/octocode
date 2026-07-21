# octocode-orchestrator-local-worker

Cloud orchestrator + local Ollama worker: **delegate execution, retain reasoning**.

## What it does

Offloads low-risk sealed packets (summarize, extract, classify, translate, draft, checklist, vision caption, map-reduce) to an installed Ollama chat model. The cloud agent keeps tools, fetch, verify, and final writes.

## When to use

- Token/context pressure on bulk or already-fetched article bodies
- User asks for local/Ollama/"save tokens"/dual-model
- Small warm-model jobs (translate, caption, one-file summarize)

## When not to use

Architecture, security, tool-using loops, worker web browse, image generation, Ollama install/setup, or full local agent bridges.

## How it works

`GATE → ROUTE → RUN → VERIFY → REPORT`

```bash
./scripts/ollama-health.sh
ollama list   # pick exact tag
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --keepalive 5m \
  --format-json --job summarize --input shard.txt --out .octocode/worker/out.json
```

Structured jobs: add `--temperature 0.2`. See `references/ollama-invoke.md`.

## Eval suite (not temp)

`evals/` is the **permanent** harness: `cases.json`, `fixtures/`, `kpi-contract.json`, `loop-report.md`.

**Temp / do not commit:** run outputs under `.octocode/` (repo-gitignored) including `.octocode/orchestrator-local-worker/evals/last-report.json`. Never treat `evals/` as disposable.

## Install

Copy or symlink this folder into your agent skills root (e.g. `~/.claude/skills/` or project `.agents/skills/`). Requires Ollama + at least one chat-capable model.

```bash
npx octocode skill --add --path . --platform claude,cursor,agents --mode symlink
```

