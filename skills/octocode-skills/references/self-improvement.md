# Self-Improvement Mode

Load when the user asks to rate, review, score, improve, refactor, or review a skill and the mode is unclear. Why: pick Rate-only vs Improve before editing.

For the rewrite contract (lobby, dedupe, ≤50, review), load `references/skill-improve.md` — this file only gates the mode.

## Mode gate

```text
1. Rate-only — score + issues; no edits.
2. Improve / refactor — fix; gate before write (then skill-improve.md).
3. Fix all — apply a prior rating in this chat; skip re-rate.
4. Cancel.
```

## Rate-only report

```text
Overall:     <score>/10 — <grade> — <one sentence>
Score card:  trigger/workflow/evidence/gates/UX/specificity/portability/risk → High|Med|Low
Issues:      Critical / High / Medium / Low — each with file:line
Strengths:   2-4 bullets to preserve
Residual:    1-3 risks
Next:        numbered choices ending with Cancel
```

Run `scripts/skill-review.mjs` first (`references/skill-review.md`). Cite findings with `references/skill-review-rules.md`.

Improve/Fix-all: after the gate, follow `references/skill-improve.md` end-to-end; do not restate its loop here.

Next: when rewriting load `references/skill-improve.md`; when presenting load `references/output-format.md`.
