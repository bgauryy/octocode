Load when configuring instrumentation or interpreting character, correctness and paired-comparison results.

# Measurement and scoring

## How it is MEASURED (characters, never self-reported)

The metric is **`total_chars = model-in + model-out`** in Unicode code points, from an
instrumented log — the *tool transcript only* (excludes system prompt, tool schemas, model
reasoning; the fixed per-arm primer is excluded by rule; any later help/schema/failed call
**is** counted).

- **model-out** = the command string + args the model wrote, plus the final answer.
- **model-in** = the tool output pulled back into context (for Headroom, the *compressed* output).

Mechanism: every research command runs through its arm's thin wrapper, which shells the real
CLI unchanged, prints output verbatim, and appends one JSONL row per call:

| Arm | Wrapper | Runs | Log env |
|---|---|---|---|
| octocode (local build) | `compare/bin/octoc` | `node packages/octocode/out/octocode.js <tool> '<json>'` | `OCTO_LOG` |
| octocode (published pin) | `compare/bin/octoc-npm` | `npx -y $OCTO_SPEC tools …` (default `octocode@18.2.2`) | `OCTO_LOG` |
| gh+RTK | `compare/bin/rtkm` | `rtk gh …` | `RTK_LOG` |
| gh+Headroom | `compare/bin/ghc` | `gh …` → Headroom compress | `GHC_LOG` (+ `HR_PY`) |
| plain gh | `compare/bin/ghm` | `gh …` (read-only) | — |

The final answer is logged as pure model-out via `compare/bin/record_answer.py`. Per-question
total = `compare/bin/sumlog.py --strict <log>`; the whole campaign is checked byte-faithfully
by `compare/bin/validate_campaign.py`. **Never trust a hand-counted number** — recompute from
the JSONL. Only `elapsed_ms` (octocode/rtk/gh) is captured for time; it is not a fair latency
metric (npx bootstrap per call on the published pin, no Headroom timing) — do not headline it.

## How it is SCORED (blind judge, correctness-first)

One blind judge per question grades the two answers as **X / Y** (order randomized per
question, tool identity redacted). It **reasons to ground truth first**, then scores each
answer: **correctness 0–10, research depth 1–5, workflow 1–5** (rubric in [JUDGING.md](JUDGING.md)).

Decision per pairing: if one arm is net strictly more correct (paired sign test) it **wins** —
a confidently-wrong answer never wins on footprint. If correctness is statistically tied,
**characters decide** by the **geometric-mean of per-question ratios** (baseline ÷ octocode) +
median + leaner win-rate + bootstrap CI — never a pooled sum alone. Aggregate paired, per
question, over **≥3 passes**. Method + worked example: [aggregation and statistics](aggregation-and-stats.md).

