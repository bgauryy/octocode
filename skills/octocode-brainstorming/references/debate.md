# Perspective Review — evidence challenge

Load at the converge step (Workflow step 5), after research and cross-pollination. The goal is not to create a transcript; it is to clarify the idea, pressure-test the evidence, and output the strongest defensible decision. The main agent is referee.

## Inputs

Before dispatch or sequential lenses, provide:

- Clarified idea: `user + painful situation + desired outcome + success signal + core assumptions`.
- Chosen framing(s), hypothesis map, claim ledger, and research limits.
- Prior-art evidence with URLs or local `file:line` anchors.

If the clarified idea is missing audience, problem, or success criterion, stop at Clarify. Do not spend workers to ask three versions of the same question.

## Roles

Dispatch together when worker budget allows; otherwise run them sequentially with these labels. Each role returns at most 3 claims in this shape: `claim -> because -> evidence -> decision impact -> confidence`.

> **Critical Architect** — feasibility and architecture risk. Test integration complexity, blast radius, security/performance/maintenance risk, hidden constraints, and the hardest technical unknown to prove.

> **Visionary Entrepreneur** — opportunity and wedge. Test why now, who urgently cares, strategic value, differentiation, distribution path, and what winning would unlock.

> **Product** — workflow and MVP. Test target workflow, adoption friction, scope razor, retention/value metric, and the smallest prototype or research step that changes the decision.

## Evidence rules

- No uncited new claims. Persona output without evidence is dropped or marked `weak`.
- Market/user claims from Visionary/Product still need sources or explicit `weak` markers.
- Use the claim ledger; do not repeat raw search snippets.
- A role may ask one targeted follow-up only if it changes the decision and the user is reachable; otherwise state the assumption.

## Cross-exam

After openings, the main agent picks the 1-2 claims most likely to flip the verdict. If budget remains, ask only the relevant role(s) to rebut those claims with new evidence. If budget is gone, do the rebuttal as a short main-agent pass and note `perspective review shortened (budget)`.

Rebuttal must concede what it cannot refute. Repeating the same citation is not a rebuttal.

## Referee / best-of-panel

Keep every claim that survived challenge, remove conceded claims, and mark unresolved claims as decision points. Record the **decision delta**: what flipped, what was conceded, what stayed contested, and which perspective had the strongest evidence.

Decision labels:

- `Build RFC` — idea is worth planning; prepare `octocode/octocode-rfc-generator` handoff.
- `Prototype First` — prove one hard unknown before design work.
- `Narrow` — choose a tighter user/problem/framing.
- `Park` — evidence is thin or timing is wrong.
- `Do Not Build` — existing solutions or risks dominate.

RFC handoff is ready only when the verdict is `worth-prototyping` or clearly `underserved`, the user/problem/success signal are specific, prior art/differentiation are grounded, the panel agrees on a bounded MVP or first implementation slice, and the biggest unknown is now an implementation/design tradeoff rather than demand.
