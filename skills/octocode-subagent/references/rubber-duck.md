# Rubber Duck
Load when a plan, diagnosis, or design needs verbal challenge without new research. Why: forcing restatement surfaces hidden assumptions cheaper than a full critic swarm.

## Pattern
Spawn a **listener** worker (minimal or no tools) whose only job is to hear the parent’s explanation and poke holes.

## Packet (downlink)
- `goal`: rubber-duck the following claim/plan; do not solve the original task
- `context`: short brief of the problem + the explanation to stress-test (not the whole chat)
- `scope`: ask clarifying questions; restate in your own words; list assumptions and failure modes
- `acceptance`: returns restatement, gaps, 3–7 pointed questions, confidence
- `return`: `restatement` · `assumptions` · `gaps` · `questions` · `next`

## Rules
1. Duck does **not** inherit parent tools/chat — independence is the point.
2. Parent must answer the duck’s questions or mark them deferred before shipping.
3. Agreement (“looks fine”) without restatement → failed duck; re-ask.
4. If duck finds a load-bearing gap, fix in parent, or spawn a real verifier with anchors — do not treat duck prose as evidence.

## When NOT
- Need external facts/code proof → `octocode-research` or verifier with tools.
- Need blunt code critique → `octocode-roast`.
- Already have independent anchor tests → run those first.

## Variants
- **Self-duck (no spawn):** parent writes restatement + assumptions in-chat once; upgrade to a real duck when risk is high.
- **Duo duck:** two ducks with different lenses (for example security vs UX) — still parent adjudicates.

Next: escalate claims → `references/interview.md`; merge → `references/synthesize.md`.
