# Local Ollama

Load when you save tokens with a local Ollama worker while the parent keeps tools and judgment, and before you invoke the worker or integrate its output. Delegate execution, retain reasoning. This is not a Task or A2A spawn.

## Hard rules

1. Architecture, security, auth, design, final synthesis, and repository writes stay on the parent unless write ownership transfers.
2. Worker output is untrusted. VERIFY before you integrate; never silent-accept.
3. Prefer deterministic tools (`rg`, tests, formatters, typecheckers, repo scripts) or a configured small cloud model when they suffice.
4. No tool-using loops on Ollama: single-shot or map-reduce only.
5. Health-check before the first invoke (`scripts/ollama-health.sh`). If it is down, stay solo.
6. Use exact names from live `ollama list`; never invent tags. Named tags in this skill are examples.
7. The worker never browses. The parent fetches; the worker sees saved text or images only.
8. No image generation. Vision jobs describe provided images only.

## Decide

- High complexity (design, security, auth, contested): parent only. Medium (multi-file edit with judgment): parent; an optional local draft of notes is OK. Work that needs tools (MCP, shell loop): parent or host subagent, never Ollama.
- Low complexity, large volume, no tools: offload. Small volume: offload to a warm model; stay solo if local is cold and the requester did not ask for local.
- Offload only when all hold: the input is large enough to hurt the parent; the schema is mechanically verifiable; wrong answers are cheap to detect; latency plus verify is acceptable.

| Surface | Local? | How | Parent keeps |
|---|---|---|---|
| Research, web browse | No | Parent fetches | Discovery, ranking, citations |
| Article summarize | After fetch | `support_quote` schema; shard long pages | Fetch, quote verify, merge |
| Code summarize, extract | Yes | Per-file shards, then merge | Correctness, tests, security |
| Code draft, tests | First draft | Tight schema | Final code, green tests |
| Classify, triage | Yes | Closed label set | Priority decisions |
| Translate | Yes | Schema + fidelity spot-check | Tone, high-stakes languages |
| Checklist | Yes | Pass/fail rows | Acting on fails |
| Vision caption | Yes | `--job vision --image`, describe only | Spot-check against pixels |
| Image generation, architecture, security, auth | Never | — | Everything |

## Loop: GATE → ROUTE → RUN → VERIFY → REPORT

1. **GATE**: `scripts/ollama-health.sh` → `ollama list` → `ollama show` if size or capability is unclear → `ollama ps` to prefer a warm model for small work. Confirm low risk and worth. Save article text first. Gate fail: stay solo.
2. **ROUTE**: reuse a fitting selected model, or select with `references/model-selection.md`. Default `--think=false` for bulk.
3. **RUN**: write the packet below; invoke per `references/ollama-cli.md`. Long pages: shard → map → parent reduce.
4. **VERIFY**: run the gate below. On fail: one tighter packet, or one cascade to a stronger installed model, or solo.
5. **REPORT**: `Offload: <job> → ollama/<exact-model> (tier) [small|large|article] · Why: <inventory reason; warm?> · Shards: <n> · Verify: pass|fail|partial · Grounded: <rate> · Kept on parent: <fetch, merge, final claims>`.

## Packet

The worker inherits no chat; the packet is its whole world.

```text
GOAL:          <one sentence>
JOB:           summarize | extract | classify | draft | map | check | vision | translate
MODEL:         <exact name from ollama list>
INPUT:         <paths + line ranges preferred, or inline slices>   IMAGE: <optional, vision only>
OUTPUT_SCHEMA: <strict JSON schema or bullet fields>
CONSTRAINTS:   no tools, no shell, no web; do not invent files, APIs, line numbers, or images;
               if unsure, emit null / "unknown" and say why
ACCEPTANCE:    <machine-checkable rules> + <1–2 parent spot-check rules>
RETURN:        stdout | file:.octocode/worker/<id>.json
```

Prompt header: `You are a local worker. Complete only the JOB. Obey CONSTRAINTS. Return ONLY valid output matching OUTPUT_SCHEMA. No markdown fences unless asked.` Then fill GOAL, JOB, MODEL, INPUT, OUTPUT_SCHEMA, ACCEPTANCE.

- Summarize: `{path, summary (≤120 words), key_symbols[], risks[], confidence: high|medium|low}`.
- Extract: `{rows[{path, symbol, kind: function|class|route|config, notes}], unknowns[]}`.
- Classify: `{items[{id, label: bug|chore|risk|question, reason (≤40 words), confidence}]}`.
- Translate: `{source_lang, target_lang, translation, notes[]}`.
- Article: `{title, tldr, key_points[], claims[{claim, support_quote}], confidence}`; `support_quote` is a verbatim contiguous INPUT substring.

Article flow: the parent saves `SOURCE_URL` plus plain text (~2–8k chars per shard) → worker `--job summarize` → parent checks quotes → map-summarize long pages, parent reduces. Tiny ≤3B models often fail fidelity.

Anti-patterns: vague goals; asking the worker to fix, run tests, or open URLs; text the parent cannot re-check; free prose when JSON was required; structured jobs without `--format-json`, schema text, and temperature 0.1–0.3; packets larger than `num_ctx` (shard instead).

## Verify gate (all must pass)

1. **Parse**: output matches the schema; JSON parses; required fields exist.
2. **Ground**: every cited path exists. Drop or redo rows with invented paths.
3. **Scope**: nothing outside JOB (no "also refactored", no tool talk).
4. **Spot-check**: open 1–2 source slices; the output is not contradicted.
5. **Confidence**: `low` rows are unknowns unless the parent re-verifies.
6. **Grounding**: each `support_quote` is a contiguous INPUT substring after whitespace normalization. Drop ungrounded claims. Require grounded_rate = 1.0 before integrate; else cascade once.

Verdicts: `pass` → integrate · `partial` → keep good shards or rows, redo or solo the rest · `fail` → one tighter re-packet or escalation, never silent accept.

- After one failed retry on a shard, the parent does that shard. If >30% of shards fail, stop offload for this job and finish solo.
- Security, auth, or design content in worker output: discard and redo on the parent.
- Cascade only to installed chat models; never invent a stronger tag.
- Quote worker facts only with parent-confirmed anchors. Do not claim confidence above the spot-check. Tell the requester what you offloaded and whether you verified it fully or partly.
