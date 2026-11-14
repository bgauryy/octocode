# Tool ratings: 1–10

Use this rubric for new tool-rating rounds. Historical 1–5 scorecards retain their original scale;
do not double their values and call that a fresh evaluation. Each score needs question IDs,
the pinned binary, CLI/MCP evidence and an explicit tested scope.

| Score | Meaning |
|---|---|
| 10 | Excellent throughout the tested scope; edge cases and the strongest relevant competitor checked; no material issue found |
| 9 | Strong, correct, complete behavior with a small remaining cost or usability issue |
| 8 | Good behavior with clear, manageable tradeoffs |
| 7 | Useful and mostly smooth, with recurring friction or measurable overhead |
| 6 | Adequate, but important workflow or efficiency weaknesses |
| 5 | Mixed: works for a meaningful subset, with substantial limitations |
| 4 | Weak: a consequential failure or repeated recovery is required |
| 3 | Frequently unreliable in the selected tasks |
| 2 | Barely usable; most deciding tasks fail |
| 1 | Unusable or unsafe for the tested purpose |

Correctness and completeness gate efficiency: omitted evidence never earns a cost win. An unresolved
wrong answer, silent evidence loss or unreachable required page caps the overall score at 4 until
fixed and independently retested. An unavailable provider is an environment limitation, not proof
of a tool defect; mark affected dimensions unverified and the overall score provisional.

| Dimension | Weight | Required evidence |
|---|---:|---|
| Correctness | 30% | Deciding source or independent oracle; counts, identity and selected scope |
| Schema/input | 15% | Current schema/description, valid request, near miss and actionable rejection |
| Output | 15% | Useful first answer, provenance, honest partial/empty state, unnecessary repetition |
| Pagination | 10% | Complete page reconstruction and executable unchanged continuations, or justified NA |
| Workflow | 10% | Question → discovery → deciding evidence; calls, recovery and CLI/MCP parity |
| Efficiency | 15% | Same selected evidence against the lean alternative; input/output characters, calls, first/repeat time |
| Cache/freshness | 5% | Repeat-state distinction and relevant edit/ref/invalidation behavior, or justified NA |

Overall = weighted mean of measured applicable dimensions, rounded to one decimal. Report measured
weight and confidence beside it. NA means genuinely inapplicable, not skipped; normalize its weight
out. Unverified dimensions leave the score provisional. Publish the underlying dimensions and one
sentence explaining both the strength and the main deduction; no unexplained composite or forced
criticism. Separate historical evidence, new observations and recommendations.

Confidence is high when both surfaces, required continuations, an oracle and the deciding rival
were checked; medium when a material surface or dimension has limited coverage; low when the
positive path is unverified. A confidence label describes this round, not the entire product.

Specific prompts belong in [questions/tools](questions/tools/README.md); shared runtime checks
belong in [CHECKS.md](CHECKS.md). Preserve every failed or invalid attempt in the raw campaign;
exclude invalid measurements from paired efficiency statistics and explain the exclusion.
