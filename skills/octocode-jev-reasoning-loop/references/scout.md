# Scout — typed read-prioritization

Load when many candidate files might hold a capability and reading them all would
spend host context the task does not need. Why: a scout converts "read N files"
into one typed Jev judgment per candidate plus reads of only the confirmed few;
the candidate bytes never enter the host context.

## What a scout is — and is not

A scout is `LOCATE → JUDGE`: `scripts/scout.mjs` gathers every anchor-matched
span per candidate server-side (sandboxed to the root, redacted, ≤12 spans,
bounded chars), sends ONE Jev request with a per-candidate **Score** question
over an ordered relationship taxonomy, and maps the returned distributions to
`read | skip | gray_read`.

A scout **prioritizes reads. It never authorizes an action and never becomes
evidence.** Every verdict is `provisional: true` and carries its span anchors;
reopen the anchors with a real read before asserting anything a scout suggested.
Never report absence ("X is not implemented here") from a scout skip alone.

## Gate — when NOT to scout

- You will read the file regardless (you are about to edit it): read directly.
- A lexical or exact check settles it (grep, known path, symbol lookup): use it.
- Fewer than ~4 candidates, or you expect to read nearly all of them: the peek
  is overhead. Scouting wins only when selectivity is real — skip fraction
  greater than roughly `q/b`, where `q` ≈ tokens per scout row (~40) and `b` ≈
  tokens per avoided file read.

## The taxonomy is the discriminator

The default Score levels — `none → mentions → imports → implements` — make the
classic trap (a file that names or imports a capability without defining it) an
explicit answer instead of a false positive. Supply custom ordered levels when
the question has a different spectrum. Criteria and instructions are structured
JSON, not prose: pass exclusions and distinctions as fields
(`distinguish: "importing is NOT implementing"`).

## Items mode and multi-dimension courts

`items: [{id, content, source?}]` replaces `candidates`+`anchors` when the host
already holds cheap rows (PR/commit/issue titles from a search): the scout
judges the rows so only the top items get their expensive diffs opened.
`dimensions: [{key, role, claim, levels}]` (1–4, ≤24 questions total) sends
several independent courts over the SAME shared state in one request — spans
amortize; each answer is independent, so the vector is combined by
deterministic host code, never another model. Exactly one `primary` drives the
action via frozen policy v2; a `veto` court at its bottom level may only demote
a read to `gray_read` (forces more reading, never creates a skip); `info`
courts report. Live precedent: 8 PRs × {relevance, is_fix} = 16 questions, one
call, one correct read.

## Frozen policy v2

`no anchor matches → skip` · `argmax = top level → read` ·
`P(top) ≤ 0.25 → skip` · otherwise `gray_read` (fail-open). Coverage gates
trusting a skip, never a read. Do not tune these thresholds against a suite that
is evaluating them; policy changes require a fresh held-out confirmation
(precedent: `.octocode/octocode-eval-benchmark/jevpeek-scout/`, where v2 frozen
pre-suite gave C=0.37× read-everything, 0.49× lexical prefilter, zero
false-skips across TypeScript and Rust, Brier 0.001).

## Run

```sh
node scripts/scout.mjs --input scout.json --dry-run   # build + inspect the packet
node scripts/scout.mjs --input scout.json             # one live batched judgment
```

Input: `{ claim, anchors[], candidates[2..12], root?, levels?, window?, spanBudget? }`.
Output per candidate: `action`, taxonomy `level`, `score`, `probabilities`,
`coverage`, `anchors`, `provisional: true`. Confidence is distribution
concentration, never correctness — act on probability mass, and treat a soft
distribution as `gray_read`, not as a weak yes.

## Relation to the reasoning loop

A scout is upstream triage: it decides *what evidence to fetch*, while the loop
routes *judgment over fetched evidence*. A scout verdict never substitutes for a
route call — feed the files you actually read into `contentRef` evidence and the
normal GATE. One scout per candidate set; a second pass needs a changed locate
(new anchors or wider window), not a repeat vote.
