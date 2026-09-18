# sonnet-16-v1 results

**One run, 16 cases, Sonnet host, blinded judge. Verdict: on this suite forced Jev did not help — baseline won 11–5, paired total delta −1.56 / 15.** Report-only pilot; see limitations before generalizing.

Run: 48 agents (16 × 2 arms + 16 judges), 0 errors, 16/16 live Jev calls succeeded, ~12 min, 3.66M host tokens, 538 tool calls. Jev API overhead: 47,707 input + 1,400 output tokens across 16 calls.

## Aggregate

| Metric | Baseline (octocode only) | Jev (octocode + 1 forced Jev call) |
|---|---|---|
| Wins | **11** | **5** |
| Correctness (0–5 avg) | 4.25 | 4.06 |
| Research quality (0–5 avg) | **4.00** | 3.25 |
| Efficiency (0–5 avg) | **4.06** | 3.44 |
| Total /15 avg | **12.31** | 10.75 |
| Octocode calls avg | 5.06 | 5.69 |

Paired total delta (jev − baseline): **−1.56 / 15**. Damage is concentrated in research-quality (−0.75) and efficiency (−0.62); correctness is roughly flat (−0.19).

## Per case

| Case | Kind | Baseline C/R/E (tot) | Jev C/R/E (tot) | Δtot | Winner | Jev P | Jev flipped answer? |
|---|---|---|---|---|---|---|---|
| Q1 | question | 5/5/5 (15) | 4/3/2 (9) | −6 | baseline | 0.97 | no |
| Q2 | question | 5/4/4 (13) | 5/4/5 (14) | +1 | jev | 1.0 | no |
| Q3 | question | 3/2/3 (8) | 4/4/5 (13) | +5 | jev | 1.0 | no |
| Q4 | question | 5/5/5 (15) | 4/3/2 (9) | −6 | baseline | 0.97 | no |
| Q5 | question | 4/3/4 (11) | 5/3/5 (13) | +2 | jev | 1.0 | no |
| Q6 | question | 3/3/4 (10) | 5/5/4 (14) | +4 | jev | 1.0 | no |
| Q7 | question | 5/5/5 (15) | 4/3/4 (11) | −4 | baseline | 1.0 | no |
| Q8 | question | 4/5/5 (14) | 5/3/3 (11) | −3 | baseline | 1.0 | no |
| Q9 | question | 5/4/5 (14) | 4/3/2 (9) | −5 | baseline | 0.99 | no |
| Q10 | question | 5/4/4 (13) | 4/3/4 (11) | −2 | baseline | 1.0 | no |
| ISS1 | issue | 5/5/3 (13) | 3/3/3 (9) | −4 | baseline | 1.0 | no |
| ISS2 | issue | 4/4/4 (12) | 3/3/3 (9) | −3 | baseline | 0.85 | no |
| ISS3 | issue | 5/5/5 (15) | 4/3/3 (10) | −5 | baseline | 1.0 | no |
| ISS4 | issue | 4/4/4 (12) | 3/2/3 (8) | −4 | baseline | 1.0 | no |
| ISS5 | issue | 2/2/1 (5) | 5/5/5 (15) | **+10** | jev | 1.0 | no |
| ISS6 | issue | 4/4/4 (12) | 3/2/2 (7) | −5 | baseline | 1.0 | no |

## The decisive finding

**Jev changed the worker's direction in 0 / 16 cases.** Every treatment note reads the same: the worker formed a provisional answer from direct source reads, then Jev *confirmed* it (probability 0.85–1.0, claim gate never blocked). Jev never faced — and never corrected — an attractive wrong lead, because this suite has almost none: Q1/Q4/Q7/Q8/Q9 are single-file source lookups with one right answer.

So the treatment effect of Jev-the-judge on final correctness was ≈0. The score gaps came from **second-order costs of a forced call on a decided question**:
- extra octocode calls chasing Jev's suggested next-checks (Q4 hit 8 calls, over the 7 cap; Q9 7 vs 2);
- "hypothesis_triage" process narrative that judges scored down as extraneous, unverifiable, or backed by bare repo links without ref/lines.

Split by outcome: the 5 jev wins averaged **+4.4** (Q3, Q6, ISS5 are cross-repo / history-heavy cases where the *treatment worker* simply investigated better — ISS5 baseline botched the root cause 5/15, treatment nailed PR #51083 at 15/15); the 11 baseline wins averaged **−4.27**, all on cases where the answer was already deterministic and Jev only added overhead.

## Why this is consistent with the skill, not a refutation

The standalone `octocode-jev-reasoning-loop` skill explicitly says: *"Do not force the full loop onto routine work"* and *"Exact lookup … stays deterministic."* This benchmark's forced-one-call-per-case protocol deliberately violates that routing rule, so **−1.56 measures the cost of misrouting Jev onto lookups, not Jev's value at a real fork.** The skill's own live recovery benchmark — built from attractive-wrong-lead cases — showed 3/3 recovery; this suite is the opposite population, so it cannot exercise Jev's designed mechanism.

## Next decision-changing experiment

Route-gate Jev before spending a call: classify each case `deterministic` / `missing_fact` / `disputed_inference` and call Jev only on `disputed_inference`. On this suite that keeps the 5 wins and drops the 11 forced-call losses. Better: build a disputed-inference suite with real attractive-wrong-lead cases (two source-backed interpretations, no cheap deterministic check) and re-run — that isolates Jev's recovery value instead of taxing it on source-tracing.

## Limitations

One run per case; possible prior model knowledge on public repos; moving default branches; host LLM tokens not exposed (efficiency judged from octocode call counts + citation quality); single judge model (Sonnet) shared with the workers. Do not treat as product-level evidence.
