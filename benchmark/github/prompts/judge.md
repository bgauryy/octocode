# Judge agent prompt — github benchmark

Use the **[unified judge](../../judge/prompt.md)** for this benchmark.

Fill in these placeholder values before pasting:

```
AGENTS:    octocode, gh
RUNS:      <absolute path to benchmark/github/output/octocode>,
           <absolute path to benchmark/github/output/gh>
QUESTIONS: <absolute path to benchmark/github/QUESTIONS.md>
OUTPUT:    <absolute path to benchmark/github/output/summary.md>
```

---

## Benchmark-specific scoring notes

The unified judge applies its standard quality + depth + turns scoring. For this benchmark, also apply these capability-specific notes when assigning scores:

### Quality notes

**SEARCH result limits (Q1, Q4):**
If an agent gives an incomplete count and disclosed a tool cap, score proportionally to supported facts and note the cap. If the count is incomplete and undisclosed, score Q=1.

**CONTENT large files (Q7):**
Files over the GitHub `/contents/` inline size limit (1MB) require blob retrieval or char-offset pagination. If an agent cannot answer because it hit the size limit, score Q=0 only if it did not acknowledge the limitation; score Q=1 if it disclosed the cap and answered partially.

**PR inline comments vs PR-level review summaries (Q13):**
`gh pr view --json reviews` returns PR-level summaries, not inline thread comments. If an agent answers from summaries when inline comments were required, treat the missing inline comments as a missing load-bearing fact (Q≤2).

### Depth notes

**SEARCH questions (Q1–Q4):** D=3 requires file:line for every match, not just a count or file list.

**CONTENT targeted reads (Q5, Q6):** D=3 requires the exact function names and signatures, not just a description of the file.

**STRUCTURE enumeration (Q9–Q11):** D=3 requires a complete enumerated list of all items, with counts verified.

**PR questions (Q12–Q15):** D=3 requires PR number + body excerpt + at minimum the specific requested sub-facts (commit SHAs, inline comment quote, file-change list). PR number alone = D≤1.

### Turns note

For this benchmark, `calls` from q<n>.json is the turns count. Neither agent has LLM-level turn data. Report `calls` as T in the per-question table.

---

## Output path

```
benchmark/github/output/summary.md
```
