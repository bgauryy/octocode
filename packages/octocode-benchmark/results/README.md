# Results

Finished benchmark write-ups (one file per run). Write each using [`../REPORT_TEMPLATE.md`](../REPORT_TEMPLATE.md): a per-question table (correctness, research depth, workflow, chars in/out) and a summary of all.

**File naming:** `<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` — the comparison name, the run's start time `HHMMSS` (24-hour, so same-day runs never collide), then the date. Example: `octocode-vs-gh-rtk-021054-2026-08-05.md`.

| Write-up | Comparison | Notes |
|---|---|---|
| [octocode-vs-gh-headroom-023223-2026-08-05.md](octocode-vs-gh-headroom-023223-2026-08-05.md) | octocode-vs-gh-headroom | One pass, 15/17 gradeable (Q15/Q17 excluded — Arm-B runner bail, not an octocode limit; verified octocode answers Q15 in 1 call), orchestrated (2 isolated runners + 1 blind grader). Correctness near-tie (A 9.07 / B 9.67). Decided on leanness: **octocode ~5.8× leaner** (203,708 vs 1,180,822 chars; leaner on 11/15) even though Arm A's `gh` output was Headroom-compressed every call (13–47%/call, measured). Headroom can't offset whole-file/tree/diff pulls (Q4 652k, Q10 125k, Q2 82k). Arm A had 2 confidently-wrong (Q7, Q14 package.json peer fields); B had 0. gh+headroom won 3 (Q5, Q11, Q13). |
| [octocode-vs-gh-rtk-021054-2026-08-05.md](octocode-vs-gh-rtk-021054-2026-08-05.md) | octocode-vs-gh-rtk | Full pass, all 17, orchestrated (2 isolated runners + 1 blind grader per question). Correctness a near-tie (A 9.18 / B 9.38 mean; equal on 10). Decided on the leanness tiebreak: **octocode ~62% leaner overall** (395,644 vs 1,040,783 chars; leaner on 10/17). Grader preference B 8 / A 4 / tie 5. `gh`+`rtk` chars inflated by whole-file/diff pulls (Q5 326k, Q2 164k, Q11 123k, Q17 106k). Shared miss: both mislabeled a vite devDependency on Q14. |
| [octocode-vs-gh-rtk-224513-2026-08-04.md](octocode-vs-gh-rtk-224513-2026-08-04.md) | octocode-vs-gh-rtk | Full by-hand pass, all 17 questions (`octocode v17.0.1` vs `gh 2.76` + `rtk 0.41`). Correctness tied 10/10 both arms. On chars: **octocode ~30% leaner overall** (168,773 vs 242,691), leaner on 13/17 — driven by region-targeted reads on large files/manifests gh can only fetch whole (Q2, Q9, Q12, Q16). `gh`+`rtk` leaner on pure locate/diff-dump (Q5, Q11, Q13, Q15). Structural finding: rtk silently truncated `gh pr diff` on Q5. |

Efficiency is measured in **characters** (raw CLI output pulled into context). Run by hand from the markdown questions — see [`../INSTRUCTIONS.md`](../INSTRUCTIONS.md).
