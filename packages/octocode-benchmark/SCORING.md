# Scoring (by hand)

Grader prose is the evidence. Score each answer, per question, on four things:

- **Correctness** — did it answer every material part? (0–10)
- **Research depth** — how well the evidence supports the answer (1–5)
- **Workflow** — how clean the path was: right calls, no wasted or redundant work (1–5)
- **Chars in/out** — characters pulled into context (raw CLI output), per question

**Decide with correctness first.** Compare Octocode (B) to baseline (A):

- B clearly higher on correctness → B wins; clearly lower → B loses
- essentially equal → **tie**, broken by efficiency: fewer chars in/out at equal correctness

A confidently-wrong answer (major false confidence) blocks a win regardless of efficiency. One full pass is a snapshot; repeat it for a stable result.
