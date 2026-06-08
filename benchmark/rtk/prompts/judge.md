# Judge agent prompt — rtk benchmark

Use the **[unified judge](../../judge/prompt.md)** for this benchmark.

Fill in these placeholder values before pasting:

```
AGENTS:    octocode, rtk
RUNS:      <absolute path to benchmark/rtk/output/octocode>,
           <absolute path to benchmark/rtk/output/rtk>
QUESTIONS: <absolute path to benchmark/rtk/QUESTIONS.md>
OUTPUT:    <absolute path to benchmark/rtk/output/summary.md>
```

---

## Benchmark-specific scoring notes

The unified judge applies its standard quality + depth + turns scoring. For this benchmark, also apply these capability-specific notes when assigning scores:

### Quality notes

**Comment preservation (Q3, Q4, Q5, Q19):**
If an answer is missing information that exists only in source comments (TODO text, doc comment intent, SAFETY annotations, architecture rationale), score according to how much of the requested fact pattern is still supported. Note the specific comment text that was missed.

**Result completeness (Q1, Q2, Q15):**
If an agent provides a count without disclosing it may be incomplete (due to `rtk rg` result caps), independently verify the count. If it is incomplete and undisclosed, score Q=1. If the agent noted a tool cap, score proportionally.

**PR metadata coverage (Q10, Q11, Q12, Q17):**
Missing labels, missing PR comment content, or missing PR discussion points are missing load-bearing facts (Q≤2). If ALL three of labels, comments, and file-change list are absent, Q=1.

**Remote content breadth (Q13, Q14, Q20):**
If an answer is visibly incomplete because `rtk` was capped at its 2000-char passthrough window, note the cap and score proportionally.

**Out-of-scope capabilities:**
RTK does not support npm registry lookup (Q18) or LSP-style symbol resolution. If the rtk researcher correctly reports a capability as out of scope, this is honesty (D credit) not a quality penalty beyond the missing facts.

### Depth notes

**Code search questions (Q1, Q2, Q15):** D=3 requires file:line for every call site. Result caps that prevent exhaustive listing cap D at 2.

**Comment questions (Q3, Q4, Q5, Q19):** D=3 requires the exact comment text quoted, not a paraphrase or inference from code logic alone.

**PR questions (Q10, Q11, Q12, Q17):** D=3 requires PR number + body + at least one comment/review quote where the question calls for it.

**Architecture / documentation questions (Q3, Q5, Q7):** D=3 requires quoting the relevant section, not summarizing it.

### Turns note

For this benchmark, `calls` from q<n>.json is the turns count. Neither agent has LLM-level turn data. Report `calls` as T in the per-question table.

An agent that makes 16 tool calls to answer a question that octocode answers in 1 has higher `turns_per_point` — a visible cost even if quality is the same.

---

## Capability categories

Use these category labels in the per-question table (same as the benchmark README):

| Label | Questions |
|---|---|
| Comment Preservation | Q3, Q4, Q5, Q19 |
| Result Completeness | Q1, Q2, Q15 |
| PR Metadata | Q10, Q11, Q12, Q17 |
| Remote Content | Q13, Q14, Q20 |
| Directory Structure | Q6, Q7, Q8, Q9 |
| File Metadata | Q8, Q9 |
| Package Registry | Q18 |
| Cross-cutting | Q16 |

---

## Output path

```
benchmark/rtk/output/summary.md
```
