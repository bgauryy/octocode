# Full campaign — Octocode (published npx CLI **v18.2.2**) vs gh + Headroom

**Outcome:** At correctness parity, the **published** Octocode CLI (`npx octocode@18.2.2`) is
**reliably ~2.3× leaner** than gh+Headroom (compressed `gh`) across the full 30-question v2 set
over 3 passes. Per-question geometric-mean char ratio **2.27×** (95% bootstrap CI **1.70–3.01×**,
entirely above 1×), median 2.60×, Octocode leaner on **67/90** instances (sign test p<0.0001).
Correctness was a dead tie (mean **8.68 vs 8.68**/10, sign p=0.59). Judge winners **47 / 43**
(octocode / headroom): where a single snippet already answers, Headroom's compressed `gh` is
leaner and takes the char tiebreak.

## Run metadata

- **Question set:** `compare/github-questions/` v2, 30 questions.
- **Matchup:** Octocode (anchor) vs gh + Headroom (baseline). Pairwise; this campaign ran Headroom only.
- **Octocode arm:** **published npx `octocode@18.2.2`** via pinned wrapper `compare/bin/octoc1822`
  (`npx -y octocode@18.2.2 tools …`).
- **Baseline:** `gh` 2.96.0 piped through **Headroom 0.34.0** (`compare/bin/ghc`, ONNX backend,
  compression active — preflight `--warmup` confirmed, transforms logged per call).
- **Runner model:** claude-sonnet-4-6 (anthropic). **Judge model:** gpt-5.5 (guy-provider-openai) —
  neutral, different family; blind X/Y packet, seed=42, tool identity redacted (0 leaks verified).
- **Passes:** 3 (isolated runner per arm×batch×pass; batched Q1-15/Q16-30 per arm).
- **Measurement:** `total_chars = model-in + model-out` from instrumented per-question JSONL;
  Headroom `model-in` is the **compressed** output (what enters context). Byte-faithful strict
  validation of every raw+out artifact (see caveats).

## Summary (paired, per question)

| Metric | vs Headroom |
|---|---:|
| Correctness — win / tie / loss (octocode) | **25 / 35 / 30** (sign p=0.59) |
| Mean correctness — octocode / headroom | **8.68 / 8.68** (of 10) |
| Judge winners — octocode / headroom / unresolved | **47 / 43 / 0** |
| Research depth — mean (octocode / headroom) | ~4.3 / ~4.3 |
| Workflow — mean (octocode / headroom) | ~4.0 / ~3.4 |
| **Char ratio — geometric mean (headline)** | **2.27× leaner** |
| Char ratio — 95% bootstrap CI | **1.70 … 3.01×** |
| Char ratio — median (min … max) | 2.60× (0.03 … 206.74×) |
| Questions Octocode leaner (of 90) + sign p | **67 / 90**, p<0.0001 |
| Chars pooled sum — *outlier-sensitive* | 5.77× (headroom 14.27M / octocode 2.47M) |
| — top contributor (Q28/p3) share of headroom total | 20.3% |
| — pooled ratio, leave-one-out (drop top) | 4.98× |
| Per-pass geo-mean (P1 / P2 / P3) | 2.66× / 2.06× / 2.15× |

## Best-on (roll-up)

- **Accuracy:** exact parity (8.68 vs 8.68, p=0.59). Each arm has a few clear wins: Octocode on
  deterministic dependency reads (Q14 vite peer 8.0 vs 2.7; Q23 Linux VFS 9.0 vs 7.3; Q24 axios
  blast-radius 10 vs 9.3), Headroom on a few deep repo questions (Q28 hermes memory 9.0 vs 5.0;
  Q29 MCP auth 9.7 vs 6.7; Q26 zustand docs 8.7 vs 6.7). They cancel out.
- **Workflow / characters:** Octocode — leaner on 67/90, geo-mean 2.27× (CI 1.70–3.01×). Even with
  Headroom compressing raw `gh` output, tree/large-file/multi-hop questions still cost far more:
  **Q16 27×, Q28 14×, Q23 8.8×, Q8 7.5×, Q17 7×, Q3 6.6×, Q19 5.3×, Q24 5.1×, Q11 4.9×, Q4 4.9×**.
