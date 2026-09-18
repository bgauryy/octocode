# Bug-Triage Benchmark — Results
**Suite:** bug-triage-v1  
**Generated:** 2026-09-18T15:51:17.394Z  
**Cases graded:** 10 / 10

## Verdict
**ACCEPT — replicate**

## Aggregate quality

| Metric | Baseline | Treatment | Delta |
|---|---:|---:|---:|
| Mean score /10 | 8.01 | 8.59 | 0.58 |
| Cases won | 0 | 8 | — |
| Ties | — | 2 | — |
| Major false claims | 0 | 0 | — |
| Jev-change evident (judge) | — | 3 | — |

## Per-case scores

| Case | Repo | Baseline | Treatment | Δ | Winner | Jev changed? | Baseline FC? | Treatment FC? |
|---|---|---:|---:|---:|---|---|---|---|
| BUG-01 | next.js | 7 | 7.5 | +0.5 | treatment | no | — | — |
| BUG-02 | next.js | 7 | 7.8 | +0.7999999999999998 | treatment | no | — | — |
| BUG-03 | next.js | 7.3 | 8.5 | +1.2000000000000002 | treatment | no | — | — |
| BUG-04 | next.js | 8.3 | 9 | +0.6999999999999993 | treatment | no | — | — |
| BUG-05 | axios | 9 | 9.5 | +0.5 | treatment | yes | — | — |
| BUG-06 | axios | 9.1 | 9.3 | +0.20000000000000107 | tie | no | — | — |
| BUG-07 | axios | 7.2 | 8.5 | +1.2999999999999998 | treatment | no | — | — |
| BUG-08 | vite | 8.1 | 8 | -0.09999999999999964 | tie | yes | — | — |
| BUG-09 | vite | 9 | 9.5 | +0.5 | treatment | yes | — | — |
| BUG-10 | vite | 8.1 | 8.3 | +0.20000000000000107 | treatment | no | — | — |

## Flow and token metrics

| Metric | Baseline | Treatment |
|---|---:|---:|
| Complete cases | 10 | 10 |
| Octocode calls | 29 | 29 |
| Octocode query rows | 47 | 58 |
| Tool error rows | 0 | 0 |
| Jev calls | — | 3 |
| Cases where Jev called | — | 3 |
| Cases where Jev changed decision | — | 0 |
| Jev input tokens | — | 3601 |
| Jev output tokens | — | 289 |
| Summed Jev latency (ms) | — | 0 |

## Gate classification (treatment arm)

| Classification | Cases |
|---|---:|
| `disputed_inference` (Jev eligible) | 4 |
| `deterministic` (no Jev) | 5 |
| `missing_fact` (no Jev) | 1 |

## Limitations
- One run per arm; no statistical significance.
- Judge model shares training with worker models; possible correlated bias.
- Public issues may be partially in model training data.
- Host LLM tokens not captured (Octocode calls and Jev tokens measured only).
- A single positive pilot does not justify default Jev routing — only optional gate-routed calls.
