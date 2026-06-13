# Benchmark Summary — Octocode CLI vs RTK CLI

**Repo under test:** `rtk-ai/rtk` · **Questions:** 20 · **Judge:** claude-sonnet-4-6

---

## Run Totals

| Metric | Octocode | RTK | Winner |
|--------|----------|-----|--------|
| Total tool calls | 96 | 100 | **Octocode** (−4) |
| Total chars (in + out) | 252,618 | 3,587,154 | **Octocode** (14.2× less) |
| Approx tokens | ~63K | ~897K | **Octocode** (14.2× less) |
| Quality score | **96/100** | **87/100** | **Octocode** (+9) |
| Chars per quality point | 2,631 | 41,232 | **Octocode** (15.7× more efficient) |
| Questions with perfect score (5/5) | 16/20 | 13/20 | **Octocode** |

---

## Per-Question Score Table

Scoring: 5 = complete and correct · 4 = mostly correct / minor gap · 3 = partial · 2 = incomplete · 1 = wrong

| Q | Dimension | Octo Score | RTK Score | Octo Calls | RTK Calls | Octo Chars | RTK Chars |
|---|-----------|:----------:|:---------:|:----------:|:---------:|:----------:|:---------:|
| 1 | Result completeness | **5** | **5** | 7 | 20 | 9,585 | 216,514 |
| 2 | Result completeness | **4** | 3 | 14 | 6 | 22,640 | 195,848 |
| 3 | Comment search | **5** | **5** | 4 | 6 | 6,787 | 194,327 |
| 4 | Comment search | **4** | **4** | 3 | 3 | 3,489 | 48,420 |
| 5 | Prose/code content | **5** | **5** | 1 | 1 | 8,171 | 11,320 |
| 6 | Prose/code content | **5** | **5** | 3 | 11 | 3,293 | 63,525 |
| 7 | Prose/code content | **5** | **5** | 1 | 1 | 8,183 | 11,157 |
| 8 | Prose/code content | **5** | **5** | 1 | 1 | 9,320 | 20,169 |
| 9 | Prose/code content | **5** | **5** | 6 | 1 | 24,713 | 76,113 |
| 10 | Comment preservation | **5** | 4 | 1 | 1 | 2,947 | 10,236 |
| 11 | Comment preservation | **5** | **5** | 2 | 2 | 6,295 | 45,901 |
| 12 | Comment preservation | **5** | **5** | 1 | 1 | 2,443 | 10,236 |
| 13 | Directory structure | 4 | **5** | 5 | 11 | 10,258 | 64,464 |
| 14 | Directory structure | **5** | **5** | 5 | 5 | 9,206 | 312,758 |
| 15 | File metadata | **4** | 3 | 5 | 1 | 8,473 | 170,458 |
| 16 | Exhaustive counting | **5** | 3 | 4 | 5 | 10,007 | 737,481 |
| 17 | File metadata | **5** | 1 | 2 | 8 | 4,138 | 397,561 |
| 18 | PR body + metadata | **5** | **5** | 1 | 2 | 2,890 | 3,261 |
| 19 | PR body + metadata | **5** | **5** | 16 | 10 | 64,831 | 654,105 |
| 20 | PR body + metadata | **5** | 4 | 17 | 4 | 45,347 | 343,300 |
| **Σ** | | **96 / 100** | **87 / 100** | **96** | **100** | **252,618** | **3,587,154** |

---

## Dimension Scores

| Dimension | Questions | Octocode | RTK | Notes |
|-----------|-----------|:--------:|:---:|-------|
| Result completeness | Q1, Q2, Q16 | **14/15** | 11/15 | RTK missed call-site counts (Q2), gave estimate range (Q16) |
| Comment search | Q3, Q4 | 9/10 | 9/10 | Both identified annotations correctly; Q4 edge case on unsafe |
| Prose/code content | Q5–Q9 | **25/25** | **25/25** | Perfect tie |
| Comment preservation | Q10–Q12 | **15/15** | 14/15 | RTK inferred behavior instead of quoting text (Q10) |
| Directory structure | Q13, Q14 | 9/10 | **10/10** | RTK's local clone caught `mvn_cmd.rs` (Q13) |
| File metadata | Q15, Q17 | **9/10** | 4/10 | RTK's stale clone returned wrong files on Q17 |
| PR body + metadata | Q18, Q19, Q20 | **15/15** | 14/15 | Octocode more thorough on label survey (Q20) |
| **TOTAL** | **20 Qs** | **96/100** | **87/100** | |

---

## Key Findings

### Efficiency

RTK consumed **14.2× more chars** than Octocode (3.59M vs 253K) while scoring 9 points lower. RTK reads full files via `rtk read` on the local clone — Q16 alone returned 737K chars. Octocode's structured API responses return curated slices: Q16 used 10K chars for the same exhaustive test count.

At equal quality, RTK costs ~15.7× more in LLM token budget per answer.

### Quality

**Octocode leads:**
- **File metadata (Q15, Q17):** Live GitHub PR API returns current commit data. RTK's clone was 3 days stale on Q17, returning entirely wrong files (4/10 vs 9/10 on this dimension).
- **Exhaustive counting (Q16):** Octocode counted exactly 2,085 `#[test]` functions; RTK gave a 1,200–1,500 estimate due to result caps.
- **PR metadata (Q20):** Structured label data from `githubSearchPullRequests` vs limited passthrough output.

**RTK leads:**
- **Directory structure (Q13):** RTK's clone reflected the post-merge state with `mvn_cmd.rs` present; Octocode missed this file.

**Tie:**
- Prose/code content (Q5–Q9): Perfect 5/5 for both.
- PR body content (Q18, Q19): Both navigated PR archaeology correctly.

---

## Winner

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║   WINNER:  OCTOCODE CLI                                          ║
║                                                                  ║
║   Quality:   96/100  vs  87/100  (+9 points)                    ║
║   Calls:     96  vs  100  (−4)                                   ║
║   Tokens:    ~63K  vs  ~897K  (14.2× more efficient)            ║
║   Chars/pt:  2,631  vs  41,232  (15.7× more token-efficient)    ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

Octocode wins on both accuracy and efficiency. RTK's local clone is a strength for freshly-cloned repos (found `mvn_cmd.rs` that Octocode missed) but becomes a liability for time-sensitive metadata — a 3-day-old clone returned catastrophically wrong results on Q17. Octocode's GitHub API tools provide live data at a fraction of the token cost.
