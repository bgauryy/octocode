## Check brief: Octocode local tools vs smart rg and ast-grep

You check Octocode's local tools against the best a shell expert does with `rg`, `ast-grep` (`sg`),
`sed -n`, `find`, `wc` and `git`. Method and per-tool checklist:
`packages/octocode-benchmark/CHECKS.md`. Read it first. A sanity or end-to-end pass works
through that file's checklists of the same names, one row per item.

**Rules.** No scripts: make each call by hand, read the answer, judge it. A one-line `| wc -c` or `time`
is fine; keep nothing. Use the pinned binary you were given and name its hash.

**For each task, run both sides and record one row:**

| Task | Octocode call | Octocode chars, first / repeat ms | Expert command | Expert chars, ms | Right on both? | Better and why |

Expert side, smart not plain: the command an expert would really type, from
[COMPARISONS.md](../COMPARISONS.md) (one section per tool, with what "better" means).

**Tasks (pick a real target in this repository for each):**
1. Where a word is used in a folder.
2. Which files hold a word; which files lack it.
3. Where a word is used most (counts, top 20).
4. One function's body, short and over 400 lines.
5. Outline of a large file.
6. Lines A–B of a file.
7. A word inside a minified one-line file.
8. Calls of a function, not comments, strings or definitions (method and free calls).
9. Calls inside one function (`inside` rule).
10. Who references a symbol, with roles (lspSearch vs `rg -w`).
11. Largest, longest and newest files under a folder.
12. Most imported files and unused exports of a package (graphQuery vs `rg … | sort | uniq -c`).
13. Which of 300 log lines, 60 files or 200 search hits are about X (clasify vs reading them).

**Judge each row on:** right answer; deciding line on page 1; next call possible from the output; characters
(≈ 4 per token); time; what the expert had to know (flags, patterns, file length). Octocode loses a row
when the expert's answer is right and smaller with no guessing; say so plainly.

**Report:** save it as a capability round per [METHOD.md](../METHOD.md#record): the table, a one-line verdict per task (Octocode / expert / even), every Octocode defect with the
exact call that shows it, and what you did not check. Delete any scratch you wrote; keep only the round folder.
