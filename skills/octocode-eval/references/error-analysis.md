# Error Analysis
Load before writing new eval cases or when the suite feels generic. Why: evals should come from real failure modes, not vanity metrics.

## Process (Hamel-style)
1. **Dataset** — gather representative traces (prod, dogfood, or synthetic starter).
2. **Open coding** — domain expert notes the *first* clear failure per trace (journaling).
3. **Axial coding** — cluster notes into a **failure taxonomy**; count frequency.
4. **Saturation** — stop when ~20 new traces add no new category (still review a meaningful batch).
5. **Write evals** — one grader/case family per top failure mode; attach `failureSignature`.

## Outputs
| Artifact | Use |
|---|---|
| Failure taxonomy | Prioritize what to measure |
| Top-N modes | Capability suite targets |
| Signatures | `mechanism:…\|cause:…` for mining / Awareness reflect |
| New cases | Suite loop growth |

## Rules
- Do not start from generic platform metrics (“toxicity”, “helpfulness”) unless they appear in your taxonomy.
- Frequency beats rarity — evals aren’t free; cover what actually happens.
- Revisit after product/model shifts; taxonomies rot.
- Upstream errors cause downstream noise — fix/tag the first break.

## Link to loops
Error analysis feeds the **suite loop**; experiments then hill-climb those cases; meta loop changes the program when the same signatures recur.

Next: add cases → `eval-harness.md`; choose public vs private benches → `benchmarking.md`.
