# Code research and root cause

Load for consequential code claims (callers, imports, cycles, reachability, deletion, architecture), a Map/Validate/Investigate/Plan run, or a root cause: a violated supported contract or an unknown symptom.

| Mode | Chain |
|---|---|
| Map | literal + synonyms → repos/packages → tree/search → exact finalist reads → clusters |
| Validate | reframe/invert → local first → external evidence → Advocate/Critic → build, narrow, or drop |
| Investigate | structure → symptom search → exact boundary reads → graph/LSP/AST → history/tests |
| Plan / Architecture | contract → entry points → graph/LSP affected scope → exact boundaries → safest step/tradeoffs |

Pick surfaces (local, GitHub, packages, PR/history, docs/web) by the claim; name a skipped surface when the claim spans it.

## Proof ladder
`candidate → exact evidence → claim-specific corroboration → verification → verdict`. Safe delete: lobby `PROVE` rule.

| Finding | Minimum corroboration |
|---|---|
| cycle | graph `cycles` + exact imports; type-only vs runtime |
| affected scope | graph `impact`/`dependents`/`path` + exact reads + LSP references/callers |
| security sink | sink shape + exact read + sources/callers + guard check |
| test gap | changed symbol + no test references + nearby test read |
| performance | exact hot path; benchmark only when runtime proof matters |

- Local deps, errors, and config feed external queries. Upstream fixes return to local proof.
- Advocate/Critic: cite the strongest case for and against. Rebut the claim most likely to flip the decision.

## Root cause
```text
contract: actual + expected + authority + trigger + impact
-> reproduction or equivalent runtime evidence + symptom anchor
-> entry -> transformations -> state/dependencies -> output/consumers
-> two hypotheses: likely mechanism + plausible alternate
-> first boundary where actual diverges; exact reads there
-> AST/LSP/history/tests for reachability and "why now"
-> disconfirm the alternate; counterfactual: removing the cause removes the symptom
```

Report: `Root cause · Violated contract · Evidence (path:line / runtime) · Disconfirmation · Why now · Fix · Verification`.

Before answering: state corpus/ref; follow continuations or declare them unnecessary; separate syntax, semantic, history, artifact, and runtime proof; cite local `path:line` and remote URL/PR/commit; label `confirmed`/`likely`/`uncertain` with checks run and not run.

Next: an edit or fix → `workflow-change.md`; upstream mechanism → `workflow-external.md`; a claim that keeps flipping or tied hypotheses → `campaigns.md`.
