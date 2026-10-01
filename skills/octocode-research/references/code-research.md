# Code research

Load for consequential code claims (callers, imports, cycles, reachability, deletion, architecture) or a Map/Validate/Investigate/Plan run.

| Mode | Chain |
|---|---|
| Map | literal + synonyms → repos/packages → tree/search → exact finalist reads → clusters |
| Validate | reframe/invert → local first → external evidence → Advocate/Critic → build, narrow, or drop |
| Investigate | structure → symptom search → exact boundary reads → graph/LSP/AST → history/tests |
| Plan / Architecture | contract → entry points → graph/LSP affected scope → exact boundaries → safest step/tradeoffs |

State the surface plan (local, GitHub, packages, PR/history, docs/web) and why each skipped surface is skipped.

## Proof ladder
`candidate → exact evidence → claim-specific corroboration → verification → verdict`. AST, LSP, and graph are alternatives or complements, never a compulsory chain.

| Finding | Minimum corroboration |
|---|---|
| dead export / safe delete | graph `issues` or `deadCode` candidate + LSP `includeDeclaration:false` + text/AST across code, tests, configs, docs + runtime registrations + public-API/external-consumer check |
| cycle | graph `cycles` + exact imports; type-only vs runtime |
| affected scope | graph `impact`/`dependents`/`path` + exact reads + LSP references/callers |
| security sink | sink shape + exact read + sources/callers + guard check |
| test gap | changed symbol + no test references + nearby test read |
| performance | exact hot path; benchmark only when runtime proof matters |

- Local deps, errors, and config feed external queries; upstream fixes return to local proof.
- Advocate/Critic: strongest cited case for and against; rebut the claim most likely to flip the decision.
- Compress large outputs into `claim → evidence → confidence → next`.

Before answering: corpus/ref and skipped surfaces stated; continuations followed or declared unnecessary; syntax, semantic, history, artifact, and runtime proof distinguished; local `path:line` and remote URL/PR/commit cited; `confirmed`/`likely`/`uncertain` with checks run and not run.

Next: an edit → `workflow-change.md`; a claim that keeps flipping → `campaigns.md`.
