# Interview
Load when another agent’s output must be challenged before the parent trusts it. Why: MAST inter-agent misalignment (withholding, derailment, weak verification) survives polite merge.

## Pattern
Parent (or a fresh **interviewer** worker) asks structured questions against a sealed claim set — like a design review, not a chat replay.

## Packet shapes
**To interviewer (preferred fresh worker):**
- `goal`: interview the subject claims; try to falsify load-bearing ones
- `context`: subject’s `result` + ≤8 claimed anchors only — **no** subject transcript
- `scope`: question → demand evidence → mark confirmed/contested/unknown
- `acceptance`: every load-bearing claim has a status; contradictions listed
- `return`: `questions_asked` · `claim_table` · `contradictions` · `verdict` · `next`

**To subject (optional re-interview):**
- Only the interviewer’s questions + original acceptance criteria
- Forbid “defend your prose”; require anchors or concede

## Question bank (pick what flips the verdict)
1. What would falsify this?
2. Which anchor did you actually open/run?
3. What did you skip or not check?
4. Where could two agents disagree?
5. What is the smallest counterexample?
6. If the user is wrong about X, how does your answer change?

## Rules
1. Interviewer starts **fresh** — never shares the subject’s chain-of-thought dump.
2. Parent re-checks contested anchors itself.
3. “We agree” without new anchors → REVERT trust; keep claim `uncertain`.
4. Cap rounds (default 1–2). Steer once; then stop + parent.
5. Lateral subject↔interviewer chat stays **off** unless parent relays.

## When NOT
- No claims yet → gather evidence first.
- Pure rubber-duck of parent’s own plan → `references/rubber-duck.md`.
- Need measurable ACCEPT/REVERT on the graph → `octocode-eval-benchmark`.

Next: instruction borrowing → `references/mimic-flow.md`; barrier merge → `references/synthesize.md`.
