# Blind Review
Load when a critic must judge the **artifact**, not the author’s story. Why: shared rationale creates MAST-style “agreeing in a different font.”

## Pattern
Spawn a fresh reviewer with only the deliverable + acceptance criteria. Strip identity, rationale, and peer chat.

## Packet
- `goal`: blind review this artifact against acceptance
- `context`: artifact only (diff, doc, report, packet `result`) + acceptance checklist
- `scope`: pass/fail each criterion; demand missing anchors; no coaching the author
- `acceptance`: criterion table with pass|fail|unknown + required fixes
- `return`: `criteria` · `blockers` · `nits` · `verdict` · `next`

## Citation scrub (variant)
When publishing research-like output (Anthropic research systems use a citation pass):
- Give scrubber the report + source list/snippets only
- Require every load-bearing claim → concrete source location
- Fail claims with no cite; do not invent sources

## Rules
1. No author transcript, no “why we did it,” no peer opinions.
2. Parent re-checks failed criteria on real anchors.
3. If reviewer asks for rationale, parent may supply **facts/anchors** only — not persuasion.
4. Blind review ≠ rubber duck (duck hears the explanation; blind must not).

## When NOT
- Need the author to explain thinking → `references/rubber-duck.md`.
- Need adversarial kill-shots on a plan → `references/red-team.md`.
- Need measurable ACCEPT on a graph → `octocode-eval-benchmark`.

Next: consensus retries → `references/consensus.md`; techniques → `references/techniques.md`.
