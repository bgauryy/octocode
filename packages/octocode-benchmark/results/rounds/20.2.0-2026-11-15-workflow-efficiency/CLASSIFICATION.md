# Semantic scouting and source verification

The measured improvement is a narrower verification workflow, not a universal classification win. Jev identified useful candidates before the first source read, but produced four false positives in this small corpus. Reading candidate outlines and deciding blocks reduced completed output by 23.1%; an expert exact-search workflow still returned less text.

This experiment used the pre-fix local 20.2.0 binary, SHA256 `bfe156c11c87901b89ab6387efd778b1b864d0178e114fe36790a22d22be502b`, and requested `jev-1.14.0`. The first scout had not read the eleven source bodies or evaluator gold. Questions distinguished provider refusal/retry, cancellation, and batching/coalescing from saved reads. The later outline refinement was source-informed; it is not a second blind trial. Fresh-candidate acceptance belongs to the round's final report.

| Completed approach | Provider calls | Input tokens | Returned characters |
|---|---:|---:|---:|
| Initial file scout plus full candidate verification | 11 | 39,249 | 55,156 |
| Same scout plus outlines and matchString/block reads | 11 | 39,249 | 42,392 |
| Expert exact Octocode discovery and deciding reads | 0 | 0 | 24,545 |
| Expert lean rg and exact reads | 0 | 0 | 25,319 |
| Same questions applied through sections, with verification | 54 | 94,109 | 127,398 |

Octocode values count compact JSON characters; the lean rg value counts its text output. They are output volumes, not token measurements. Exact baselines were selected after learning the sources, so they are strong expert rivals rather than blinded search trials. The initial scout alone took 0.715 seconds in the recorded run; this one observation does not establish end-to-end or tail-latency superiority.

| Question family | Precision | Missed positives in this corpus |
|---|---:|---:|
| Refusal versus retry | 3/4 | 0 |
| Explicit cancellation | 1/1 | 0 |
| Batching/coalescing versus saved reads | 3/6 | 0 |

False positives included reporting and validation code mistaken for implementations. High confidence did not remove that distinction. Sections added six unknown cells and more verification work; surrounding context sometimes made a section appear to implement its neighbor's mechanism.

Across the main trial, sections, replay and adversarial probes, 68 live requests reported 138,492 input tokens and 12,425 output tokens. At the checked price of $0.042 per million input tokens and free output, that is approximately $0.00582; it is a list-price calculation, not an invoice. Jev evaluates independent questions against the same ingested state, making question batching useful. Long context is documented as slower and less accurate, so a larger window is not automatically the better route. [TypeSafe models](https://docs.typesafe.ai/models), [System One API](https://docs.typesafe.ai/api/system-one).

Use exact tools first when they can decide the scope. For unread semantic candidates, pass source paths or resources, batch independent mechanism questions, distinguish implementation from declarations/validators/reports, and include an `unknown` choice label. Verify the returned source read; when it is much larger than the deciding evidence, inspect symbols and then read a matching block or range. Keep local source unchanged and GitHub reads pinned to their commit. Use sections for claims that need every occurrence or ordering, and inspect unknowns before claiming absence.

Guardrail probes observed a fresh unchanged evaluation billing again, saved pages replaying without new inference, a changed source rejecting an old read with `staleSnapshot`, one hostile source instruction failing to steer the answer, and an incomplete source producing unknown. These cases do not establish general prompt-injection resistance or model calibration.

Raw receipts, source hashes, question definitions, evaluator gold, failure attempts and complete accounting are retained in [the campaign evidence](../../../campaigns/20.2.0-2026-11-15-workflow-efficiency/classification/REPORT.md) and [metrics](../../../campaigns/20.2.0-2026-11-15-workflow-efficiency/classification/metrics.json). No benchmark driver is part of the package.
