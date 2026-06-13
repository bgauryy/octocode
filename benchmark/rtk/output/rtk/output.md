# Run rtk

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| rtk | 20 / 20 | 100 | 5,938 | 3,581,216 | 3,587,154 | 896,789 | 46,460 | 860,483 | 814,023 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 20 | 933 | 215,581 | 216,514 | 54,129 | 5,697 | 37,595 | 31,898 | `filter_markdown_body` appears in 2 source files (plus READ… |
| Q2 | 6 | 618 | 195,230 | 195,848 | 48,962 | 3,723 | 38,324 | 34,601 | RunOptions builder method call sites across all source file… |
| Q3 | 6 | 559 | 193,768 | 194,327 | 48,582 | 3,347 | 29,564 | 26,217 | All TODO, FIXME, HACK annotations found in `src/`: |
| Q4 | 3 | 291 | 48,129 | 48,420 | 12,105 | 1,587 | 207,853 | 206,266 | There is 1 `// SAFETY:` (case-insensitive: `// Safety:`) co… |
| Q5 | 1 | 51 | 11,269 | 11,320 | 2,830 | 744 | 10,858 | 10,114 | ## TOML Filter Pipeline Stages (from `src/core/README.md`) |
| Q6 | 11 | 522 | 63,003 | 63,525 | 15,882 | 4,121 | 42,463 | 38,342 | `src/` has only `main.rs` (no `lib.rs`). The top-level `mod… |
| Q7 | 1 | 44 | 11,113 | 11,157 | 2,790 | 479 | 19,436 | 18,957 | ## 1. Trusted vs Untrusted Inputs |
| Q8 | 1 | 57 | 20,112 | 20,169 | 5,043 | 498 | 26,542 | 26,044 | ## 1. What parts of git diff does rtk keep? |
| Q9 | 1 | 55 | 76,058 | 76,113 | 19,029 | 735 | 43,897 | 43,162 | ## `gh` subcommands rtk intercepts (from `src/cmds/git/gh_c… |
| Q10 | 1 | 51 | 10,185 | 10,236 | 2,559 | 368 | 20,005 | 19,637 | ## 1. When should `skip_filter_on_failure` be true? |
| Q11 | 2 | 165 | 45,736 | 45,901 | 11,476 | 968 | 59,180 | 58,212 | ## `FilterLevel` Enum (src/core/filter.rs, L8–L12) |
| Q12 | 1 | 51 | 10,185 | 10,236 | 2,559 | 499 | 11,063 | 10,564 | ## `RunOptions` struct (src/core/runner.rs, L17–L27) |
| Q13 | 11 | 502 | 63,962 | 64,464 | 16,116 | 3,957 | 23,196 | 19,239 | ## Subdirectories under `src/cmds/` with `.rs` files (exclu… |
| Q14 | 5 | 277 | 312,481 | 312,758 | 78,190 | 2,116 | 33,638 | 31,522 | ## Files in `src/discover/` |
| Q15 | 1 | 57 | 170,401 | 170,458 | 42,615 | 872 | 20,450 | 19,578 | ## Largest `.rs` file under `src/` |
| Q16 | 5 | 292 | 737,189 | 737,481 | 184,371 | 3,070 | 51,373 | 48,303 | ## `#[test]` function counts across `src/` |
| Q17 | 8 | 526 | 397,035 | 397,561 | 99,391 | 3,821 | 46,599 | 42,778 | ## 5 Most Recently Modified `.rs` Files Under `src/` |
| Q18 | 2 | 65 | 3,196 | 3,261 | 816 | 1,468 | 20,731 | 19,263 | ## PR #2129: `fix(gh): show fallback note when PR/issue bod… |
| Q19 | 10 | 582 | 653,523 | 654,105 | 163,527 | 6,247 | 77,509 | 71,262 | ## PR that introduced `--ultra-compact` / `-u` flag |
| Q20 | 4 | 240 | 343,060 | 343,300 | 85,825 | 2,143 | 40,207 | 38,064 | ## Labels on 10 Most Recently Updated PRs in `rtk-ai/rtk` |
| **Σ** | **100** | **5,938** | **3,581,216** | **3,587,154** | **896,789** | **46,460** | **860,483** | **814,023** | |
