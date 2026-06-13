# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| octocode | 20 / 20 | 107 | 22,870 | 590,389 | 613,259 | 153,315 | 80,859 | 2,024,244 | 1,943,385 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 7 | 1,215 | 8,370 | 9,585 | 2,397 | 2,575 | 610,573 | 607,998 | `filter_markdown_body` appears in 3 files (2 with actual ca… |
| Q2 | 14 | 2,744 | 19,896 | 22,640 | 5,660 | 4,927 | 111,702 | 106,775 | RunOptions builder method call counts across all `.rs` file… |
| Q3 | 4 | 798 | 5,989 | 6,787 | 1,697 | 1,379 | 37,381 | 36,002 | 3 annotation-bearing locations found in `src/` (1 false-pos… |
| Q4 | 3 | 526 | 2,963 | 3,489 | 873 | 1,000 | 24,331 | 23,331 | **No `// SAFETY:` comments found in `src/`.** |
| Q5 | 1 | 186 | 7,985 | 8,171 | 2,043 | 331 | 12,443 | 12,112 | From `src/core/README.md`, the TOML filter pipeline applies… |
| Q6 | 3 | 595 | 2,698 | 3,293 | 824 | 966 | 34,183 | 33,217 | **Note:** `src/lib.rs` does not exist — the project is a bi… |
| Q7 | 1 | 195 | 7,988 | 8,183 | 2,046 | 483 | 18,273 | 17,790 | ## 1. Trusted vs Untrusted Inputs |
| Q8 | 1 | 232 | 9,088 | 9,320 | 2,330 | 347 | 21,743 | 21,396 | ## 1. What rtk keeps from git diff |
| Q9 | 6 | 1,410 | 23,303 | 24,713 | 6,179 | 2,146 | 60,204 | 58,058 | ## `gh` subcommands rtk intercepts (in `run()` dispatch) |
| Q10 | 5 | 1,273 | 16,125 | 17,398 | 4,350 | 9,782 | 65,121 | 55,339 | ## 1. When `skip_filter_on_failure` should be `true` — comm… |
| Q11 | 2 | 437 | 5,858 | 6,295 | 1,574 | 663 | 18,060 | 17,397 | The `FilterLevel` enum is defined in `src/core/filter.rs` (… |
| Q12 | 1 | 238 | 2,205 | 2,443 | 611 | 284 | 9,589 | 9,305 | `RunOptions` struct is defined in `src/core/runner.rs` (lin… |
| Q13 | 5 | 1,032 | 9,226 | 10,258 | 2,565 | 1,715 | 40,121 | 38,406 | ## Subdirectories under `src/cmds/` (10 total) |
| Q14 | 5 | 986 | 8,220 | 9,206 | 2,302 | 1,683 | 40,150 | 38,467 | ## `src/discover/` — 7 files |
| Q15 | 5 | 1,039 | 7,434 | 8,473 | 2,119 | 1,873 | 49,063 | 47,190 | **Largest `.rs` file: `src/hooks/init.rs`** |
| Q16 | 4 | 722 | 9,285 | 10,007 | 2,502 | 1,464 | 47,120 | 45,656 | ## Top 5 files by `#[test]` count |
| Q17 | 9 | 2,092 | 107,963 | 110,055 | 27,514 | 11,895 | 314,431 | 302,536 | **Note:** The `localFindFiles` tool does not return modific… |
| Q18 | 1 | 219 | 2,671 | 2,890 | 723 | 1,802 | 16,009 | 14,207 | ## PR #2129: "fix(gh): show fallback note when PR/issue bod… |
| Q19 | 29 | 6,726 | 290,149 | 296,875 | 74,219 | 32,963 | 451,180 | 418,217 | ## Q19: PR Introducing `--ultra-compact` / `-u` Flag |
| Q20 | 1 | 205 | 42,973 | 43,178 | 10,795 | 2,581 | 42,567 | 39,986 | ## Q20: Labels on 10 Most Recently Updated PRs in rtk-ai/rtk |
| **Σ** | **107** | **22,870** | **590,389** | **613,259** | **153,315** | **80,859** | **2,024,244** | **1,943,385** | |
