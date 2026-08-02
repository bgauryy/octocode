# Results — octocode-vs-gh-rtk

> Tracked results ledger. Latest scored run first; full artifacts in the (gitignored) `output/<run>/` dir it names. Refresh this file after every scored run (see BENCHMARK.md § Results ledger).

## Run: compare-run-20260802-b

- **Time of check:** 2026-08-02 ~14:45–15:05 IDT (solves) · judged same session
- **Verdict: correctness TIE (0.95 vs 0.90 at k=1 — one-question Δ, within noise) · cost WIN for octocode (0.112× bytes).**
- Provenance: repo SHA `9fed7103` · octocode CLI v18.0.0 (local build:dev) · `rtk` (homebrew) + `gh` authed, shell glue allowed · k=1, single-agent solve-then-judge (not blind) · **no control arm ever run for this suite** (top open item)
- Artifacts: `output/compare-run-20260802-b/` — `octocode-vs-gh-rtk.md`, `ANSWERS.md`, `kpi.json`, `logs/octocode-vs-gh-rtk/*/calls.jsonl`

### Performance comparison matrix

| Metric | A: `rtk`+`gh` | B: octocode | B/A |
|---|---:|---:|---:|
| Correctness — all 10 (primary; no contamination annotations) | 0.90 | **0.95** | +0.05 (noise at k=1) |
| Quality (judge 1–5) | 4.1 | **4.8** | +0.7 |
| Bytes into context | 343,719 | **38,378** | **0.112×** |
| Est. tokens (bytes/4) | 85,929 | **9,594** | 0.112× |
| Tool calls | 28 | **25** | 0.89× |
| Tool wall-clock | **13.8 s** | 32.7 s | 2.4× |
| False confidence | 0 | 0 | — |

### Per-question matrix

| Q | Topic | A corr | B corr | A bytes | B bytes | B/A |
|---|---|---:|---:|---:|---:|---:|
| 1 | zustand example + peer dep | 1.0 | 1.0 | 7,542 | 2,830 | 0.38 |
| 2 | route→regex | 1.0 | 1.0 | 13,513 | 2,802 | 0.21 |
| 3 | vue PR #15035 | 1.0 | 1.0 | 1,594 | 2,958 | 1.9 ▲ |
| 4 | v8StackLineRe live-bug | 1.0 | 1.0 | 13,571 | 1,660 | 0.12 |
| 5 | vscode dispatch | **0.5** | **0.5** | 57,651 | 3,919 | 0.07 |
| 6 | Vue vdom vs Svelte | 1.0 | 1.0 | 75,050 | 5,367 | 0.07 |
| 7 | node streams wiring | 1.0 | 1.0 | 39,944 | 4,525 | 0.11 |
| 8 | esbuild architecture | 1.0 | 1.0 | 22,927 | 2,667 | 0.12 |
| 9 | fastify lifecycle | **0.5** | **1.0** | 32,284 | 6,507 | 0.20 |
| 10 | redis BITFIELD DoS | 1.0 | 1.0 | 79,643 | 5,143 | 0.06 |

▲ = baseline cheaper. Q5: **both arms failed identically** (`_dispatch` vs GT's `dispatchEvent`) — flagged as ambiguous wording in the run's QUESTION_AUDIT.md. Q9: B grounded the lifecycle in `docs/Reference/Lifecycle.md`; A inferred from the (differently-ordered) `lifecycleHooks` array.

### Conclusion

rtk closes much of the raw-token gap vs plain gh, and a skilled shell agent matches octocode on famous-fact correctness — but octocode still reads **9× fewer bytes into context** at equal-or-better correctness, and the one correctness gap that did appear (Q9) was a *grounding* difference, exactly what the tools are for. Cost win improved vs prior run (0.112× vs 0.24×) thanks to matchString/symbols replacing whole-file pulls. Watch item: wall-clock ratio worsened (2.4× vs 1.4×) — node startup + clone/LSP warmup dominated Q7. Next run needs: control arm (C), ≥3 solvers, Q5 re-oracled.

## Run: compare-run-20260802-c (subagent re-run, blind judge)

- **Time of check:** 2026-08-02 16:36–17:01 IDT · independent solvers + blind judge
- **Verdict: baseline WIN this sample** — the `gh --jq` worker was exceptional: 14 calls, ~11.9 KB read, correctness **1.00**, quality 5.0; octocode worker 0.95/4.5 at ~43 KB (lost 0.5 on Q5 vscode dispatch, same as every prior solver). Across 3 runs the suite is a **genuine TIE**: 0.90/0.90 → 0.95/0.90 → 0.95/1.00; byte direction flips with accounting (raw-stdout favors octocode 9×; self-reported read favors gh+jq 3.6×). Settling this needs k≥3 and runner L3 tokens.

## Prior runs

| Run | Date | Verdict | Corr B vs A | Bytes B/A | Wall-clock B/A |
|---|---|---|---|---|---|
| compare-run-20260802 | 2026-08-02 (am) | TIE | 0.90 vs 0.90 | 0.24× | 1.4× |
| compare-run-20260802-b | 2026-08-02 (pm) | TIE (cost WIN) | 0.95 vs 0.90 | 0.112× | 2.4× |
| compare-run-20260802-c (subagents, blind judge) | 2026-08-02 (eve) | **baseline WIN this sample** | 0.95 vs **1.00** | B ~43 KB vs A ~11.9 KB read-bytes (RES 0.24×) |
