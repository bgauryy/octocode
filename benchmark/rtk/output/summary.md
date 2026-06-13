# Benchmark Summary — Octocode CLI vs RTK CLI
## `rtk-ai/rtk` · 20 Questions · Judge: claude-sonnet-4-6

---

## Run Totals

| Metric | octocode | rtk | Winner |
|--------|----------|-----|--------|
| Questions answered | 20 / 20 | 20 / 20 | tie |
| Total tool calls | 107 | 100 | rtk (−7) |
| **Total chars (in + out)** | **613,259** | **3,587,154** | **octocode (−83%)** |
| Approx tokens | ~153K | ~897K | octocode (−83%) |
| Tool elapsed ms | 80,859 ms | 46,460 ms | rtk (−43%) |
| Q wall ms (incl. reasoning) | 2,024 s | 860 s | rtk (−57%) |

> RTK used 5.85× more characters than octocode to answer the same 20 questions.
> Octocode was slower in wall-clock time because it ran structured API queries with more selective targeted reads.

---

## Score by Question

Scoring: 5 = complete and correct · 4 = mostly correct / minor gap · 3 = partial · 2 = incomplete · 1 = wrong

| Q | Dimension | octocode | rtk | octo chars | rtk chars | Efficiency winner |
|---|-----------|:--------:|:---:|----------:|----------:|-------------------|
| Q1 | Search: exhaustive callers | **5** | **5** | 9,585 | 216,514 | octocode (−96%) |
| Q2 | Search: builder methods | **4** | 3 | 22,640 | 195,848 | octocode (−88%) |
| Q3 | Comment-as-target: TODO/FIXME | **5** | **5** | 6,787 | 194,327 | octocode (−97%) |
| Q4 | Comment-as-target: SAFETY | **5** | 4 | 3,489 | 48,420 | octocode (−93%) |
| Q5 | Prose: filter pipeline | **5** | 4 | 8,171 | 11,320 | octocode (−28%) |
| Q6 | Code: mod declarations | **5** | **5** | 3,293 | 63,525 | octocode (−95%) |
| Q7 | Prose: SECURITY.md | **5** | **5** | 8,183 | 11,157 | octocode (−27%) |
| Q8 | Large file: diff_cmd.rs | **5** | **5** | 9,320 | 20,169 | octocode (−54%) |
| Q9 | Large file: gh_cmd.rs dispatch | **5** | **5** | 24,713 | 76,113 | octocode (−68%) |
| Q10 | Comment preservation | **5** | 4 | 17,398 | 10,236 | rtk (−41%) |
| Q11 | Comment preservation: FilterLevel | **5** | **5** | 6,295 | 45,901 | octocode (−86%) |
| Q12 | Comment preservation: RunOptions | **5** | **5** | 2,443 | 10,236 | octocode (−76%) |
| Q13 | Structure: src/cmds/ | **5** | **5** | 10,258 | 64,464 | octocode (−84%) |
| Q14 | Structure: src/discover/ | **5** | **5** | 9,206 | 312,758 | octocode (−97%) |
| Q15 | Metadata: largest .rs file | **5** | 2 | 8,473 | 170,458 | octocode (−95%) |
| Q16 | Exhaustive count: #[test] | 4 | 3 | 10,007 | 737,481 | octocode (−99%) |
| Q17 | Metadata: recently modified | 4 | 4 | 110,055 | 397,561 | octocode (−72%) |
| Q18 | PR body | **5** | **5** | 2,890 | 3,261 | octocode (−11%) |
| Q19 | PR search: ultra-compact | 2 | 3 | 296,875 | 654,105 | octocode (−55%) |
| Q20 | PR labels | 3 | 3 | 43,178 | 343,300 | octocode (−87%) |
| **Σ** | | **92 / 100** | **86 / 100** | **613,259** | **3,587,154** | **octocode wins 19/20** |

---

## Dimension Scores