- **Where Headroom wins chars:** single-hit lookups — **Q2 0.06×, Q13 0.32×, Q7 0.48×, Q22 0.58×,
  Q30 0.65×, Q14 0.68×, Q15 0.75×** — one compressed `gh` call beats a multi-call octocode path.
- **Overall:** at correctness parity, the published Octocode CLI wins the matchup on leanness;
  compression narrows but does not close the gap (CI well above 1×).

## Fairness & measurement caveats

- **Compression verified ON:** preflight `preflight.py --warmup` reported compression active
  (ONNX backend, no failures); per-call `transforms` are logged (e.g. `router:code_aware`), and a
  `ratio 0.0`/`router:noop` only appears on tiny outputs Headroom intentionally passes through.
- **Outlier disclosure:** pooled 5.77× is outlier-sensitive — Q28/p3 alone is 20.3% of Headroom's
  total. Headline uses the geo-mean (2.27×) + CI, not the pooled sum; leave-one-out is 4.98×.
- **Stability:** the leaner side flips across passes on Q5, Q10, Q14, Q15, Q18, Q22, Q25, Q30 —
  near-1× questions, not decisive winners.
- **Byte-faithful validation:** validated via campaign-local `build_metrics.py` decoding each raw
  and out artifact without newline translation (avoids the shared `sumlog.py` CRLF re-read bug).
  All 180 logs reconcile exactly (char count + sha for both raw and compressed artifacts).
- **Public-set ceiling:** v2 is orientation, not a shipping gate; correctness clusters near
  ceiling and is contaminated-prone — treat the parity result as such.

## Artifacts

- Campaign: `campaigns/full-hr-091618-2026-08-08/`
- Logs: `{octocode,headroom}-p{1,2,3}-Q{1..30}.jsonl` (+ `-artifacts/`, headroom `-diagnostics.log`)
- Answers: `answers/{arm}-p{pass}.md`
- Blind packet + map: `blind-packet.md` (+ `.MAP.secret.txt`, seed=42)
- Verdicts (gpt-5.5): `judge/p{1,2,3}-Q{n}.md`; scores `judge/scores-p{pass}-h{1,2}.csv`
- Metrics: `metrics.json` (validated) · aggregate `metrics-agg.json` · `aggregate.py`

## Per-question table (mean across 3 passes; Chars = total = model-in + model-out)

