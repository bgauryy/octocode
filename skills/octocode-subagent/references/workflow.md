# Workflow — GATE → ROUTE → RUN → VERIFY → REPORT

Load when running the full local-offload loop (beyond the lobby summary). Why: each phase owns a gate, and a skipped one fails silently.

**1. GATE** — `./scripts/ollama-health.sh` → `ollama list` → `ollama show MODEL_NAME` when size/capabilities unclear → `ollama ps` to prefer already-warm for small tasks. Confirm low-risk and worth offload. For articles: source text already saved (or fetch it yourself) before invoke. Gate fail → stay solo.

**2. ROUTE** — load `references/model-selection.md` (mandatory); load `references/usage-matrix.md` / `references/decision-matrix.md` / `references/family-playbooks.md` only when needed. **Do not** load `references/ollama-local-models.md` on routine routing. Job → tier → smallest fitting installed chat model → prefer warm → skip embedders → `--think=false` for bulk.

| Complexity | Volume | Action |
|---|---|---|
| High | Any | Orchestrator only |
| Low | Large | Offload |
| Low | Small | Offload OK — prefer warm `small`/`balanced` |

**3. RUN** — jobs: `summarize | extract | classify | draft | map | check | vision | translate`. Serving knobs (`keepalive`, `--format-json` + schema, `--temperature 0.2` for structured, `num_ctx` vs shard size): `references/ollama-invoke.md`. Long pages: shard → map → orchestrator reduce.
```bash
./scripts/ollama-worker.sh --model "$OLLAMA_WORKER_MODEL" --think=false --keepalive 5m --job summarize \
  --input /path/to/shard.txt --schema /path/to/schema-hint.txt --out .octocode/worker/shard-001.json
```

**4. VERIFY** — load `references/verify-gate.md`. Never silent-accept. On fail: tighter packet **or** cascade once to stronger *installed* model **or** solo.

**5. REPORT**
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

**Default job patterns** — not an exhaustive list; see also `references/usage-matrix.md`. **Never local:** architecture, security, auth, web browse, image generation, final verified claims.

| Job | Local | Orchestrator |
|---|---|---|
| Summarize / article body | Draft + quotes | Fetch, substring-verify, merge |
| Extract / classify / check | JSON rows | Schema-validate, decide |
| Translate / vision caption | Emit | Spot-check fidelity / pixels |
| Draft / map-reduce | First pass / shards | Edit+tests / reduce |

Next: at ROUTE load `references/model-selection.md`; before integrating load `references/verify-gate.md`.
