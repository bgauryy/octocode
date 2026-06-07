# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| octocode | 20 / 20 | 133 | 10,733 | 1,970,411 | 1,981,144 | 495,286 | 253,318 | 1,495,819 | 1,275,320 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 14 | 1,279 | 78,215 | 79,494 | 19,874 | 24,937 | 156,272 | 131,335 | - **`facebook/react` — `useState`**: Defined in `packages/r… |
| Q2 | 7 | 623 | 11,940 | 12,563 | 3,141 | 9,571 | 94,360 | 84,789 | ## RunOptions Builder Method Call Sites in `rtk-ai/rtk` |
| Q3 | 1 | 87 | 7,508 | 7,595 | 1,899 | 1,818 | 64,442 | 62,624 | - `packages/react-reconciler/src/ReactFiberWorkLoop.js` in … |
| Q4 | 30 | 2,293 | 33,740 | 36,033 | 9,009 | 55,275 | 224,205 | 168,930 | ## All TODO, FIXME, and HACK Comments Under `src/` in `rtk-… |
| Q5 | 1 | 59 | 8,003 | 8,062 | 2,016 | 2,016 | 34,658 | 32,642 | ## Filtering Taxonomy in `src/core/README.md` (`rtk-ai/rtk`) |
| Q6 | 1 | 59 | 3,009 | 3,068 | 767 | 3,324 | 32,970 | 29,646 | ## Command Category Structure Under `src/cmds/` in `rtk-ai/… |
| Q7 | 1 | 63 | 4,937 | 5,000 | 1,250 | 1,906 | 22,098 | 20,192 | ## Files Under `src/discover/` in `rtk-ai/rtk` |
| Q8 | 3 | 192 | 2,579 | 2,771 | 693 | 2,660 | 28,372 | 25,712 | ## Largest `.rs` File by Line Count Under `src/` in `rtk-ai… |
| Q9 | 3 | 243 | 50,951 | 51,194 | 12,799 | 7,413 | 56,309 | 48,896 | ## 5 Most Recently Modified `.rs` Files Under `src/` in `rt… |
| Q10 | 1 | 68 | 6,978 | 7,046 | 1,762 | 1,736 | 27,298 | 25,562 | ## PR #2129: `fix(gh): show fallback note when PR/issue bod… |
| Q11 | 5 | 375 | 116,416 | 116,791 | 29,198 | 14,234 | 77,124 | 62,890 | ## PR That Introduced `--ultra-compact` / `-u` |
| Q12 | 4 | 339 | 260,157 | 260,496 | 65,124 | 11,514 | 93,990 | 82,476 | ## Open PR Labels: Breaking Changes? |
| Q13 | 1 | 65 | 14,924 | 14,989 | 3,748 | 1,971 | 62,626 | 60,655 | ## Full Diff Filter in `src/cmds/git/diff_cmd.rs` |
| Q14 | 1 | 52 | 8,006 | 8,058 | 2,015 | 1,872 | 39,684 | 37,812 | ## SECURITY.md Threat Model |
| Q15 | 17 | 1,481 | 30,745 | 32,226 | 8,057 | 30,126 | 196,781 | 166,655 | ## Total `#[test]` Functions in `src/` modules |
| Q16 | 9 | 827 | 13,822 | 14,649 | 3,663 | 17,340 | 97,856 | 80,516 | ## Complete `gh` Subcommand Dispatch Table in `src/cmds/git… |
| Q17 | 13 | 828 | 1,269,552 | 1,270,380 | 317,595 | 32,852 | 33 | 0 | ## PR That Introduced the Hooks System |
| Q18 | 1 | 21 | 837 | 858 | 215 | 3,461 | 11,265 | 7,804 | ## npm Package Named "rtk" |
| Q19 | 6 | 535 | 7,257 | 7,792 | 1,948 | 9,219 | 68,146 | 58,927 | ## `// SAFETY:` Comments in `src/` |
| Q20 | 14 | 1,244 | 40,835 | 42,079 | 10,520 | 20,073 | 107,330 | 87,257 | ## CI Checks in `.github/workflows/` |
| **Σ** | **133** | **10,733** | **1,970,411** | **1,981,144** | **495,286** | **253,318** | **1,495,819** | **1,275,320** | |
