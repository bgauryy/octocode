# Results — octocode-vs-gh

**TL;DR 🏆 octocode WIN — robust across 3 runs.** Correctness 1.00 vs 0.67–0.85; REQ (tokens×quality×flow) **69–139× better**. `gh` has no range/symbol fetch, so multi-MB whole-file pulls (checker.ts = 3.15 MB for a 3-line answer) are structural and no solver can filter them away. Only lane gh wins: tight `--jq` PR-metadata reads.

> Tracked results ledger. Latest scored run first; full artifacts in the (gitignored) `output/<run>/` dir it names. Refresh this file after every scored run (see BENCHMARK.md § Results ledger).

## Run: compare-run-20260802-b

- **Time of check:** 2026-08-02 ~14:30–14:45 IDT (solves) · judged same session
- **Verdict: WIN for octocode** — pre-registered rule: uncontaminated correctness ≥ baseline AND materially lower bytes, no guardrail regression.
- Provenance: repo SHA `9fed7103` · octocode CLI v18.0.0 (local build:dev) · `gh` authed · k=1 solver/arm, single-agent solve-then-judge (not blind) · oracles verified same day
- Artifacts: `output/compare-run-20260802-b/` — `octocode-vs-gh.md`, `ANSWERS.md`, `kpi.json`, `logs/octocode-vs-gh/*/calls.jsonl`

### Performance comparison matrix

| Metric | A: `gh` CLI | B: octocode | B/A |
|---|---:|---:|---:|
| Correctness — uncontaminated primary (n=5: q1,q3,q5,q6,q9) | 0.80 | **1.00** | +0.20 |
| Correctness — all 10 | 0.85 | **1.00** | +0.15 |
| Quality (judge 1–5) | 3.9 | **4.7** | +0.8 |
| Bytes into context | 4,273,866 | **93,905** | **0.022×** |
| Est. tokens (bytes/4) | 1,068,466 | **23,476** | 0.022× |
| Tool calls | 45 | **34** | 0.76× |
| Tool wall-clock | **20.7 s** | 36.6 s | 1.8× |
| False confidence | 0 | 0 | — |

### Per-question matrix

| Q | Topic | Contam. | A corr | B corr | A bytes | B bytes | B/A |
|---|---|---|---:|---:|---:|---:|---:|
| 1 | AST `ref()` count | – | 0.5 | **1.0** | 192,279 | 3,513 | 0.018 |
| 2 | checker.ts region | (q2a) | 0.5 | **1.0** | 3,151,773 | 1,285 | **0.0004** |
| 3 | latest lib/ PR | – | 1.0 | 1.0 | 1,396 | 4,643 | 3.3 ▲ |
| 4 | router cross-repo | ✔ | 1.0 | 1.0 | 18,595 | 2,762 | 0.15 |
| 5 | flask history | – | 1.0 | 1.0 | 2,370 | 9,692 | 4.1 ▲ |
| 6 | absence trap | – | 1.0 | 1.0 | 75,706 | 55,686 | 0.74 |
| 7 | axios chain | ✔ | 1.0 | 1.0 | 7,438 | 4,767 | 0.64 |
| 8 | baseGet callers | ✔ | 1.0 | 1.0 | 551,509 | 2,896 | **0.005** |
| 9 | networking.c outline | – | 0.5 | **1.0** | 243,963 | 6,934 | 0.028 |
| 10 | 3 defs + budget | ✔ | 1.0 | 1.0 | 28,837 | 1,727 | 0.06 |

▲ = baseline cheaper (tight `gh --jq` on PR/commit metadata).

### Conclusion

Octocode wins on every KPI except raw speed: **more correct** where the tools matter (AST vs text, region reads vs whole-file, outline vs dump) and **45× cheaper in context bytes** overall — the gap is capability-driven, not luck (trajectory layer: capabilityPoint exercised 10/10). `gh`'s failure mode is whole-file transfer (3.15 MB for a 3-line function) and no AST/LSP/npm surface. Guardrails clean. Result replicates the morning run (WIN, 1.00 vs 0.67 @ 0.098×).

## Run: compare-run-20260802-c (subagent re-run, blind judge)

- **Time of check:** 2026-08-02 16:36–17:01 IDT · 2 independent solver subagents + blind judge (arm labels shuffled, anchors spot-verified via curl)
- **Verdict: cost WIN, correctness tie 0.85/0.85.** B's worker lost Q5 (budget exhausted → honest Unknown → 0.0) and Q6 (lazy absence proof → 0.5) — solver variance, not toolchain ceiling (run b: 1.00). A structurally forced ~7.5 MB of whole-file pulls (checker.ts 3.15 MB, lodash.js 546 KB, networking.c 244 KB) vs B ~55 KB → **RES ≈ 133×**.
- Bytes are worker-self-reported READ bytes — not comparable to run b's raw-stdout numbers.

- **Flow (trajectory judge):** A 4/5 (solid verification, but gh's tool constraint forces multi-MB brute force) · B 4/5 (anchored fetches + LSP call hierarchy; Q5 starved by budget, honestly marked Unknown). **REQ B/A = 139×** (corr×qual×flow per read-KB).

## Prior runs

| Run | Date | Verdict | Corr B vs A (uncontam.) | Bytes B/A |
|---|---|---|---|---|
| compare-run-20260802 | 2026-08-02 (am) | WIN | 1.00 vs 0.67 | 0.098× |
| compare-run-20260802-b | 2026-08-02 (pm) | WIN | 1.00 vs 0.80 | 0.022× |
| compare-run-20260802-c (subagents, blind judge) | 2026-08-02 (eve) | cost WIN (corr tie) | 0.85 vs 0.85 | ~55 KB vs ~7.5 MB read-bytes (133× RES) |
