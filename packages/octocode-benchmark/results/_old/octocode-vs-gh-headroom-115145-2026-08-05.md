# Octocode vs `gh` + Headroom — campaign 115145

**Campaign start:** 2026-08-05T11:51:45Z  
**Runs:** 3 isolated passes per arm × 17 questions = 102 answers  
**Corpus:** immutable snapshot reused from campaign 104647 for direct repeatability  
**Headroom:** 0.33.0, `kompress-v2-base`, ONNX backend  
**Character unit:** Unicode code points from preserved, SHA-256-verified artifacts

## Verdict

**Octocode wins this complete, strict-valid campaign.** It won 29 of 51 blind
question pairings versus 22 for `gh` + Headroom, scored 508/510 correctness
versus 501/510, and used 800,586 delivered characters versus 2,119,615.
Octocode therefore used **62.2% fewer characters**, a **2.65× smaller** context
footprint.

## Aggregate result

| Arm | Correctness | Research | Workflow | Calls | Failed calls | Chars in | Pair wins |
|---|---:|---:|---:|---:|---:|---:|---:|
| `gh` + Headroom | 501/510 (9.824) | 247/255 (4.843) | 207/255 (4.059) | 153 | **6** | 2,119,615 | 22 |
| Octocode | **508/510 (9.961)** | **252/255 (4.941)** | **234/255 (4.588)** | **121** | 7 | **800,586** | **29** |

Failed research probes remain in the totals as workflow waste. They are not
deleted from an accepted run. The campaign is valid because every probe's
output, exit status, artifacts, and measurements were preserved and all six
passes completed.

## Per-pass result

| Pass | Headroom C/R/W | Headroom chars | Octocode C/R/W | Octocode chars | Pair wins | Winner |
|---:|---:|---:|---:|---:|---:|---|
| 1 | 167/82/66 | 581,167 | **170/83/82** | **273,646** | 7–10 | Octocode |
| 2 | 168/83/72 | 757,956 | **170/85**/72 | **266,390** | 7–10 | Octocode |
| 3 | 166/82/69 | 780,492 | **168/84/80** | **260,550** | 8–9 | Octocode |

Octocode won every pass. Median pass footprint was 757,956 characters for
Headroom and 266,390 for Octocode, a 2.85× ratio.

## Question-level majority

| Majority winner | Questions |
|---|---|
| Octocode (10) | Q1, Q3, Q4, Q5, Q6, Q8, Q9, Q11, Q12, Q17 |
| Headroom (7) | Q2, Q7, Q10, Q13, Q14, Q15, Q16 |

Q1's wording was again treated as materially ambiguous: both internally
consistent pairs—`getRouteRegex` → `getParametrizedRoute` and
`getNamedRouteRegex` → `getNamedParametrizedRoute`—received full correctness.

Q14 was correct in all six answers. At the frozen Vitest manifest,
`dependencies.vite` is absent, `peerDependencies.vite` is present,
`peerDependenciesMeta.vite.optional` is `false`, and Vite is separately present
in `devDependencies`. Vite is a required peer, not a regular dependency.

Material correctness deductions were concentrated in incomplete or inaccurate
multi-part answers: Headroom P1 Q17, Headroom P2 Q17, Headroom P3 Q5/Q6/Q12,
and Octocode P3 Q17. The blind grader established frozen-ref ground truth before
opening the identity-redacted packet.

## Headroom compression

| Pass | Raw `gh` chars | Delivered chars | Reduction | Calls |
|---:|---:|---:|---:|---:|
| 1 | 637,241 | 581,167 | 8.80% | 62 |
| 2 | 788,569 | 757,956 | 3.88% | 42 |
| 3 | 820,400 | 780,492 | 4.86% | 49 |
| **Total** | **2,246,210** | **2,119,615** | **5.64%** | **153** |

All 153 Headroom calls used the ONNX backend. Transform census: protected 49,
noop 48, mixed 23, SmartCrusher 21, diff 10, code-aware 1, and text 1. No call
used `router:protected:user_message`, and no model-readiness/disabled diagnostic
appeared.

## Integrity checks

- Campaign validator: zero failures.
- Exact census: 102 JSONL logs, 51 Headroom diagnostics, six complete answer
  files, and 427 referenced artifacts.
- All Unicode counts, byte counts, SHA-256 hashes, source exits, transforms,
  token ratios, artifact paths, and metrics matched independent recomputation.
- No missing, extra, duplicated, orphaned, or out-of-campaign artifacts.
- Every answer contained Q1–Q17 exactly once; maximum calls per question was
  eight.
- A Headroom P2 attempt that exposed output before logging was rejected before
  scoring and produced no campaign files. A fresh isolated replacement pass was
  run instead.

## Scope

This is a paired benchmark on one public GitHub question suite. It measures
delivered CLI-output characters and graded answer quality under the recorded
runner conditions. It does not directly measure tokens, latency, cost, or all
research workloads.

## Bottom line

The repeat confirms the prior hardened campaign's direction with a larger gap:
Octocode produced the higher blind-graded answer score, won 29–22, made 32 fewer
research calls, and used 62.2% fewer delivered characters. Headroom's 5.64%
transport reduction did not offset the larger `gh` research outputs.
