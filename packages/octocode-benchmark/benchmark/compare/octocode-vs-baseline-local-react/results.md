# Results — octocode-vs-baseline-local-react

**TL;DR 📊 split decision (first scored run).** octocode more correct (**0.95 vs 0.85** — wins AST-precision and semantic-reference questions); filtered bare-POSIX cheaper on read-bytes (REQ B/A 0.25×). Headline engine finding: octocode's structural/AST + documentSymbols are unreliable on Flow-typed `.js` (silent undercounts, honestly disclosed by the worker) — top work item.

> Tracked results ledger. Latest scored run first; full artifacts in the (gitignored) `output/<run>/` dir it names. Refresh this file after every scored run (see BENCHMARK.md § Results ledger).

## Run: compare-run-20260802-c (first scored run — subagents, blind judge)

- **Time of check:** 2026-08-02 16:36–17:01 IDT · 2 independent solver subagents (`pi -p`, ≤40 cmds, no GT access) + blind judge (arm labels shuffled)
- **Verdict: octocode more correct (0.95 vs 0.85); bare POSIX cheaper on self-reported read-bytes (RES B/A ≈ 0.25×).** Split decision — the suite's precision questions (AST call semantics, semantic references) are where POSIX loses; its cheap questions are where filtered grep is unbeatable on bytes.
- Provenance: corpus `context/react` @ `9ceb1e7d` · octocode CLI v18.0.0 (local build:dev) · k=1 · bytes are worker-self-reported READ bytes

### Performance comparison matrix

| Metric | A: bare POSIX | B: octocode | Note |
|---|---:|---:|---|
| Correctness — all 10 | 0.85 | **0.95** | B wins Q1 (AST one-arg call semantics) and Q5 (function census) |
| Quality (judge 1–5) | **4.5** | 4.1 | A's answers unusually well-anchored for grep |
| Self-reported read bytes | **~11.1 KB** | ~46 KB | A's filtering is extremely lean |
| Tool calls | **14** | 48 | B spent calls on byte-size binary-search probes (Q7) |
| False confidence | 0 | 0 | |

### Per-question correctness (A / B)

Q1 0.5/1.0 · Q2 1.0/1.0 · Q3 1.0/1.0 · Q4 1.0/1.0 · Q5 0.5/1.0 · Q6 1.0/1.0 · Q7 1.0/1.0 · Q8 0.5/0.5 · Q9 1.0/1.0 · Q10 1.0/1.0

- **Flow (trajectory judge):** A 4/5 (disciplined POSIX, cap-free) · B 4/5 (good routing + honest AST disclosure; ~10 wasteful byte-probe calls on Q7). **REQ B/A = 0.25×.**

### Conclusion

octocode's precision surfaces (structural counts, LSP references) win the questions grep can only approximate — but the worker itself disclosed that **octocode's structural/AST mode and documentSymbols are unreliable on Flow-typed `.js`** (silent undercounts on Q1/Q5/Q10; column-0 text counting used as fallback). That Flow limitation is now the suite's headline engine finding (confirmed independently by the ast-grep suite's workers). Q8 stumped both arms (0.5/0.5) — audit its wording/oracle before the next run. Next run: k≥3, raw-stdout accounting alongside read-bytes, control arm.

## Prior runs

| Run | Date | Verdict | Corr B vs A | Notes |
|---|---|---|---|---|
| compare-run-20260802-c | 2026-08-02 | B more correct / A cheaper | 0.95 vs 0.85 | first scored run; subagents + blind judge |
