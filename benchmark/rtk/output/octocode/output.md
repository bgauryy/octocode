# Run octocode

| Agent | Questions | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|
| octocode | 20 / 20 | 49 | 5,853 | 644,590 | 650,443 | 162,611 | 43,142 | 599,718 | 556,576 |

> **Total Chars** = per-question `in_chars + out_chars`. **Approx Tokens** = `ceil(Total Chars / 4)` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
| Q1 | 4 | 678 | 9,111 | 9,789 | 2,448 | 977 | 45,703 | 44,726 | # Q1 — Exhaustive callers of `filter_markdown_body` |
| Q2 | 2 | 371 | 8,154 | 8,525 | 2,132 | 525 | 19,830 | 19,305 | # Q2 — All usages of `RunOptions` builder methods |
| Q3 | 3 | 306 | 10,132 | 10,438 | 2,610 | 673 | 26,250 | 25,577 | # Q3 — Architecture intent in `src/core/runner.rs` comments |
| Q4 | 1 | 165 | 1,484 | 1,649 | 413 | 255 | 9,705 | 9,450 | # Q4 — All TODO, FIXME, and HACK comments in `src/` |
| Q5 | 2 | 227 | 10,713 | 10,940 | 2,735 | 484 | 22,368 | 21,884 | # Q5 — Filtering taxonomy documented in `src/core/README.md` |
| Q6 | 1 | 54 | 14,875 | 14,929 | 3,733 | 258 | 16,597 | 16,339 | # Q6 — Command category structure under `src/cmds/` |
| Q7 | 2 | 135 | 6,447 | 6,582 | 1,646 | 468 | 18,874 | 18,406 | # Q7 — Files under `src/discover/` and their purpose |
| Q8 | 3 | 434 | 5,463 | 5,897 | 1,475 | 780 | 29,631 | 28,851 | # Q8 — Largest source file by line count |
| Q9 | 3 | 381 | 37,191 | 37,572 | 9,393 | 5,139 | 53,835 | 48,696 | # Q9 — Five most recently modified `.rs` files in `src/` |
| Q10 | 1 | 88 | 7,212 | 7,300 | 1,825 | 2,335 | 19,390 | 17,055 | # Q10 — PR #2129: the prior fix being re-implemented |
| Q11 | 4 | 396 | 70,742 | 71,138 | 17,785 | 9,724 | 53,184 | 43,460 | # Q11 — The PR that introduced `--ultra-compact` / `-u` |
| Q12 | 2 | 157 | 78,120 | 78,277 | 19,570 | 6,121 | 33,312 | 27,191 | # Q12 — Open PR labels: any breaking changes? |
| Q13 | 2 | 157 | 13,866 | 14,023 | 3,506 | 468 | 39,988 | 39,520 | # Q13 — Full diff filter in `src/cmds/git/diff_cmd.rs` |
| Q14 | 1 | 66 | 8,019 | 8,085 | 2,022 | 236 | 16,085 | 15,849 | # Q14 — `SECURITY.md` threat model |
| Q15 | 4 | 702 | 9,650 | 10,352 | 2,588 | 1,099 | 31,420 | 30,321 | # Q15 — Total `#[test]` functions across all `src/` modules |
| Q16 | 2 | 324 | 12,549 | 12,873 | 3,219 | 479 | 40,252 | 39,773 | # Q16 — Complete `gh` subcommand dispatch table in `src/cmd… |
| Q17 | 3 | 289 | 316,684 | 316,973 | 79,244 | 7,518 | 43,276 | 35,758 | # Q17 — The PR that introduced the hooks system |
| Q18 | 1 | 60 | 1,814 | 1,874 | 469 | 3,513 | 16,929 | 13,416 | # Q18 — npm package named `rtk` |
| Q19 | 3 | 401 | 2,555 | 2,956 | 739 | 769 | 24,744 | 23,975 | # Q19 — Safety annotation comments in `src/` |
| Q20 | 5 | 462 | 19,809 | 20,271 | 5,068 | 1,321 | 38,345 | 37,024 | # Q20 — CI checks in `.github/workflows/` |
| **Σ** | **49** | **5,853** | **644,590** | **650,443** | **162,611** | **43,142** | **599,718** | **556,576** | |
