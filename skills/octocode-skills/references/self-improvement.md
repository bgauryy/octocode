# Self-Improvement Mode

Load when the user asks to rate, review, improve, or refactor a skill. Why: match the work to the requested mode without assuming write authority.

For the rewrite contract (lobby, dedupe, ≤50, review), load `references/skill-improve.md` — this file only gates the mode.

## Choose the mode

```text
1. Rate/review — score and report issues; no edits.
2. Improve/refactor — inspect, patch, and verify when the request authorizes edits.
3. Apply prior findings — use the current conversation's rating; do not repeat it without a reason.
```

## Rate-only report

```text
Overall:     <score>/10 — <grade> — <one sentence>
Score card:  trigger/workflow/evidence/gates/UX/specificity/portability/risk → High|Med|Low
Issues:      Critical / High / Medium / Low — each with file:line
Strengths:   2-4 bullets to preserve
Residual:    1-3 risks
Next:        relevant action(s), if any
```

Run `scripts/skill-review.mjs` first (`references/skill-review.md`). Cite findings with `references/skill-review-rules.md`.

Ask only when the request leaves write authority or the desired outcome materially unclear. For edits, follow `references/skill-improve.md`; do not restate its loop here.

Next: when rewriting load `references/skill-improve.md`; when presenting load `references/output-format.md`.
