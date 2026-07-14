# Perspective Review

Load after research/cross-pollination to challenge the evidence and select the strongest defensible decision. Return synthesis, not a transcript; the main agent is referee.

## Inputs

Provide one packet: `user + painful situation + desired outcome + success signal + assumptions`, chosen framings, hypothesis/claim ledger, evidence anchors, and research limits. If audience, problem, or success is missing, stop at Clarify.

## Lenses

Run together only when the worker budget permits; otherwise run sequentially. Each returns at most three rows: `claim -> because -> evidence -> decision impact -> confidence`.

| Lens | Challenge |
|---|---|
| Critical Architect | feasibility, integration/blast radius, security/performance/maintenance, hardest technical unknown |
| Visionary Entrepreneur | urgency, wedge, strategic value, differentiation, distribution, upside |
| Product | workflow, adoption friction, scope razor, retention/value signal, smallest decision-changing test |

## Evidence And Cross-Exam

- Drop or mark `weak` every uncited new claim, including market/user claims.
- Use ledger evidence instead of raw snippets. State assumptions when a follow-up would leave the decision unchanged.
- Pick the 1-2 claims most likely to flip the verdict. Ask only relevant lenses for new evidence; repeating a citation is not rebuttal.
- Every rebuttal states its concessions. If budget ends, run a short referee pass and report the shortened review.

## Referee

Keep claims that survived, remove concessions, and mark unresolved claims as decision points. Record the decision delta: what flipped, stayed contested, had stronger evidence, and changed the verdict.

| Decision | Meaning |
|---|---|
| Build RFC | ready for design tradeoffs and a bounded RFC handoff |
| Prototype First | prove one hard unknown before design |
| Narrow | choose a tighter user/problem/framing |
| Park | timing or evidence is weak |
| Do Not Build | solutions or risks dominate |

An RFC handoff needs a worth-prototyping/underserved verdict; specific user, problem, and success; grounded prior art; and a bounded first slice. The largest unknown must be a design tradeoff rather than demand.
