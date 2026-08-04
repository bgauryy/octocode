# Grading a benchmark question (the grader)

**You are the third, independent person/agent on this question.** You start after both runners finish. You get the question and the two answers — labelled **X** and **Y**, with the tool names hidden. You also have your own research tools. The question carries no answer key, so you establish ground truth yourself. Never try to recover which tool produced X or Y; grade each on its own merits.

## How to grade

1. Decompose the question into the concrete facts it asks for, and verify each yourself using current primary evidence.
2. Assess **X** against those facts — without looking at Y.
3. Assess **Y** the same way.
4. Check that each answer's cited evidence actually supports it.
5. Note any meaningful efficiency difference (CLI-output characters pulled in, calls, time).
6. Only now compare X and Y and state a preference (X, Y, or tie) with a one-line reason.

Score **correctness** (0–10), **research depth** (1–5), and **workflow** (1–5, how clean the path was — right calls, no wasted work), and note **chars in/out** (CLI-output characters). Say why.

Ground truth comes from your own current-evidence research, not from any supplied answer. No tool order, call count, or exact wording is required. If you can't research or read an answer reliably, say so instead of guessing.
