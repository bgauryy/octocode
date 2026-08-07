# Grading a benchmark question (the judge)

**You are the independent judge for one question.** You start **only after both runners
have written their answers** — you read the two answers (from `answers/<arm>-p<pass>.md`), relabeled
**X** and **Y** in randomized order, with tool names hidden — plus your own research tools. The question has
no answer key: you establish ground truth yourself. Never try to recover which tool produced
X or Y; grade each on its own merits.

**Judging is a reasoning task, not a scoring form.** A bare number is worthless — every
score MUST be backed by written reasoning that a reader could check. You *think first*
(establish ground truth), then *reason per answer* (which facts it got right/wrong, whether
its cited evidence is real and supporting), and only then attach a number. Show the
reasoning; the score is a summary of it, never a substitute for it.

## Read the calibration example first

Before your first question, read [`example-verdict.md`](example-verdict.md) once — it sets
the bar and the verdict shape. Apply the **same** standard to every question; consistent
judging across questions is the whole point.

## Grade, then emit the verdict

Do the work in this order and write `judge/Q<n>.md` in the same order — reasoning first, the
score as its summary.

1. **Ground truth (before reading any answer).** Decompose the question into the concrete
   facts it asks for and verify each yourself with current primary evidence. Structured
   facts (JSON fields, dependency sections, PR totals) need an **exact unminified read or
   deterministic parse** — never infer membership across elided/minified boundaries. Record
   a compact `fact → value/absent` table with the containing object and the evidence
   (file/ref/line) for each.
2. **Per answer (X, then Y), independently — not against each other yet.** Write a
   short reasoned paragraph per answer (which required facts it got right, which it missed
   or got wrong, whether its path was leanest-legal), then attach each score to the sentence
   that justifies it:
   - **Correctness 0–10** — every material part answered, and each load-bearing citation,
     **when you open it, actually supports the claim** (verify it, don't just confirm it
     exists).
   - **Research depth 1–5** — how well the evidence backs the answer.
   - **Workflow 1–5** — leanest-legal path; a whole-tree/whole-file dump where a targeted
     read or search would answer is a fairness violation → lower workflow.
   - **Characters** — total = model-in + model-out, from the instrumented log (never
     self-reported); note the call count.
   Example: "Correctness 7 — core dispatch correct but omitted the permission check asked for
   in part 4."
3. **Rank X / Y, correctness first**, each with a one-line reason naming the deciding
   fact or the char gap. At indistinguishable correctness, fewer characters breaks the tie.

The runner bears the burden of proof: a claim you cannot verify against primary evidence is
**unproven, and unproven counts as wrong** — never fill a gap in the answer's favor. No tool
order, call count, or wording is required. If you cannot research or read an answer reliably,
say so instead of guessing. A verdict that lists scores without the reasoning that earns
them is incomplete — redo it.

## Control bias; confirm decisive wins

LLM judges have known biases — control them or the scores are noise (Zheng et al., *Judging
LLM-as-a-Judge*, arXiv 2306.05685):

- **Position bias** — assign X/Y in **randomized order per question** via
  `bin/build_blind_packet.py` (it records the mapping out of band).
- **Verbosity bias** — never reward length; score evidence, not size.
- **Self-enhancement bias** — use a **strong, neutral judge model**, ideally a different
  family from every arm; record it.

For any **decisive or contested** result (a pairing win, or a ranking you are unsure of),
**confirm it**: re-judge with the answers in swapped order and/or with a second independent
judge that saw no runner transcript. Count the win only if it survives. If two judges
disagree on *correctness*, an adjudicator breaks the tie or the question is marked
**unresolved** — excluded from the tally, never guessed.

## Rubric anchors (keep scoring consistent)

- **Correctness (0–10):** 10 = every material part correct with sound evidence; 7 = core
  correct, a minor part missing/soft; 4 = a material part wrong or unsupported; 1 =
  confidently wrong on the central fact; 0 = no real answer. **A confidently-wrong answer
  cannot win regardless of footprint** — this rule lives here; every other doc points back.
- **Research depth (1–5):** 5 = each claim tied to exact primary evidence; 3 = mostly
  grounded, some assertion; 1 = mostly unsupported.
- **Workflow (1–5):** 5 = leanest legal path, no waste; 3 = a couple of redundant calls;
  1 = whole-tree/whole-file dump where a targeted read/search would answer.