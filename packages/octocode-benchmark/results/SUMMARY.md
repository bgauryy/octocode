# Benchmark results summary

This page reports the latest complete result for each GitHub CLI matchup.
Invalid or incomplete campaigns are not included in headline calculations.
Historical complete reports remain available for traceability, but results with
different harness generations are not summed into one synthetic total.

## Latest complete campaigns

| Matchup | Questions / repeats | Correctness (baseline / Octocode) | Characters (baseline / Octocode) | Verdict |
|---|---:|---:|---:|---|
| [`gh` alone](octocode-vs-gh-233502-2026-08-05.md) | 17 × 1 | 9.94 / 9.94 | 286,812 / **152,710** | Equal correctness; Octocode 1.88× smaller and leaner on 11/17. |
| [`gh` + RTK](octocode-vs-gh-rtk-081914-2026-08-05.md) | 17 × 1 | **10.000** / 9.647 | 542,592 / **485,117** | RTK wins on correctness; Octocode used 10.6% fewer characters. |
| [`gh` + Headroom](octocode-vs-gh-headroom-115145-2026-08-05.md) | 17 × 3 | 9.824 / **9.961** | 2,119,615 / **800,586** | Octocode wins 29–22 and uses 62.2% fewer characters. |

## What the current evidence supports

- **Octocode is consistently smaller on aggregate** in these three latest
  complete campaigns.
- **Correctness is not a universal Octocode win.** RTK won its latest campaign
  because correctness takes priority over footprint.
- **The strongest current evidence is the Headroom campaign.** It has three
  passes per arm, strict artifact-backed measurement, complete log/answer
  census, blind grading, and an independent measurement audit.
- These campaigns reuse one public 17-question suite. They are paired campaign
  results, not independent samples of all repository-research work.

## Latest Headroom detail

| Metric | `gh` + Headroom | Octocode |
|---|---:|---:|
| Correctness | 501/510 | **508/510** |
| Research depth | 247/255 | **252/255** |
| Workflow | 207/255 | **234/255** |
| Calls | 153 | **121** |
| Failed calls retained | **6** | 7 |
| Delivered characters | 2,119,615 | **800,586** |
| Pair wins | 22 | **29** |

Headroom reduced 2,246,210 raw `gh` characters to 2,119,615, a 5.64%
reduction. Octocode's delivered footprint was 2.65× smaller. All six Q14
answers correctly reported Vite as a required peer and not a regular
dependency.

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
