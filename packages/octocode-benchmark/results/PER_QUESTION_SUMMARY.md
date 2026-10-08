# Per-question summary — chars & correctness by arm

**Dataset:** local-build Octocode CLI **v18.1.1**, 30-question v2 GitHub set × 3 passes, blind gpt-5.5 judge. The octocode anchor logs are byte-identical across the three baseline campaigns, so octocode is a single column.

**Sources (recomputed, not assumed):** chars = `total_chars` (model-in + model-out) read from each per-call JSONL, mean over 3 passes. Correctness = judge score 0–10, de-blinded via each campaign's MAP, mean over 3 passes (gh from `aggregate-out.json`; rtk/headroom from `judge/passN-verdicts.md`). Octocode correctness = mean over all 3 matchups that judged the identical octocode answer.

Regenerate: `python3 compare/bin/per_question_summary.py --out results/PER_QUESTION_SUMMARY.md`

> ⚠️ octocode chars cross-check vs gh-campaign JSON differs on 1 question(s): [(21, 65467.7, 52256.0)] — recomputed JSONL values are authoritative.

## Characters (mean per question, model-in + model-out)

| Q | octocode | gh | gh+RTK | gh+Headroom | RTK/O | HR/O | gh/O |
|---|---:|---:|---:|---:|---:|---:|---:|
| Q1 | 14,697 | 19,773 | 27,925 | 28,202 | 1.90× | 1.92× | 1.35× |
| Q2 | 19,159 | 731 | 26,361 | 1,322 | 1.38× | 0.07× | 0.04× |
| Q3 | 9,586 | 7,921 | 27,777 | 75,103 | 2.90× | 7.83× | 0.83× |
| Q4 | 26,201 | 131,764 | 136,621 | 257,472 | 5.21× | 9.83× | 5.03× |
| Q5 | 48,933 | 151,486 | 70,432 | 135,064 | 1.44× | 2.76× | 3.10× |
| Q6 | 14,593 | 42,905 | 53,565 | 41,147 | 3.67× | 2.82× | 2.94× |
| Q7 | 8,761 | 9,072 | 10,855 | 15,960 | 1.24× | 1.82× | 1.04× |
| Q8 | 14,574 | 23,947 | 63,101 | 59,135 | 4.33× | 4.06× | 1.64× |
| Q9 | 16,399 | 55,362 | 66,358 | 73,325 | 4.05× | 4.47× | 3.38× |
| Q10 | 12,246 | 13,200 | 10,552 | 9,377 | 0.86× | 0.77× | 1.08× |
| Q11 | 25,508 | 93,021 | 376,970 | 522,136 | 14.78× | 20.47× | 3.65× |
| Q12 | 17,036 | 82,360 | 98,514 | 82,695 | 5.78× | 4.85× | 4.83× |
| Q13 | 19,547 | 10,644 | 10,721 | 483,845 | 0.55× | 24.75× | 0.54× |
| Q14 | 7,596 | 10,390 | 8,312 | 6,093 | 1.09× | 0.80× | 1.37× |
| Q15 | 7,378 | 4,893 | 4,836 | 4,769 | 0.66× | 0.65× | 0.66× |
| Q16 | 10,731 | 23,578 | 1,057,962 | 959,745 | 98.59× | 89.43× | 2.20× |
| Q17 | 15,109 | 232,760 | 179,720 | 6,844,627 | 11.90× | 453.03× | 15.41× |
| Q18 | 12,772 | 12,178 | 8,244 | 7,614 | 0.65× | 0.60× | 0.95× |
| Q19 | 16,525 | 179,627 | 166,806 | 186,059 | 10.09× | 11.26× | 10.87× |
| Q20 | 15,816 | 48,996 | 114,467 | 80,465 | 7.24× | 5.09× | 3.10× |
| Q21 | 65,468 | 297,100 | 430,307 | 310,655 | 6.57× | 4.75× | 4.54× |
| Q22 | 53,622 | 12,611 | 425,890 | 13,237 | 7.94× | 0.25× | 0.24× |
| Q23 | 17,085 | 212,408 | 182,346 | 170,490 | 10.67× | 9.98× | 12.43× |
| Q24 | 16,067 | 70,767 | 114,119 | 72,038 | 7.10× | 4.48× | 4.40× |
| Q25 | 15,775 | 17,807 | 26,187 | 21,315 | 1.66× | 1.35× | 1.13× |
| Q26 | 16,594 | 66,256 | 50,292 | 24,089 | 3.03× | 1.45× | 3.99× |
| Q27 | 7,930 | 8,780 | 14,515 | 12,124 | 1.83× | 1.53× | 1.11× |
| Q28 | 62,423 | 1,472,374 | 6,964,358 | 835,870 | 111.57× | 13.39× | 23.59× |
| Q29 | 41,495 | 100,557 | 211,918 | 108,554 | 5.11× | 2.62× | 2.42× |
| Q30 | 33,689 | 32,215 | 117,539 | 35,424 | 3.49× | 1.05× | 0.96× |

