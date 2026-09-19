# Scout — typed read-prioritization

Load when many candidate files or rows might hold a capability and reading them all spends host context the task does not need. A scout converts "read N files" into one typed Jev judgment per candidate plus reads of only the confirmed few; candidate bytes never enter the host context.

`scripts/scout.mjs` gathers every anchor-matched span per candidate server-side (sandboxed, redacted, ≤12 spans, budget-bounded), sends ONE request with a per-candidate **Score** question over an ordered taxonomy, and maps distributions to `read | skip | gray_read`. A scout **prioritizes reads; it never authorizes an action and never becomes evidence.** Every verdict is `provisional: true` with span anchors; reopen anchors before asserting, and never report absence from a skip alone. Prefer the native `jevScout` tool when available (same contract, native redaction/sandbox), and prefer packet-free entry points — structured tool calls or `--input` files — over hand-authoring inline JSON: authoring nine packets in agent context measured 33k extra agent tokens versus a zero-authoring driver. Presets `taxonomy: "implements"|"relevance"` replace inline level lists. This runner is the reference implementation, fallback, and parity source (`.octocode/rfc/jev-scout-production/`).

## Gate — when NOT to scout

Read directly when you must read the file regardless (you are about to edit it), when a lexical or exact check settles it, or when candidates number under ~4 or you expect to read nearly all. Scouting wins only when the skip fraction exceeds roughly `q/b` (q ≈ 40 tokens per scout row; b ≈ tokens per avoided read) — one batched request amortizes shared state; a single-candidate judgment is insurance, not savings.

## Taxonomy, items, dimensions

Default levels `none → mentions → imports → implements` make the classic trap (naming or importing without defining) an explicit answer; supply custom ordered levels for other spectra. Criteria and instructions are structured JSON — pass exclusions as fields (`distinguish: "importing is NOT implementing"`). `items: [{id, content, source?}]` replaces `candidates`+`anchors` for pre-fetched rows (unique IDs; `itemSpanBudget` 200–8000, default 3000). `dimensions: [{key, role, claim, levels}]` (1–4, ≤24 questions) sends several courts over the same shared state; exactly one `primary` drives the action, a `veto` court may only demote read → `gray_read`, `info` reports. Combine vectors in deterministic host code, never another model.

## Frozen policy v2 and thresholds

`no anchor matches → skip` · `argmax = top level → read` · `P(top) ≤ 0.25 → skip` · otherwise `gray_read` (fail-open). Coverage gates trusting a skip, never a read. Do not tune thresholds against a suite evaluating them; changes need fresh held-out confirmation (`.octocode/octocode-eval-benchmark/jevpeek-scout/`: 0.19–0.49× host reads, zero false-skips on those suites, Brier ≤ 0.007; the evolved fan-out suite later produced the first false-skips — evidence for the next held-out round, not for tuning). Near-threshold candidates flip read ↔ gray_read across provider samples on identical packets — gate parity on the read|gray_read class, never exact actions.

## Run

`node scripts/scout.mjs --input scout.json [--dry-run]`. Input: local `{claim, anchors[], candidates[2..12], root?, levels?, window?, spanBudget?}` or `{claim, items[2..12], itemSpanBudget?, levels?}`; either may add `dimensions`. Output per candidate: `action`, `level`, `score`, `probabilities`, `coverage`, `anchors`, `provisional: true`. Confidence is distribution concentration, never correctness — treat a soft distribution as `gray_read`, not a weak yes. One scout per candidate set; a second pass needs a changed locate, not a repeat vote. Feed files you then read into `contentRef` evidence and the normal GATE.
