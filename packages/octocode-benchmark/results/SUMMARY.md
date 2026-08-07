# Benchmark results summary

This page reports the latest complete result for each GitHub CLI matchup.
Invalid or incomplete campaigns are not included in headline calculations.
Historical complete reports remain available for traceability, but results with
different harness generations are not summed into one synthetic total.

## Latest complete campaigns

Each matchup is a full **30-question v2 set × 3 passes** (local build `out/octocode.js`
v18.1.1), blind neutral **gpt-5.5** judge, X/Y randomized per question, leanest-path enforced,
95% bootstrap CIs (5,000 resamples of the per-question `total_chars` ratios).

| Matchup | Questions | Correctness (Octocode / baseline) | Char ratio (per-Q geo-mean, 95% CI) | Wins O/tie/B · leaner | Verdict |
|---|---:|---|---|---|---|
| [vs gh+Headroom](full-octocode-vs-headroom-134213-2026-08-07.md) | 30 × 3 | 9.30 / 8.62 (Octocode edges) | **2.62×** (1.87–3.71), median 2.77× | 60 / 0 / 28 · 63/88 | Octocode more correct **and** ~2.6× leaner; CI above 1×. Baseline prone to char blowups on tree/file dumps (one 20M-char question). |
| [vs gh+RTK](full-octocode-vs-rtk-162848-2026-08-07.md) | 30 × 3 | 9.29 / 9.42 (near-parity) | **3.21×** (2.36–4.46), median 2.83× | 56 / 0 / 33 · 67/89 | Correctness parity; Octocode **reliably ~3.2× leaner** (CI well above 1×). Supersedes the earlier 2-pass RTK "parity" finding — the larger 3-pass run resolves toward Octocode leaner. |

## What the current evidence supports

- **Correctness is a near-ceiling tie** in both matchups (Octocode 9.30 vs Headroom 8.62, and 9.29 vs RTK 9.42; RTK 9.42
  marginally higher as a **lossless** raw-`gh` passthrough; Headroom 8.61 lowest as **lossy**
  compression, and Octocode is net more correct than Headroom). Correctness-first scoring
  means a leaner answer never overrides a wrong one.
- **Characters: Octocode is reliably leaner than *both* baselines** — ~2.6× vs gh+Headroom
  (95% CI 1.87–3.71) and ~3.2× vs gh+RTK (95% CI 2.36–4.46); both CIs sit above 1×, and
  Octocode is leaner on ~72% of questions (63/88 and 67/89). Fewer characters delivered =
  healthier context window and sharper model attention on the load-bearing evidence.
- **The RTK result is now stable at 3 passes.** The earlier 2-pass RTK snapshot straddled 1×
  (snippet-vs-file path variance); the larger 30×3 run resolves to a clear Octocode lean. The
  public v2 suite is still orientation, not a shipping gate (contaminated-ceiling correctness).

## Historical multi-pass Headroom detail ([1845](octocode-vs-gh-headroom-1845-2026-08-05.md), 20 × 3, Headroom 0.34.0)

*Retained for its stronger three-pass evidence; the current headline is the [2-pass campaign](campaign-2pass-183432-2026-08-06.md).*

| Metric | `gh` + Headroom | Octocode |
|---|---:|---:|
| Correctness (sum /200) | 150 | **182** |
| Research depth (mean /5) | 3.6 | **3.7** |
| Workflow (mean /5) | 3.3 | **4.15** |
| Delivered characters (chars-in) | 1,950,658 | **518,758** |
| Per-question preference wins | 3 | **17** |
| Failed calls retained | **5–7** | 21 |

Headroom compressed ~2.19M raw `gh` characters to ~1.95M (10.9% reduction);
Octocode still delivered **≈3.76× fewer** context characters (519K vs 1.95M)
via targeted/minified reads, not a compressor. Arm A made **5 confident errors
on deterministic package.json/metadata questions** (Q7, Q10, Q14, Q16, Q18) —
the correctness gap. Octocode's honest weaknesses: Q3 commit-content retrieval
(33 calls / 9 failed; A won with a targeted 3-call path) and a higher raw
failed-call count from schema/empty-search retries; A also won Q6 and Q13.

## Historical reports (restored, prior methodology)

Kept for traceability; not summed into the headline (different harness generations):
[gh+RTK 011611](octocode-vs-gh-rtk-011611-2026-08-06.md) ·
[gh+RTK 200641](octocode-vs-gh-rtk-200641-2026-08-05.md) ·
[gh+RTK localfix](octocode-vs-gh-rtk-localfix-020431-2026-08-06.md) ·
[gh+Headroom 011859](octocode-vs-gh-headroom-011859-2026-08-06.md) ·
[gh+Headroom 200034](octocode-vs-gh-headroom-200034-2026-08-05.md).

## Validity policy

A published headline campaign must contain every planned question and pass,
preserve every research call—including failed probes—and pass its applicable
measurement checks. Failed calls inside a complete campaign remain counted as
workflow waste; deleting them would bias the result. A campaign that loses
measurements, omits answers, mixes units, or cannot classify its transport is
excluded instead of repaired after the fact.

The incomplete 15/17 Headroom campaign and the Headroom campaign explicitly
marked invalid for a winner claim were removed from `results/` and do not
contribute to this summary.

## Scope and interpretation

The benchmark measures answer correctness, evidence depth, workflow quality,
and Unicode characters delivered by each CLI. It does not directly measure
tokens, model latency, monetary cost, or product-wide capability. Use public
benchmark results as orientation; use private held-out failures for shipping
decisions.

See [benchmark design](../BENCHMARK.md), [scoring](../SCORING.md),
[judging](../JUDGING.md), and the [run instructions](../INSTRUCTIONS.md).
