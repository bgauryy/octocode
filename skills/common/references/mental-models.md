# Mental models for decisions and issue discovery

Load when a hidden assumption, uncertain choice, misleading metric or consequential failure needs a better question. These lenses select checks; they are neither proof nor a checklist to recite. Keep the domain skill's scope, permissions and output.

## Choose the lens

Ask what is missing, what could go wrong, what follows, what would change the conclusion, and whether a simpler approach works. Select only questions whose answers could change the next action; scale effort to the consequence.

| Lens | Deciding question → check |
|---|---|
| First Principles / Occam / Via Negativa | Which requirement needs this complexity? Compare the simplest viable path against the same acceptance, safety and recovery checks; establish consumers before removal. |
| Inversion / Pre-Mortem / Red Teaming | How could this fail despite a convincing happy path? Trace a reachable failure and test the strongest rival that would make the suspected defect harmless. |
| Systems / Second-Order Thinking | What happens when consumers react? Trace downstream state, retries, queues and caches; check whether cost or risk merely moved. |
| Black Swan / Tail Risk | Which environmental assumption could fail with severe consequences? Perturb inputs, ordering or dependencies in isolation; test containment. Known edge cases are not automatically Black Swans, and finite tests cannot cover unknown events. |
| Optionality | Which reversible step buys deciding evidence? Check restoration and effects rollback cannot undo; avoid delaying settled work. |
| Antifragility | What would turn this failure into validated improvement? Keep a reproducer and test the regression or policy change on different cases too. Recovery alone is resilience. |
| Bayesian / Knightian Uncertainty | Which observation separates leading explanations, and where does the decision change? Update from independent evidence; vary unsupported assumptions and leave unjustified probabilities unknown. |
| Goodhart / Opportunity Cost | Could the score improve while the user outcome worsens? Hold acceptance and quality guardrails constant; include omissions, retries and the work displaced. |
| OODA | What changed, and which next action resolves it? Observe, orient, decide, act, compare. After a non-informative repeat, change the hypothesis or method. |
| Lindy | Does a maintained proven approach satisfy the workload? Compare under current constraints; age alone proves neither correctness nor fit. |

## Uncertainty, reversibility and survival

Name the observation that would invalidate the current understanding. When it appears, refresh evidence or change the hypothesis before the affected action; continue unaffected authorized work. Ask only for a missing decision or authority that evidence cannot supply.

Track task success and failure containment separately. A faster outcome cannot offset violating authorization, security or integrity. Keep impact, likelihood and confidence distinct; no observed severe failure does not imply zero risk. Use the owner's limits rather than invented quotas.

Before consequential transitions or resumed work, reconcile stale inputs, pending effects and uncertain outcomes before retrying. Verify recovery against actual state and disclose its limits.

## Issue-hunting loop

1. Frame the intended outcome, invariant, scope and observable acceptance.
2. Name a failure hypothesis, challenged assumption, strongest rival and smallest authorized check that distinguishes them. A hypothetical consequence sets priority, not severity as a proven finding.
3. Exact-read the deciding source; resolve identity/callers when needed. Runtime claims require execution evidence. Record expected and observed results with source or run identity.
4. Mark confirmed, refuted or unresolved; follow new evidence. For an authorized repair, fix the cause and replay the reproducer plus affected checks. Reviews and plans hand off proposals within their scope.
5. Close on verified acceptance or a named blocker; state unchecked scope. A design scenario walk remains inferred or untested. Report conclusions and evidence without private deliberation.

## Turn a lens into a check

- A faster research agent reads fewer pages: reconstruct the same requested evidence before comparing time.
- A write acknowledgment is lost: inspect durable state and reconcile before retrying; test duplicate-effect containment in isolation.
- A dependency is proposed for an uncertain workload: compare a reversible prototype with the existing approach and name the observation that would justify adoption.

## Concept boundaries and sources

These are engineering applications, not evidence that naming models improves agents.

- [Taleb: The Black Swan, prologue](https://www.fooledbyrandomness.com/TheBlackSwan.pdf)
- [Taleb: Antifragile](https://www.fooledbyrandomness.com/Antifragile.htm)
- [Google DeepMind: Specification gaming](https://deepmind.google/blog/specification-gaming-the-flip-side-of-ai-ingenuity/)
