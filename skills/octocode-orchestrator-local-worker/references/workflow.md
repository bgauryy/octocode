# Workflow — GATE → ROUTE → RUN → VERIFY → REPORT

Load for the full offload loop (beyond the lobby summary).

## 1. GATE

```bash
./scripts/ollama-health.sh
ollama list
ollama show <MODEL>   # when size/capabilities unclear
ollama ps             # prefer already-warm for small tasks
```

Confirm low-risk and worth offload. For articles: source text already saved (or fetch yourself) before invoke. Gate fail → stay solo.

## 2. ROUTE

Load `model-selection.md` (mandatory). Load `usage-matrix.md` / `decision-matrix.md` / `family-playbooks.md` only when needed. **Do not** load `ollama-local-models.md` on routine routing.

| Complexity | Volume | Action |
|---|---|---|
| High | Any | Orchestrator only |
| Low | Large | Offload |
| Low | Small | Offload OK — prefer warm `small`/`balanced` |

Job → tier → smallest fitting installed chat model → prefer warm → skip embedders → `--think=false` for bulk.

## 3. RUN

```bash
./scripts/ollama-worker.sh \
  --model "$OLLAMA_WORKER_MODEL" \
  --think=false --keepalive 5m \
  --job summarize \
  --input /path/to/shard.txt \
  --schema /path/to/schema-hint.txt \
  --out .octocode/worker/shard-001.json
```

Jobs: `summarize | extract | classify | draft | map | check | vision | translate`.

Serving knobs: see `ollama-invoke.md` (`keepalive`, `--format-json` + schema, `--temperature 0.2` for structured, `num_ctx` vs shard size).

Article schema: `evals/fixtures/schema-article-summarize.txt`. Long pages: shard → map → orchestrator reduce.

## 4. VERIFY

Load `verify-gate.md`. Never silent-accept. On fail: tighter packet **or** cascade once to stronger *installed* model **or** solo.

## 5. REPORT

```text
Offload: <job> → ollama/<exact-model> (tier: …) [size: small|large|article]
Why this model: <inventory reason; warm?>
Shards: <n> | Verify: pass|fail|partial | Grounded: <rate if article>
Kept on orchestrator: <fetch, merge, final claims, …>
```

## Recovery

| Failure | Action |
|---|---|
| Ollama down / no fitting model | Solo; report / suggest size class |
| Truncated / empty | Shrink shard or `--num-ctx`; retry once |
| Invalid JSON | `--format-json` + `--temperature 0.2`; else cascade/solo |
| Cold shards | `--keepalive`; prefer `ollama ps` warm |
| Ungrounded quotes / bad paths | Discard; cascade or orchestrator redo |

## Default job patterns

Not an exclusive whitelist. See also `usage-matrix.md`.

| Job | Local | Orchestrator |
|---|---|---|
| Summarize / article body | Draft + quotes | Fetch, substring-verify, merge |
| Extract / classify / check | JSON rows | Schema-validate, decide |
| Translate / vision caption | Emit | Spot-check fidelity / pixels |
| Draft / map-reduce | First pass / shards | Edit+tests / reduce |

**Never local:** architecture, security, auth, web browse, image generation, final verified claims.
