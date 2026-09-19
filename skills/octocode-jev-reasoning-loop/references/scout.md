# Scout — typed read-prioritization

Load when many candidate files or rows might hold a capability and reading them all spends host context the task does not need. A scout converts "read N files" into one typed Jev judgment per candidate plus reads of only the confirmed few; candidate bytes never enter the host context.

`scripts/scout.mjs` gathers every anchor-matched span per candidate server-side (sandboxed, redacted, ≤12 spans, budget-bounded), sends ONE request with a per-candidate **Score** question over an ordered taxonomy, and maps distributions to `read | skip | gray_read`. A scout **prioritizes reads; it never authorizes an action and never becomes evidence.** Every verdict is `provisional: true` with span anchors; reopen anchors before asserting, and never report absence from a skip alone. Prefer the native `jevScout` tool when available; the skill runner is the standalone alternative. `scripts/code-scout.mjs` accepts a question and candidate paths, `scripts/pr-triage.mjs` accepts fetched history rows, and `scripts/scout.mjs` accepts a full input packet. Presets `taxonomy: "implements"|"relevance"` replace inline level lists.

## Gate — when NOT to scout

Read directly when you must read the file regardless, a lexical or exact check settles it, or you expect to read nearly all candidates. Scout when the avoided reads justify the request and orchestration overhead.

## Taxonomy, items, dimensions

For native `jevScout`, use `source.local` for file candidates or `source.items` for fetched rows. Set `taxonomy: "relevance"` for history rows; omission uses `implements` (`none → mentions → imports → implements`). One preset needs no `dimensions` array. Use `dimensions` for custom or multiple judgments, without a top-level `taxonomy`: exactly one `primary` drives the action, `veto` may only demote read → `gray_read`, and `info` reports. Order custom levels lowest to highest; the final level triggers read. The skill runner's input shape is listed below.

## Read policy

The current runner maps no anchor matches to `skip`, a top-level argmax to `read`, P(top) ≤ 0.25 to `skip`, and other results to `gray_read`. These are local policy defaults, not universal probability thresholds. Read both `read` and `gray_read` candidates; the strict `reads` list omits the latter; native `requiredReads` includes both.

The compact `code-scout.mjs` output exposes `required_reads` (read plus gray_read), per-candidate probabilities, anchors and coverage, the resolved model, and artifact paths. Use `--model` to pin comparisons and `--output` to retain a run. Low coverage calls for a wider read; it is not a confidence measure.

Budget truncation is explicit: a rejected excerpt with omitted characters or spans becomes `gray_read` with reason `incomplete_excerpt`. Whole-file coverage remains advisory; an untruncated anchor window still may not cover the deciding code. Widen retrieval before relying on rejection. A skip is never proof of absence. Validate false-skip rates on held-out tasks before automating this policy; compare the combined read/gray-read class when small probability changes cross that boundary.

## Run

`node scripts/scout.mjs --input scout.json [--dry-run]`. Input: local `{claim, anchors[], candidates[2..12], root?, taxonomy?, levels?, window?, spanBudget?}` or `{claim, items[2..12], itemSpanBudget?, taxonomy?, levels?}`; either may add `dimensions`. Output per candidate: `action`, `level`, `score`, `probabilities`, `coverage`, `truncated`, `anchors`, `provisional: true`. Apply the read policy above; confidence is distribution concentration, not correctness. A second pass needs changed evidence, not a repeat vote. Reopen selected sources before using them in a claim or reasoning judgment.
