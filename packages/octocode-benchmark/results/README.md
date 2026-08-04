# Results

Finished benchmark write-ups (one file per run). Write each using [`../REPORT_TEMPLATE.md`](../REPORT_TEMPLATE.md): a per-question table (correctness, research depth, workflow, chars in/out) and a summary of all.

| Write-up | Comparison | Notes |
|---|---|---|
| [octocode-vs-gh-rtk-full-2026-08-04.md](octocode-vs-gh-rtk-full-2026-08-04.md) | octocode-vs-gh-rtk | Full by-hand pass, all 17 questions (`octocode v17.0.1` vs `gh 2.76` + `rtk 0.41`). Correctness tied 10/10 both arms. On chars: **octocode ~30% leaner overall** (168,773 vs 242,691), leaner on 13/17 — driven by region-targeted reads on large files/manifests gh can only fetch whole (Q2, Q9, Q12, Q16). `gh`+`rtk` leaner on pure locate/diff-dump (Q5, Q11, Q13, Q15). Structural finding: rtk silently truncated `gh pr diff` on Q5. |
| [octocode-vs-gh-rtk-campaign-03.md](octocode-vs-gh-rtk-campaign-03.md) | octocode-vs-gh-rtk | Full by-hand pass, all 17 questions. Correctness tied 10/10 both arms. On chars: `gh`+`rtk` ~33% leaner overall (locate-style); octocode leaner only on large-manifest region reads (Q7, Q10, Q16). |

Efficiency is measured in **characters** (raw CLI output pulled into context). Run by hand from the markdown questions — see [`../INSTRUCTIONS.md`](../INSTRUCTIONS.md).
