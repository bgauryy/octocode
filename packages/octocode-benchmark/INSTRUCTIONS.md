# Run a benchmark (by hand)

No harness. You (or three separate agents) work through the markdown questions and write a markdown result. These are the operational steps; the flow's design and rationale (why blind + isolated) live once in [BENCHMARK.md](BENCHMARK.md).

## Steps

1. **Pick a comparison** under `compare/` and read its `README.md` (the two arms and the allowed surface for each). Its questions are either the shared [`compare/github-questions/`](compare/github-questions/) (the three GitHub matchups) or the matchup's own `questions/` (corpus-local matchups).
2. **Freeze mutable state first.** For each question, resolve every branch/PR-state/SHA the prompt depends on, note the resolved SHA + UTC, and use those frozen refs in the answer.
3. **Seal each runner's initial context.** Give it [`RUNNER.md`](RUNNER.md), the
   matchup rules, the assigned-arm section from
   [`RUNNER_TOOL_CONTEXT.md`](RUNNER_TOOL_CONTEXT.md), and the question—nothing
   from the competing arm or grader. Keep that primer identical across all
   questions and passes and record the tool versions.
4. **For each `Q<n>.md`, three independent passes:**
   - **Runner A** answers using the baseline CLI only. Record each command (chars the model wrote = model-out) and the characters it pulled in (model-in), plus the final answer chars; total = in + out.
   - **Runner B** answers using `npx octocode tools …` only. Same recording (model-in + model-out).
   - **Grader** reads both answers *blind* (don't reveal which tool produced which), establishes the facts by independent research, and grades each answer on its own.
5. **Write it up** in `results/<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` (comparison name, run start time `HHMMSS`, then date — e.g. `results/octocode-vs-gh-rtk-021054-2026-08-05.md`) using [REPORT_TEMPLATE.md](REPORT_TEMPLATE.md) — per-question table + summary of all.

## Rules that keep it fair

- Same question, same agreed budget (max tool calls you set for the run), same frozen refs for both arms.
- Give each arm its **leanest legitimate path** — targeted reads, filtered views, concise output. Handicapping either arm (e.g. a full-file pull where a search snippet would answer) distorts the comparison; note any known suboptimality in the write-up.
- Questions carry **no answer key**. The grader establishes ground truth by its own research; if the evidence can't resolve a point, it says so.
- Grade **semantic support**, not exact wording, length, citations, or tool order.
- Octocode arm = `npx octocode tools <tool> …` only.
- Fixed tool primers are setup context and excluded from CLI-output character
  totals. Any help/catalog/schema command issued during research is counted.

## Add a question

Put it in the right set — shared [`compare/github-questions/`](compare/github-questions/) for GitHub questions, or the matchup's own `questions/` for corpus-local ones. Create `Q<n>.md` with **only** a title, an `id`, and a `## Question`. Nothing else — no scope, budget, hints, claims, or reference (those would bias the run). The question must be self-contained and objectively checkable:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Name the repo(s)/ref(s) or $CORPUS path and state exactly what to report.
```

Add its row to that set's `README.md` index. Keep any expected answer only in your own head / the write-up — never in the file.
