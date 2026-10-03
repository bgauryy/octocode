# Local Ollama

Load when you save tokens with a local Ollama worker while the parent keeps tools and judgment, and before you invoke the worker or integrate its output. Delegate execution, retain reasoning. This is not a Task or A2A spawn.

## Decide
- Prefer deterministic tools (`rg`, tests, formatters, typecheckers, repo scripts) or a configured small cloud model when they suffice.
- The parent fetches; the worker sees saved text or images.
- Complexity: high (design, security, auth, contested) → parent only; medium (multi-file edit with judgment) → parent, a local notes draft is OK; low with large volume → offload; low with small volume → a warm model, else solo unless the requester asked for local.
- Offload only when all hold: input large enough to hurt the parent; schema mechanically verifiable; wrong answers cheap to detect; latency plus verify acceptable.

| Surface | Local? | How | Parent keeps |
|---|---|---|---|
| Research, web browse | No | Parent fetches | Discovery, ranking, citations |
| Article summarize | After fetch | `support_quote` schema; shard long pages | Fetch, quote verify, merge |
| Code summarize, extract | Yes | Per-file shards, then merge | Correctness, tests, security |
| Code draft, tests | First draft | Tight schema | Final code, green tests |
| Classify, triage | Yes | Closed label set | Priority decisions |
| Translate | Yes | Schema + fidelity spot-check | Tone, high-stakes languages |
| Checklist | Yes | Pass/fail rows | Acting on fails |
| Vision caption | Yes | `--job vision --image`; describe provided images only | Spot-check against pixels |
| Image generation | Never | — | Everything |

## Loop
1. **GATE**: `scripts/ollama-health.sh` (down: stay solo) → `ollama list` → `ollama show` if size or capability is unclear → `ollama ps` for a warm model on small work. Confirm low risk and worth. Gate fail: solo.
2. **ROUTE**: reuse a fitting selected model, or select with `references/model-selection.md`.
3. **RUN**: write the packet; invoke per `references/ollama-cli.md`. Long pages: shard → map → parent reduce.
4. **VERIFY**: run the gate below; apply its verdict.
5. **REPORT**: `Offload: <job> → ollama/<exact-model> (tier) [small|large|article] · Why: <inventory reason; warm?> · Shards: <n> · Verify: pass|fail|partial · Grounded: <rate> · Kept on parent: <fetch, merge, final claims>`.

## Packet (the worker inherits no chat)
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
Prompt header: `You are a local worker. Complete only the JOB. Obey CONSTRAINTS. Return ONLY valid output matching OUTPUT_SCHEMA. No markdown fences unless asked.` Then GOAL, JOB, MODEL, INPUT, OUTPUT_SCHEMA, ACCEPTANCE.

- Summarize: `{path, summary (≤120 words), key_symbols[], risks[], confidence: high|medium|low}`.
- Extract: `{rows[{path, symbol, kind: function|class|route|config, notes}], unknowns[]}`.
- Classify: `{items[{id, label: bug|chore|risk|question, reason (≤40 words), confidence}]}`.
- Translate: `{source_lang, target_lang, translation, notes[]}`.
- Article: `{title, tldr, key_points[], claims[{claim, support_quote}], confidence}`; `support_quote` is a verbatim contiguous INPUT substring. Flow: parent saves `SOURCE_URL` plus plain text (~2–8k chars per shard) → `--job summarize` → parent checks quotes → map-summarize long pages, parent reduces. Tiny ≤3B models often fail fidelity.

Anti-patterns: vague goals; asking the worker to fix code, run tests, or open URLs; structured jobs without schema text.

## Verify gate (worker output is untrusted; all must pass; never silent-accept)
1. **Parse**: JSON parses and matches the schema with required fields.
2. **Ground**: every cited path exists; drop or redo rows with invented paths.
3. **Scope**: nothing outside JOB (no "also refactored", no tool talk).
4. **Spot-check**: open 1–2 source slices; nothing contradicted.
5. **Confidence**: `low` rows are unknowns unless the parent re-verifies.
6. **Grounding**: each `support_quote` is a contiguous INPUT substring after whitespace normalization; drop ungrounded claims; require grounded_rate = 1.0 before integrate.

Verdicts: `pass` → integrate · `partial` → keep good shards or rows, redo or solo the rest · `fail` → one tighter re-packet, one cascade (`references/model-selection.md`), or solo.
- One failed retry on a shard: the parent does that shard. >30% of shards fail: stop offload for this job; finish solo.
- Security, auth, or design content in output: discard; redo on the parent.
- Quote worker facts only with parent-confirmed anchors; claim no confidence above the spot-check.
