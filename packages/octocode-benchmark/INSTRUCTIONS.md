# Run a benchmark (by hand)

No harness. You (or three separate agents) work through the markdown questions and write a markdown result.

## Steps

1. **Pick a comparison** under `compare/` and read its `README.md` (the two arms and the allowed surface for each).
2. **Freeze mutable state first.** For each question, resolve every branch/PR-state/SHA the prompt depends on, note the resolved SHA + UTC, and use those frozen refs in the answer.
3. **For each `questions/Q<n>.md`, three independent passes:**
   - **Runner A** answers using the baseline CLI only. Record each command and the characters it pulled in.
   - **Runner B** answers using `npx octocode tools …` only. Same recording.
   - **Grader** reads both answers *blind* (don't reveal which tool produced which), establishes the facts by independent research, and grades each answer on its own.
4. **Write it up** in `results/<comparison-id>-<name>.md` using [REPORT_TEMPLATE.md](REPORT_TEMPLATE.md) — per-question table + summary of all.

## Rules that keep it fair

- Same question, same agreed budget (max tool calls you set for the run), same frozen refs for both arms.
- Give each arm its **leanest legitimate path** — targeted reads, filtered views, concise output. Handicapping either arm (e.g. a full-file pull where a search snippet would answer) distorts the comparison; note any known suboptimality in the write-up.
- Questions carry **no answer key**. The grader establishes ground truth by its own research; if the evidence can't resolve a point, it says so.
- Grade **semantic support**, not exact wording, length, citations, or tool order.
- Octocode arm = `npx octocode tools <tool> …` only.

## Add a question

Create `questions/Q<n>.md` with **only** a title, an `id`, and a `## Question`. Nothing else — no scope, budget, hints, claims, or reference (those would bias the run). The question must be self-contained and objectively checkable:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Name the repo(s)/ref(s) or $CORPUS path and state exactly what to report.
```

Add its row to `questions/README.md`. Keep any expected answer only in your own head / the write-up — never in the file.