| Q | Corr O/B | Depth O/B | Workflow O/B | Chars O / B (mean) | Winner (3p) | Ratio B/O |
|---|---|---|---|---|---|---|
| Q1 | 10.0/10.0 | 5.0/5.0 | 4.3/4.0 | 14,868 / 32,949 | O (3-0) | 2.19× |
| Q2 | 10.0/10.0 | 4.7/4.7 | 2.7/5.0 | 14,101 / 699 | B (0-3) | 0.06× |
| Q3 | 10.0/9.7 | 5.0/5.0 | 3.0/4.3 | 9,848 / 69,680 | O (2-1) | 6.64× |
| Q4 | 9.3/8.0 | 5.0/4.7 | 3.7/2.3 | 43,094 / 197,534 | O (2-1) | 4.89× |
| Q5 | 9.0/8.3 | 5.0/4.3 | 4.7/4.0 | 50,197 / 61,973 | O (2-1) | 1.17× |
| Q6 | 8.7/10.0 | 4.3/5.0 | 4.0/2.3 | 13,520 / 61,110 | B (1-2) | 4.56× |
| Q7 | 8.3/7.7 | 4.3/3.7 | 4.7/4.3 | 10,165 / 4,854 | O (2-1) | 0.48× |
| Q8 | 10.0/10.0 | 5.0/5.0 | 4.7/3.0 | 11,484 / 59,272 | O (2-1) | 7.53× |
| Q9 | 9.7/10.0 | 5.0/5.0 | 4.7/3.3 | 16,516 / 65,835 | O (2-1) | 4.21× |
| Q10 | 7.7/6.7 | 3.7/3.7 | 4.0/3.3 | 9,419 / 9,386 | O (2-1) | 1.00× |
| Q11 | 9.3/9.7 | 4.3/5.0 | 3.7/2.7 | 35,907 / 177,496 | O (2-1) | 4.93× |
| Q12 | 9.3/9.7 | 4.7/4.7 | 3.7/3.3 | 26,823 / 85,253 | O (2-1) | 4.04× |
| Q13 | 10.0/9.3 | 5.0/4.7 | 3.7/4.0 | 21,899 / 6,969 | B (1-2) | 0.32× |
| Q14 | 8.0/2.7 | 3.7/1.7 | 4.0/4.0 | 13,073 / 7,921 | O (3-0) | 0.68× |
| Q15 | 9.0/10.0 | 4.7/5.0 | 4.3/5.0 | 11,970 / 10,753 | B (0-3) | 0.75× |
| Q16 | 8.3/7.0 | 4.0/3.0 | 4.7/1.3 | 26,571 / 944,692 | O (2-1) | 26.82× |
| Q17 | 7.0/7.3 | 3.3/3.3 | 4.0/2.0 | 20,833 / 147,755 | O (2-1) | 7.06× |
| Q18 | 9.3/8.0 | 4.3/3.7 | 4.7/4.0 | 8,960 / 11,252 | B (1-2) | 0.99× |
| Q19 | 8.7/10.0 | 4.3/5.0 | 4.0/2.7 | 24,773 / 139,822 | B (1-2) | 5.29× |
| Q20 | 7.7/9.0 | 3.7/4.3 | 4.3/3.7 | 13,571 / 34,462 | O (2-1) | 2.56× |
| Q21 | 8.0/8.3 | 4.0/4.3 | 3.3/2.7 | 87,723 / 259,666 | B (1-2) | 3.17× |
| Q22 | 9.7/9.3 | 4.7/4.7 | 3.0/4.7 | 38,032 / 55,014 | B (1-2) | 0.58× |
| Q23 | 9.0/7.3 | 4.7/3.3 | 4.7/2.7 | 11,947 / 106,965 | O (2-1) | 8.84× |
| Q24 | 10.0/9.3 | 5.0/4.3 | 4.7/3.0 | 14,911 / 74,565 | O (3-0) | 5.11× |
| Q25 | 8.0/7.3 | 4.3/4.0 | 4.7/4.0 | 18,087 / 36,578 | O (2-1) | 1.72× |
| Q26 | 6.7/8.7 | 3.3/4.3 | 4.0/4.3 | 7,946 / 24,979 | B (0-3) | 3.05× |
| Q27 | 8.3/8.7 | 4.0/4.3 | 5.0/4.0 | 6,962 / 18,600 | O (2-1) | 2.68× |
| Q28 | 5.0/9.0 | 3.0/4.7 | 1.7/1.3 | 153,725 / 1,943,312 | B (0-3) | 13.95× |
| Q29 | 6.7/9.7 | 3.7/5.0 | 4.0/3.7 | 43,595 / 75,766 | B (1-2) | 1.72× |
| Q30 | 9.7/9.7 | 4.7/4.7 | 4.0/4.3 | 43,362 / 31,878 | B (1-2) | 0.65× |

O = Octocode (`npx octocode@18.2.2`); B = gh+Headroom (compressed). Winner (3p) = per-pass judge
winners; Ratio B/O is the per-question geometric-mean char ratio (>1 = Octocode leaner). Full
per-instance verdicts + scores in `campaigns/full-hr-091618-2026-08-08/judge/`.

## Bottom line

Equally correct (8.68 vs 8.68, not significant). At that parity the **published Octocode CLI
v18.2.2 is typically ~2.3× leaner** than compressed gh+Headroom (geo-mean 2.27×, 95% CI
1.70–3.01×, median 2.60×), leaner on 67/90 instances — the advantage concentrated on large-file
and multi-hop questions (Q16, Q28, Q23, Q8, Q17) and inverted on single-hit lookups Headroom
compresses to almost nothing (Q2, Q13, Q7). Directionally consistent with the prior local-build
Headroom run (~2.6× leaner; that run also showed Octocode *more* correct, whereas this
published-CLI run lands at correctness parity).
