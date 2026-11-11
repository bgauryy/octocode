## Check brief: Octocode local tools vs smart rg and ast-grep

You check Octocode's local tools against the best a shell expert does with `rg`, `ast-grep` (`sg`),
`sed -n`, `find`, `wc` and `git`. Method and per-tool checklist:
`skills-dev/octocode-dev/references/tool-checks.md`. Read it first.

**Rules.** No scripts: make each call by hand, read the answer, judge it. A one-line `| wc -c` or `time`
is fine; keep nothing. Use the pinned binary you were given and name its hash.

**For each task, run both sides and record one row:**

| Task | Octocode call | Octocode chars, first / repeat ms | Expert command | Expert chars, ms | Right on both? | Better and why |

Expert side, smart not plain — the command an expert would really type:
- text: `rg -n -w -F TERM DIR -g '*.rs' -m 20`; counts `rg -c TERM DIR | sort -t: -k2 -rn | head`;
  files `rg -l`; a window in a long line `rg -o -b '.{0,60}TERM.{0,60}' FILE`
- regions: `sed -n 'A,Bp' FILE`, `rg -n -A40 'fn NAME' FILE` (say when the length had to be guessed)
- shape: `ast-grep run -l rust -p 'NAME($$$A)' DIR`, `ast-grep run -l rust -p '$X.NAME($$$A)' DIR`,
  `ast-grep scan --inline-rules '…'` for has/inside rules
- trees: `find DIR -maxdepth 2 -type d`, `find DIR -name '*.rs' -exec stat -f '%z %N' {} + | sort -rn | head`
- history: `git log --oneline -5 -- PATH`, `git log -S TERM --oneline`

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

**Report:** the table, a one-line verdict per task (Octocode / expert / even), every Octocode defect with the
exact call that shows it, and what you did not check. Delete anything you wrote before you report.