## Correctness (mean per question, /10)

| Q | octocode | gh | gh+RTK | gh+Headroom |
|---|---:|---:|---:|---:|
| Q1 | 9.8 | 10.0 | 10.0 | 9.7 |
| Q2 | 10.0 | 9.7 | 10.0 | 10.0 |
| Q3 | 10.0 | 10.0 | 10.0 | 10.0 |
| Q4 | 9.2 | 8.0 | 10.0 | 7.0 |
| Q5 | 9.6 | 9.3 | 9.0 | 8.7 |
| Q6 | 8.7 | 10.0 | 10.0 | 10.0 |
| Q7 | 9.9 | 10.0 | 10.0 | 8.3 |
| Q8 | 10.0 | 10.0 | 10.0 | 10.0 |
| Q9 | 9.7 | 9.0 | 9.7 | 10.0 |
| Q10 | 9.9 | 9.0 | 6.3 | 6.0 |
| Q11 | 9.9 | 10.0 | 9.7 | 10.0 |
| Q12 | 9.6 | 9.3 | 10.0 | 10.0 |
| Q13 | 10.0 | 7.3 | 9.7 | 10.0 |
| Q14 | 9.9 | 10.0 | 8.0 | 1.7 |
| Q15 | 9.9 | 10.0 | 9.7 | 8.7 |
| Q16 | 9.8 | 9.7 | 10.0 | 4.7 |
| Q17 | 8.2 | 7.0 | 8.3 | 8.0 |
| Q18 | 9.6 | 10.0 | 10.0 | 4.7 |
| Q19 | 9.3 | 10.0 | 10.0 | 10.0 |
| Q20 | 10.0 | 10.0 | 10.0 | 9.0 |
| Q21 | 6.2 | 10.0 | 9.3 | 10.0 |
| Q22 | 8.2 | 10.0 | 10.0 | 10.0 |
| Q23 | 7.8 | 7.3 | 8.0 | 8.3 |
| Q24 | 9.8 | 7.3 | 8.3 | 10.0 |
| Q25 | 8.7 | 8.3 | 9.7 | 9.7 |
| Q26 | 8.3 | 9.0 | 10.0 | 10.0 |
| Q27 | 9.2 | 9.7 | 10.0 | 9.0 |
| Q28 | 7.2 | 9.3 | 7.0 | 7.7 |
| Q29 | 9.6 | 9.3 | 9.3 | 9.7 |
| Q30 | 9.9 | 9.7 | 10.0 | 9.0 |

## Overall

| Metric | octocode | gh | gh+RTK | gh+Headroom |
|---|---:|---:|---:|---:|
| Mean chars / question | 22,111 | 114,849 | 368,586 | 382,598 |
| Median chars / question | 16,233 | 37,560 | 68,395 | 72,681 |
| Total chars (Σ 30 Q means) | 663,319 | 3,445,482 | 11,057,569 | 11,477,951 |
| Mean correctness /10 | 9.3 | 9.3 | 9.4 | 8.7 |
| Median correctness /10 | 10.0 | 10.0 | 10.0 | 10.0 |
| Char ratio vs octocode — per-Q, ratio of means (geo) | 1.00× | 2.04× | 3.90× | 3.50× |
| Char ratio vs octocode — per-Q, ratio of means (median) | 1.00× | 2.42× | 4.05× | 4.06× |
| Questions octocode leaner (/30, mean chars) | — | 23 | 26 | 24 |
| **Char ratio (per-instance N=90, geo-mean — headline)** | 1.00× | **2.01×** | **3.22×** | **2.73×** |
| Char ratio (per-instance N=90, median) | 1.00× | 2.24× | 3.06× | 3.16× |
| Instances octocode leaner (/90) | — | 68 | 68 | 65 |

> Char ratio >1× = octocode delivered fewer characters. octocode correctness column is the mean across all three matchups; per-matchup octocode scores are in the JSON (`--json`) under `correct.octocode_vs_*`.

## Reconciliation vs published SUMMARY.md

The per-instance (N=90) geo-means recomputed here match the published headline geo-means (`results/SUMMARY.md`, local build): gh **2.01×** vs 1.99×, RTK **3.22×** vs 3.21×, Headroom **2.73×** vs 2.62×. The Headroom gap is expected: the published headline excluded 2 instances (N=88, incl. one ~20M-char outlier question) while this table keeps all 90. Mean correctness matches too (gh 9.3/9.3, RTK 9.3/9.4, Headroom net octocode-higher).

