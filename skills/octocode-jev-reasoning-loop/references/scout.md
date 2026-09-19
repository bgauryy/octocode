# Scout — typed read-prioritization

Load when many candidate files or rows might hold a capability and reading them all spends host context the task does not need. A scout converts "read N files" into one typed Jev judgment per candidate plus reads of only the confirmed few; candidate bytes never enter the host context.

`scripts/scout.mjs` gathers every anchor-matched span per candidate server-side (sandboxed, redacted, ≤12 spans, budget-bounded), sends ONE request with a per-candidate **Score** question over an ordered taxonomy, and maps distributions to `read | skip | gray_read`. A scout **prioritizes reads; it never authorizes an action and never becomes evidence.** Every verdict is `provisional: true` with span anchors; reopen anchors before asserting, and never report absence from a skip alone. Prefer the native `jevScout` tool when available; the skill runner is the standalone alternative. `scripts/code-scout.mjs` accepts a question and candidate paths, `scripts/pr-triage.mjs` accepts fetched history rows, and `scripts/scout.mjs` accepts a full input packet. Presets `taxonomy: "implements"|"relevance"` replace inline level lists.

## Gate — when NOT to scout

Read directly when you must read the file regardless (you are about to edit it), when a lexical or exact check settles it, or when candidates number under ~4 or you expect to read nearly all. Scouting helps when the cost of avoided reads exceeds request and orchestration overhead; a candidate-count threshold alone does not establish that.

## Taxonomy, items, dimensions

Default levels `none → mentions → imports → implements` make the classic trap (naming or importing without defining) an explicit answer; supply custom ordered levels for other spectra. Criteria and instructions are structured JSON — pass exclusions as fields (`distinguish: "importing is NOT implementing"`). `items: [{id, content, source?}]` replaces `candidates`+`anchors` for pre-fetched rows (unique IDs; `itemSpanBudget` 200–8000, default 3000). `dimensions: [{key, role, claim, levels}]` (1–4, ≤24 questions) sends several courts over the same shared state; exactly one `primary` drives the action, a `veto` court may only demote read → `gray_read`, `info` reports. Combine vectors in deterministic host code, never another model.

## Read policy

The current runner maps no anchor matches to `skip`, a top-level argmax to `read`, P(top) ≤ 0.25 to `skip`, and other results to `gray_read`. These are local policy defaults, not universal probability thresholds. Read both `read` and `gray_read` candidates; the strict `reads` list omits the latter, so inspect per-candidate actions.

Coverage is reported but not enforced by the JavaScript skip policy. Treat a missing anchor or insufficient excerpt as unresolved and widen retrieval before relying on rejection. A skip is never proof of absence. Validate false-skip rates on held-out tasks before automating this policy; compare the combined read/gray-read class when small probability changes cross that boundary.

## Run

`node scripts/scout.mjs --input scout.json [--dry-run]`. Input: local `{claim, anchors[], candidates[2..12], root?, levels?, window?, spanBudget?}` or `{claim, items[2..12], itemSpanBudget?, levels?}`; either may add `dimensions`. Output per candidate: `action`, `level`, `score`, `probabilities`, `coverage`, `anchors`, `provisional: true`. Confidence is distribution concentration, never correctness — treat a soft distribution as `gray_read`, not a weak yes. One scout per candidate set; a second pass needs a changed locate, not a repeat vote. Feed files you then read into `contentRef` evidence and the normal GATE.