| Dimension | Q | octocode | rtk | Notes |
|-----------|---|:--------:|:---:|-------|
| **Search completeness** | Q1, Q2, Q3, Q4, Q16 | 23/25 | 21/25 | RTK result caps + case sensitivity cost points |
| **Prose & code content** | Q5, Q6, Q7, Q8, Q9 | 25/25 | 24/25 | RTK Q5 truncated at stage 6 of 8 |
| **Comment preservation** | Q10, Q11, Q12 | 15/15 | 14/15 | Q10: RTK derived from code, didn't quote `//` comment text |
| **Directory structure** | Q13, Q14 | 10/10 | 10/10 | Tie |
| **File metadata** | Q15, Q17 | 9/10 | 6/10 | RTK misidentified largest file (registry.rs vs init.rs) |
| **PR body + metadata** | Q18, Q19, Q20 | 10/15 | 11/15 | RTK narrowly ahead on PR search (found PR #10 candidate); both struggled with Q20 labels |

---

## Key Findings

### 1. Efficiency — Octocode wins by 5.85×

Octocode's structured, targeted reads consumed **613K chars** vs RTK's **3.59M chars**. The gap is largest on:

| Q | Dimension | octo chars | rtk chars | RTK penalty factor |
|---|-----------|----------:|----------:|------------------:|
| Q16 | #[test] exhaustive count | 10,007 | 737,481 | **73.7×** |
| Q14 | discover/ structure | 9,206 | 312,758 | **34.0×** |
| Q19 | ultra-compact PR search | 296,875 | 654,105 | 2.2× |
| Q3 | TODO/FIXME search | 6,787 | 194,327 | **28.6×** |

The cause: `rtk grep` (v0.41.0 has no `rtk rg`) and `rtk read` return full file dumps through the GitHub API (local clone had no actual `.rs` files checked out), inflating output chars 20–70× vs octocode's per-query targeted responses.

### 2. Answer Quality — Octocode wins 92 vs 86

Octocode outperformed on:
- **Q2** — Exact per-method call counts vs file-level lists only
- **Q4** — Exact `// SAFETY:` case match (none found); RTK found a case-insensitive `// Safety:` variant in `read.rs` (a fallback note, not a Rust safety annotation)
- **Q10** — Quoted the actual `//` comment text in `run_api()` explaining `RunMode::Passthrough`; RTK inferred from code behavior
- **Q15** — Correctly identified `src/hooks/init.rs` (231 KB) as largest file; RTK said `src/discover/registry.rs` (123 KB) — wrong by 2×

RTK outperformed on:
- **Q19** — Found PR #10 as the `--ultra-compact` candidate; octocode's keyword search did not converge on a specific PR

Both failed equally on:
- **Q20** — Neither tool surfaced actual label metadata from the GitHub API

### 3. Comment Preservation — Draw

Q10–Q12 (the key comment preservation dimension) were effectively a draw. Why? RTK's `Minimal` filter **preserves `///` doc comments** — it only strips `//` and `/* */` inline comments. In this codebase:
- `FilterLevel` enum has **no `///` docs** on variants → both tools correctly said "none found"
- `RunOptions` struct has **no struct-level `///` doc** and only `inherit_stdin` has a `///` comment → both quoted it identically
- Q10 (`//` inline comments in run_api()) is where RTK's filter made a difference — octocode cited the comment text verbatim; RTK derived the answer from code logic alone

### 4. Infrastructure Issues — RTK Run

- `rtk rg` does not exist in v0.41.0 — the researcher had to use `rtk gh api search/code` (GitHub Code Search API), which returns full file dumps
- `/tmp/rtk-bench` had no actual `.rs` source files checked out (directory structure only), forcing all local ops to use the GitHub API
- Both issues dramatically inflated RTK's char count; a working local clone + `rtk grep` would have been far more efficient

---

## Final Verdict

| Category | Winner | Margin |
|----------|--------|--------|
| **Answer quality** | **octocode** | 92 vs 86 (+7%) |
| **Token efficiency** | **octocode** | 613K vs 3,587K (−83%) |
| **Tool speed** (raw API ms) | **rtk** | 46s vs 81s (−43%) |
| **Wall clock** | **rtk** | 860s vs 2,024s (−57%) |

**octocode wins overall** — higher answer quality AND dramatically lower character cost. The efficiency gap (5.85×) is the dominant finding: structured targeted reads cost a fraction of full-file dumps.

RTK's primary advantages are **tool latency** (faster raw API calls) and **PR body retrieval speed** (Q18: 1 call, 1,838 out chars vs octocode's 2 calls, 4,763 out chars). On questions where one targeted call suffices, RTK is snappier.

The practical recommendation: use octocode for deep code research and exhaustive analysis; RTK's filter model excels for quick one-shot terminal lookups where one full-file read is all you need.
