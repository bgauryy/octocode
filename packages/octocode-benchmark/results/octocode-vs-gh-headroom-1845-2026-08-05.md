# Octocode CLI vs `gh` + Headroom — Run Report

**Matchup:** `compare/octocode-vs-gh-headroom` · 20-question shared GitHub set
**Campaign:** `campaign-20260805-1845` · run 2026-08-05
**Passes:** 3 per arm (pass 1 = live isolated runner research; passes 2–3 = deterministic replay of the exact instrumented commands).

## Pinned versions (a different version = a different arm A)

| Component | Version |
|---|---|
| Headroom | **0.34.0** (README pinned 0.33.0 — recorded as run under 0.34.0) |
| Kompress model / backend | `kompress-v2-base`, ONNX backend |
| `gh` | 2.96.0 (2026-07-02), authenticated |
| Octocode CLI | v18.0.1 |
| Runner model (both arms) | `claude-sonnet-4-6` |
| Grader model | `claude-opus-4-8` (independent, own research tools) |

Readiness gate `preflight.py --warmup`: **PASS** (valid no-op, SmartCrusher on JSON, Kompress on prose, 0 failures).
Campaign validation `validate_campaign.py`: **0 failures** — all 120 logs strict-valid, all 6 answer files present with `## Q1..## Q20`, hashes/artifacts intact, Headroom transforms classified.

## Protocol

- **Arm A** (`X` in pass 1): read-only `gh` piped through Headroom via `./bin/ghc`; chars-in = instrumented `sum(out_chars)`.
- **Arm B** (`Y` in pass 1): `npx octocode tools …` via `./bin/octoc`; chars-in = instrumented `sum(chars)`.
- Arms run in **isolated** subagents (fresh context, `bash`-only, no MCP, no cross-arm/reference access).
- Grading is **blind**: tool identities redacted, X/Y shuffled per pass; grader established ground truth independently.

## Measured totals (authoritative, artifact-backed Unicode code points)

| Arm | calls/pass | raw_chars | **chars_in** (median) | reduction | failed_calls |
|---|--:|--:|--:|--:|--:|
| A — gh+Headroom | 141 | ~2.19M | **~1,952,885** | 10.9% | 5–7 |
| B — Octocode | 160 | ~0.519M | **~518,758** | 0.0%¹ | 21 |

¹ Octocode transport applies no post-hoc compression; its low chars-in comes from targeted reads/minified views, not a compressor.
Char spread across passes is <0.2% (A) / <0.15% (B) — stable.

**Octocode pulled ≈3.76× fewer context chars than gh even after Headroom compression** (519K vs 1.95M).

## Blind grading result (correctness dominates; ties broken by fewer chars-in)

| Metric | Arm A (gh+Headroom) | Arm B (Octocode) |
|---|--:|--:|
| Correctness (sum /200) | 150 | **182** |
| Research depth (mean /5) | 3.6 | 3.7 |
| Workflow (mean /5) | 3.3 | **4.15** |
| Total chars_in | 1,950,658 | **518,758** |
| Per-question preference wins | 3 | **17** |

**Verdict: Octocode (B) is clearly stronger** — higher correctness and 3.76× cheaper context.

### Decisive findings
- Arm A made **5 confident errors on deterministic package.json/metadata questions** (Q7, Q10, Q14, Q16, Q18) — e.g. claiming absent fields that are present. These are the "confidently wrong" cases the rubric penalizes hardest, and align with the runner-protocol warning that a compact/compressed view eliding object boundaries is not sufficient evidence for exact membership.
- Arm A over-fetched heavily even compressed (Q11 796K, Q4 230K, Q17 207K chars-in).
- **Arm B's real weaknesses (honest):** Q3 (33 calls / 9 failed — failed to retrieve commit `705e5268` content; A won on a targeted 3-call approach); higher raw failed_calls (21) from schema/empty-search retries. A also won Q6 and Q13.

## Artifacts
- Per-question logs + artifacts, answers, metrics: `compare/octocode-vs-gh-headroom/tmp/campaign-20260805-1845/`
- `metrics.json` (validator output), `blind-packet.md`, `GRADING.md` (full per-question grader report).
